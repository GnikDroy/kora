mod errors;
mod token;

use std::iter::Peekable;

pub use errors::*;
pub use token::*;

#[derive(Default)]
pub struct Lexer;

impl Lexer {
    fn consume_whitespace(
        _: &LexerContext,
        _: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        Ok(Token::Whitespace)
    }

    fn consume_nothing_with_error(
        context: &LexerContext,
        _: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        Err(LexerErr {
            msg: "Invalid token",
            context: context.clone(),
            suggestion: "Did you perhaps forget to enclose it in quotes?".to_string(),
        })
    }

    fn consume_single_symbol(
        _: &LexerContext,
        _: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        Ok(Token::Symbol(Symbol::try_from(first).unwrap()))
    }

    fn consume_double_symbol(
        _: &LexerContext,
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        if let Some((_, next)) = line_iter.peek() {
            let symbol = Symbol::try_from([first, *next].iter().collect::<String>().as_str());
            if let Ok(symbol) = symbol {
                line_iter.next();
                return Ok(Token::Symbol(symbol));
            }
        }
        Ok(Token::Symbol(Symbol::try_from(first).unwrap()))
    }

    fn consume_number(
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        first: char,
    ) -> String {
        let mut literal = String::new();
        literal.push(first);
        while let Some((_, c)) = line_iter.peek() {
            match c {
                '0'..='9' => {
                    literal.push(*c);
                    line_iter.next();
                }
                _ => break,
            }
        }
        return literal;
    }

    fn consume_identifier_and_keyword(
        _: &LexerContext,
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        let mut identifier = String::new();
        identifier.push(first);

        while let Some((_, c)) = line_iter.peek() {
            match c {
                'A'..='Z' | 'a'..='z' | '_' | '0'..='9' => {
                    identifier.push(*c);
                    line_iter.next();
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
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        _: char,
    ) -> Result<u8, LexerErr> {
        if let Some((_, escape)) = line_iter.peek() {
            match escape {
                '\\' | '\'' => {
                    let escape = *escape;
                    line_iter.next();
                    Ok(*escape.to_string().as_bytes().first().unwrap())
                }
                _ => Err(LexerErr {
                    msg: "Invalid escape sequence",
                    context: context.clone(),
                    suggestion: "Valid sequences are [\\', \\\\]".to_string(),
                }),
            }
        } else {
            Err(LexerErr {
                msg: "Incomplete escape sequence",
                context: context.clone(),
                suggestion: "Valid sequences are [\\', \\\\]".to_string(),
            })
        }
    }

    fn consume_char_literal(
        context: &LexerContext,
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        if let Some((_, c)) = line_iter.next() && c != '\'' {
            let byte = match c {
                _ if c.len_utf8() != 1 => {
                    return Err(LexerErr {
                        msg: "Char literals must only occupy one byte",
                        context: context.clone(),
                        suggestion: "Perhaps you need a string literal? ' -> \"".to_string(),
                    })
                }
                _ if c == '\\' => Lexer::consume_char_escape_code(context, line_iter, c)?,
                _ => *c.to_string().as_bytes().first().unwrap(),
            };

            if let Some((_, quote)) = line_iter.next() && quote == '\'' {
                Ok(Token::CharLiteral(byte))
            } else {
                println!("Error char: {}", c);
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
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        _: char,
    ) -> Result<char, LexerErr> {
        if let Some((_, escape)) = line_iter.peek() {
            match escape {
                '\\' | '"' => {
                    let escape = *escape;
                    line_iter.next();
                    Ok(escape)
                }
                _ => Err(LexerErr {
                    msg: "Invalid escape sequence",
                    context: context.clone(),
                    suggestion: "Valid sequences are [\\n, \\\\]".to_string(),
                }),
            }
        } else {
            Err(LexerErr {
                msg: "Incomplete escape sequence",
                context: context.clone(),
                suggestion: "Valid sequences are [\\n, \\\\]".to_string(),
            })
        }
    }

    fn consume_string_literal(
        context: &LexerContext,
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        _: char,
    ) -> Result<Token, LexerErr> {
        let mut literal = String::new();
        while let Some((_, c)) = line_iter.peek() {
            let c = *c;
            line_iter.next();
            match c {
                '"' => {
                    return Ok(Token::StringLiteral(literal));
                }
                _ => {
                    let c = if c == '\\' {
                        Lexer::consume_string_escape_code(context, line_iter, c)?
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
        line_iter: &mut Peekable<impl Iterator<Item = (usize, char)>>,
        first: char,
    ) -> Result<Token, LexerErr> {
        let mut literal = Lexer::consume_number(line_iter, first);

        match line_iter.peek() {
            Some((_, '.')) => {
                line_iter.next();
                literal += Lexer::consume_number(line_iter, '.').as_str();
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
            _ => {}
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

        for (row, line) in source.lines().enumerate() {
            let mut line_iter = line.chars().enumerate().peekable();
            while let Some((col, c)) = line_iter.next() {
                let consumer = match c {
                    ' ' | '\t' | '\n' | '\r' => Lexer::consume_whitespace,
                    '0'..='9' => Lexer::consume_numeric,
                    '"' => Lexer::consume_string_literal,
                    '\'' => Lexer::consume_char_literal,
                    '=' | '!' | '>' | '<' => Lexer::consume_double_symbol,
                    'A'..='Z' | 'a'..='z' | '_' => Lexer::consume_identifier_and_keyword,
                    c if Symbol::try_from(c).is_ok() => Lexer::consume_single_symbol,
                    _ => Lexer::consume_nothing_with_error,
                };
                let context = LexerContext {
                    col: col + 1,
                    row: row + 1,
                };

                let token = consumer(&context, &mut line_iter, c)?;
                if token != Token::Whitespace {
                    tokens.push(TokenInfo { token, context });
                }
            }
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn valid() {
        let source = concat!(
            "ret let if else while nil int real char bool true false extern as",
            " ( ) { } [ ] ; : , + - / % = > < ! | &",
            " == >= <= !=",
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
    fn invalid() {
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
}
