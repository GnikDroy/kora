mod errors;
mod token;

use std::iter::Peekable;

pub use errors::*;
pub use token::*;

/// Char stream over the whole source; remembers the last consumed position so
/// tokens can span newlines and end-spans stay exact.
struct Cursor<I: Iterator<Item = (LexerContext, char)>> {
    iter: Peekable<I>,
    last: LexerContext,
}

impl<I: Iterator<Item = (LexerContext, char)>> Cursor<I> {
    fn new(iter: I) -> Self {
        Cursor {
            iter: iter.peekable(),
            last: LexerContext::default(),
        }
    }

    fn next(&mut self) -> Option<(LexerContext, char)> {
        let item = self.iter.next();
        if let Some((ctx, _)) = &item {
            self.last = ctx.clone();
        }
        item
    }

    fn peek(&mut self) -> Option<&(LexerContext, char)> {
        self.iter.peek()
    }
}

#[derive(Default)]
pub struct Lexer;

impl Lexer {
    fn consume_whitespace(
        _: &LexerContext,
        _: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        Ok(Token::Whitespace)
    }

    fn consume_nothing_with_error(
        context: &LexerContext,
        _: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        Err(LexerErr {
            msg: "Invalid token",
            context: context.clone(),
            suggestion: "Did you perhaps forget to enclose it in quotes?".to_string(),
        })
    }

    fn consume_single_symbol(
        context: &LexerContext,
        _: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        Symbol::try_from(first)
            .map(Token::Symbol)
            .map_err(|_| LexerErr {
                msg: "Invalid symbol",
                context: context.clone(),
                suggestion: "This character is not a recognized symbol".to_string(),
            })
    }

    fn consume_double_symbol(
        context: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        if let Some((_, next)) = cursor.peek() {
            let symbol = Symbol::try_from([first, *next].iter().collect::<String>().as_str());
            if let Ok(symbol) = symbol {
                cursor.next();
                return Ok(Token::Symbol(symbol));
            }
        }
        Lexer::consume_single_symbol(context, cursor, first)
    }

    /// `#` line comment, skipped to end of line (lexed as `Whitespace`, dropped).
    fn consume_comment(
        _: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        while let Some((_, c)) = cursor.peek() {
            if *c == '\n' {
                break;
            }
            cursor.next();
        }
        Ok(Token::Whitespace)
    }

    fn consume_number(
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        first: char,
    ) -> String {
        let mut literal = String::new();
        literal.push(first);
        while let Some((_, c)) = cursor.peek() {
            match c {
                '0'..='9' => {
                    literal.push(*c);
                    cursor.next();
                }
                _ => break,
            }
        }
        literal
    }

    fn consume_identifier_and_keyword(
        _: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        let mut identifier = String::new();
        identifier.push(first);

        while let Some((_, c)) = cursor.peek() {
            match c {
                'A'..='Z' | 'a'..='z' | '_' | '0'..='9' => {
                    identifier.push(*c);
                    cursor.next();
                }
                _ => {
                    break;
                }
            }
        }

        match Keyword::try_from(identifier.as_str()) {
            Ok(v) => Ok(Token::Keyword(v)),
            _ => Ok(Token::Identifier(identifier)),
        }
    }

    fn consume_char_escape_code(
        context: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<u8, LexerErr> {
        if let Some((_, escape)) = cursor.peek() {
            let byte = match escape {
                '\\' => b'\\',
                '\'' => b'\'',
                '"' => b'"',
                'n' => b'\n',
                't' => b'\t',
                'r' => b'\r',
                '0' => b'\0',
                _ => {
                    return Err(LexerErr {
                        msg: "Invalid escape sequence",
                        context: context.clone(),
                        suggestion: "Valid sequences are [\\n, \\t, \\r, \\0, \\\\, \\', \\\"]"
                            .to_string(),
                    });
                }
            };
            cursor.next();
            Ok(byte)
        } else {
            Err(LexerErr {
                msg: "Incomplete escape sequence",
                context: context.clone(),
                suggestion: "Valid sequences are [\\n, \\t, \\r, \\0, \\\\, \\', \\\"]".to_string(),
            })
        }
    }

    fn consume_char_literal(
        context: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        if let Some((_, c)) = cursor.next()
            && c != '\''
        {
            let byte = match c {
                _ if c.len_utf8() != 1 => {
                    return Err(LexerErr {
                        msg: "Char literals must only occupy one byte",
                        context: context.clone(),
                        suggestion: "Perhaps you need a string literal? ' -> \"".to_string(),
                    });
                }
                _ if c == '\\' => Lexer::consume_char_escape_code(context, cursor, c)?,
                _ => *c.to_string().as_bytes().first().unwrap(),
            };

            if let Some((_, quote)) = cursor.next()
                && quote == '\''
            {
                Ok(Token::CharLiteral(byte))
            } else {
                Err(LexerErr {
                    msg: "Char literals must end in '",
                    context: context.clone(),
                    suggestion: "Did you miss a '?".to_string(),
                })
            }
        } else {
            Err(LexerErr {
                msg: "Incomplete char literal",
                context: context.clone(),
                suggestion: "Did you forget to include a character?".to_string(),
            })
        }
    }

    fn consume_string_escape_code(
        context: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<char, LexerErr> {
        if let Some((_, escape)) = cursor.peek() {
            let ch = match escape {
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '0' => '\0',
                _ => {
                    return Err(LexerErr {
                        msg: "Invalid escape sequence",
                        context: context.clone(),
                        suggestion: "Valid sequences are [\\n, \\t, \\r, \\0, \\\\, \\', \\\"]"
                            .to_string(),
                    });
                }
            };
            cursor.next();
            Ok(ch)
        } else {
            Err(LexerErr {
                msg: "Incomplete escape sequence",
                context: context.clone(),
                suggestion: "Valid sequences are [\\n, \\t, \\r, \\0, \\\\, \\', \\\"]".to_string(),
            })
        }
    }

    fn consume_string_literal(
        context: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        let mut literal = String::new();
        while let Some((_, c)) = cursor.peek() {
            let c = *c;
            cursor.next();
            match c {
                '"' => {
                    return Ok(Token::StringLiteral(literal));
                }
                _ => {
                    let c = if c == '\\' {
                        Lexer::consume_string_escape_code(context, cursor, c)?
                    } else {
                        c
                    };
                    literal.push(c);
                }
            }
        }

        Err(LexerErr {
            msg: "Incomplete string literal",
            context: context.clone(),
            suggestion: "Did you miss a quotation mark '\"'?".to_string(),
        })
    }

    fn consume_numeric(
        context: &LexerContext,
        cursor: &mut Cursor<impl Iterator<Item = (LexerContext, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        let mut literal = Lexer::consume_number(cursor, first);

        if let Some((_, '.')) = cursor.peek() {
            cursor.next();
            literal += Lexer::consume_number(cursor, '.').as_str();
            return literal
                .parse::<f64>()
                .map(Token::RealLiteral)
                .map_err(|_| LexerErr {
                    msg: "Real number cannot be fit in 64 bits",
                    context: context.clone(),
                    suggestion:
                        "Select a smaller / less precise real number, or a different data type"
                            .to_string(),
                });
        }

        literal
            .parse::<isize>()
            .map(Token::IntegerLiteral)
            .map_err(|_| LexerErr {
                msg: "Integer too big to fit in 64 bits",
                context: context.clone(),
                suggestion: "Select a smaller value for the integer, or a different data type"
                    .to_string(),
            })
    }

    pub fn lex(source: &str) -> Result<Vec<TokenInfo>, LexerErr> {
        let mut tokens = vec![];

        // Continuous `(position, char)` stream with 1-based `(row, col)`.
        let mut row: isize = 1;
        let mut col: isize = 1;
        let stream = source.chars().map(move |c| {
            let context = LexerContext { row, col };
            if c == '\n' {
                row += 1;
                col = 1;
            } else {
                col += 1;
            }
            (context, c)
        });

        let mut cursor = Cursor::new(stream);
        while let Some((start_context, c)) = cursor.next() {
            let consumer = match c {
                ' ' | '\t' | '\n' | '\r' => Lexer::consume_whitespace,
                '0'..='9' => Lexer::consume_numeric,
                '"' => Lexer::consume_string_literal,
                '\'' => Lexer::consume_char_literal,
                '#' => Lexer::consume_comment,
                '=' | '!' | '>' | '<' | '&' | '|' => Lexer::consume_double_symbol,
                'A'..='Z' | 'a'..='z' | '_' => Lexer::consume_identifier_and_keyword,
                c if Symbol::try_from(c).is_ok() => Lexer::consume_single_symbol,
                _ => Lexer::consume_nothing_with_error,
            };

            let token = consumer(&start_context, &mut cursor, c)?;

            // End span is the last char the consumer ate.
            let end_context = cursor.last.clone();

            if token != Token::Whitespace {
                tokens.push(TokenInfo {
                    token,
                    start: start_context,
                    end: end_context,
                });
            }
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid() {
        let source = concat!(
            "return let if else while void int real char bool true false extern as struct new",
            " ( ) { } [ ] ; : , + - / % = > < ! . & | ^",
            " == >= <= != || && << >>",
            " 42 3.1415",
            " \"字\"",
            " 'a' '\\'' '\\\\'",
            " identifiers _id_2000",
        );
        let result = super::Lexer::lex(source);
        assert!(
            result.is_ok() && source.split(' ').count() == result.as_ref().unwrap().len(),
            "result: {:?}",
            result
        );
    }

    #[test]
    fn test_invalid() {
        let sources = [
            "?",
            "'字'",
            "'a",
            "'\\'",
            "'\\\\",
            r#""incomplete"#,
            r#""incomplete"#,
            r#""wrong escape sequence \a""#,
            r#""incomplete escape sequence \ "#,
            "99999999999999999999999",
        ];

        for source in sources {
            let result = super::Lexer::lex(source);
            assert!(result.is_err(), "{:?}", result);
        }
    }

    #[test]
    fn test_escape_sequences() {
        let toks = Lexer::lex(r"'\n' '\t' '\r' '\0'").unwrap();
        let bytes: Vec<u8> = toks
            .iter()
            .filter_map(|t| match t.token {
                Token::CharLiteral(b) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(bytes, vec![b'\n', b'\t', b'\r', b'\0']);

        let toks = Lexer::lex(r#""a\nb\tc""#).unwrap();
        match &toks[0].token {
            Token::StringLiteral(s) => assert_eq!(s, "a\nb\tc"),
            other => panic!("expected string literal, got {:?}", other),
        }
    }

    #[test]
    fn test_comments() {
        let source = "let # trailing comment\nx = 1";
        let toks = Lexer::lex(source).unwrap();
        let kinds: Vec<&Token> = toks.iter().map(|t| &t.token).collect();
        assert_eq!(
            kinds,
            vec![
                &Token::Keyword(Keyword::Let),
                &Token::Identifier("x".to_string()),
                &Token::Symbol(Symbol::Equal),
                &Token::IntegerLiteral(1),
            ]
        );
    }

    #[test]
    fn test_error_display_includes_position_and_suggestion() {
        let err = Lexer::lex("?").expect_err("expected lex error");
        let rendered = err.to_string();
        assert!(rendered.contains("1:1"), "rendered: {}", rendered);
        assert!(rendered.contains("quotes"), "rendered: {}", rendered);
    }

    #[test]
    fn test_multiline_spans() {
        let toks = Lexer::lex("ab\ncde").unwrap();
        assert_eq!(toks.len(), 2);
        assert_eq!((toks[0].start.row, toks[0].start.col), (1, 1));
        assert_eq!((toks[0].end.row, toks[0].end.col), (1, 2));
        assert_eq!((toks[1].start.row, toks[1].start.col), (2, 1));
        assert_eq!((toks[1].end.row, toks[1].end.col), (2, 3));
    }

    #[test]
    fn test_keywords() {
        let cases = [
            ("return", Keyword::Return),
            ("let", Keyword::Let),
            ("if", Keyword::If),
            ("else", Keyword::Else),
            ("while", Keyword::While),
            ("void", Keyword::Void),
            ("int", Keyword::Int),
            ("real", Keyword::Real),
            ("char", Keyword::Char),
            ("bool", Keyword::Bool),
            ("true", Keyword::True),
            ("false", Keyword::False),
            ("extern", Keyword::Extern),
            ("as", Keyword::As),
            ("struct", Keyword::Struct),
            ("new", Keyword::New),
            ("for", Keyword::For),
            ("break", Keyword::Break),
            ("continue", Keyword::Continue),
        ];
        for (source, keyword) in cases {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(toks[0].token, Token::Keyword(keyword), "source: {}", source);
        }
    }

    #[test]
    fn test_identifiers_are_not_keywords() {
        for source in ["lets", "ifx", "_return", "int2", "While", "forloop", "_"] {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(
                toks[0].token,
                Token::Identifier(source.to_string()),
                "source: {}",
                source
            );
        }
    }

    #[test]
    fn test_symbols() {
        let cases = [
            ("(", Symbol::LeftParen),
            (")", Symbol::RightParen),
            ("{", Symbol::LeftBrace),
            ("}", Symbol::RightBrace),
            ("[", Symbol::LeftBracket),
            ("]", Symbol::RightBracket),
            (";", Symbol::Semicolon),
            (":", Symbol::Colon),
            (",", Symbol::Comma),
            ("+", Symbol::Plus),
            ("-", Symbol::Minus),
            ("*", Symbol::Star),
            ("/", Symbol::Slash),
            ("%", Symbol::Percent),
            ("=", Symbol::Equal),
            ("==", Symbol::EqualEqual),
            (">", Symbol::Greater),
            ("<", Symbol::Less),
            (">=", Symbol::GreaterEqual),
            ("<=", Symbol::LessEqual),
            ("!", Symbol::Exclam),
            ("!=", Symbol::ExclamEqual),
            ("||", Symbol::PipePipe),
            ("&&", Symbol::AmpersandAmpersand),
            ("&", Symbol::Ampersand),
            ("|", Symbol::Pipe),
            ("^", Symbol::Caret),
            ("<<", Symbol::LessLess),
            (">>", Symbol::GreaterGreater),
            (".", Symbol::Dot),
        ];
        for (source, symbol) in cases {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(toks[0].token, Token::Symbol(symbol), "source: {}", source);
        }
    }

    #[test]
    fn test_double_symbols_are_greedy() {
        let toks = Lexer::lex("==").unwrap();
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].token, Token::Symbol(Symbol::EqualEqual));

        let toks = Lexer::lex("= =").unwrap();
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].token, Token::Symbol(Symbol::Equal));
        assert_eq!(toks[1].token, Token::Symbol(Symbol::Equal));

        let toks = Lexer::lex("!x").unwrap();
        assert_eq!(toks[0].token, Token::Symbol(Symbol::Exclam));
        assert_eq!(toks[1].token, Token::Identifier("x".to_string()));
    }

    #[test]
    fn test_integer_literals() {
        let cases = [("0", 0), ("42", 42), ("007", 7), ("1000000", 1000000)];
        for (source, value) in cases {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(
                toks[0].token,
                Token::IntegerLiteral(value),
                "source: {}",
                source
            );
        }
    }

    #[test]
    fn test_real_literals() {
        // A trailing dot with no fractional digits is still a real.
        let cases = [("3.14", 3.14), ("0.0", 0.0), ("3.", 3.0), ("42.5", 42.5)];
        for (source, value) in cases {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(
                toks[0].token,
                Token::RealLiteral(value),
                "source: {}",
                source
            );
        }

        // A leading dot is a separate `.` symbol, not part of the number.
        let toks = Lexer::lex(".5").unwrap();
        assert_eq!(toks[0].token, Token::Symbol(Symbol::Dot));
        assert_eq!(toks[1].token, Token::IntegerLiteral(5));

        // A second dot terminates the real and starts a new token.
        let toks = Lexer::lex("1.2.3").unwrap();
        assert_eq!(toks[0].token, Token::RealLiteral(1.2));
        assert_eq!(toks[1].token, Token::Symbol(Symbol::Dot));
        assert_eq!(toks[2].token, Token::IntegerLiteral(3));
    }

    #[test]
    fn test_char_literals() {
        let cases = [("'a'", b'a'), ("'Z'", b'Z'), ("'0'", b'0'), ("' '", b' ')];
        for (source, value) in cases {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(
                toks[0].token,
                Token::CharLiteral(value),
                "source: {}",
                source
            );
        }
    }

    #[test]
    fn test_string_literals() {
        let cases = [
            (r#""hello""#, "hello"),
            (r#""""#, ""),
            (r#""with spaces""#, "with spaces"),
            (r#""字""#, "字"),
        ];
        for (source, value) in cases {
            let toks = Lexer::lex(source).unwrap();
            assert_eq!(toks.len(), 1, "source: {}", source);
            assert_eq!(
                toks[0].token,
                Token::StringLiteral(value.to_string()),
                "source: {}",
                source
            );
        }
    }

    #[test]
    fn test_whitespace_and_comments_produce_no_tokens() {
        for source in [
            "",
            "   ",
            "\t\n\r ",
            "# just a comment",
            "  \n # comment\n  ",
        ] {
            let toks = Lexer::lex(source).unwrap();
            assert!(toks.is_empty(), "source: {:?}, toks: {:?}", source, toks);
        }
    }

    #[test]
    fn test_comment_stops_at_newline() {
        let toks = Lexer::lex("1 # comment\n2").unwrap();
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].token, Token::IntegerLiteral(1));
        assert_eq!(toks[1].token, Token::IntegerLiteral(2));
    }

    #[test]
    fn test_token_spans() {
        // A real literal spans all of its characters.
        let toks = Lexer::lex("3.14").unwrap();
        assert_eq!((toks[0].start.row, toks[0].start.col), (1, 1));
        assert_eq!((toks[0].end.row, toks[0].end.col), (1, 4));

        // A two-char operator spans both characters.
        let toks = Lexer::lex("a == b").unwrap();
        assert_eq!(toks[1].token, Token::Symbol(Symbol::EqualEqual));
        assert_eq!((toks[1].start.row, toks[1].start.col), (1, 3));
        assert_eq!((toks[1].end.row, toks[1].end.col), (1, 4));
    }
}
