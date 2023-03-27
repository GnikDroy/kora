use super::errors::LexerError;
use super::token::{KeywordKind, SymbolKind, Tok, Token};

pub struct Lexer {}

impl Lexer {
    pub fn lex(source: &str) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();
        for (col, line) in source.split(|b| b == '\n').enumerate() {
            let mut line_iter = line.chars().enumerate().peekable();
            while let Some((row, c)) = line_iter.next() {
                let tok = match c {
                    // single character symbols
                    '(' | ')' | '[' | ']' | '{' | '}' | ';' | ':' | ',' | '.' | '+' | '-' | '*'
                    | '/' | '>' | '<' => Ok(Tok::Symbol(match c {
                        '(' => SymbolKind::LeftParen,
                        ')' => SymbolKind::RightParen,
                        '[' => SymbolKind::LeftBracket,
                        ']' => SymbolKind::RightBracket,
                        '{' => SymbolKind::LeftBrace,
                        '}' => SymbolKind::RightBrace,
                        ';' => SymbolKind::Semicolon,
                        ':' => SymbolKind::Colon,
                        ',' => SymbolKind::Comma,
                        '.' => SymbolKind::Dot,
                        '+' => SymbolKind::Plus,
                        '-' => SymbolKind::Minus,
                        '*' => SymbolKind::Star,
                        '/' => SymbolKind::Slash,
                        '>' => SymbolKind::Greater,
                        '<' => SymbolKind::Less,
                        _ => panic!(),
                    })),
                    // double character symbols
                    '=' => {
                        if let Some((_, c1)) = line_iter.peek() {
                            if c1 == &'=' {
                                line_iter.next();
                                Ok(Tok::Symbol(SymbolKind::EqualEqual))
                            } else {
                                Ok(Tok::Symbol(SymbolKind::Equal))
                            }
                        } else {
                            Ok(Tok::Symbol(SymbolKind::Equal))
                        }
                    }
                    '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                        let mut literal = String::new();
                        literal.push(c);

                        while let Some((_, c1)) = line_iter.peek() {
                            match c1 {
                                '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
                                    literal.push(*c1);
                                    line_iter.next();
                                }
                                _ => break,
                            }
                        }
                        match literal.parse::<i64>() {
                            Ok(n) => Ok(Tok::NumericLiteral(n)),
                            Err(_) => Err(LexerError {
                                msg: "Integer too big to fit in 64 bits".to_string(),
                                col: col + 1,
                                row: row + 1,
                                line: line.into(),
                                suggestion: "Select a smaller value for the integer".to_string(),
                            }),
                        }
                    }
                    '"' => {
                        let mut literal = String::new();
                        let mut error = None;
                        let mut string_finished = false;
                        while let Some((_, c1)) = line_iter.peek() {
                            match c1 {
                                '"' => {
                                    line_iter.next();
                                    string_finished = true;
                                    break;
                                }
                                _ => {
                                    if *c1 == '\\' {
                                        line_iter.next();
                                        if let Some((_, c2)) = line_iter.peek() {
                                            match c2 {
                                                '\\' | '"' => {
                                                    literal.push(*c2);
                                                    line_iter.next();
                                                }
                                                _ => {
                                                    error = Some(LexerError {
                                                        msg: "Invalid escape sequence".to_string(),
                                                        col: col + 1,
                                                        row: row + 1,
                                                        line: line.to_string(),
                                                        suggestion:
                                                            "Valid sequences are [\\n, \\\\]"
                                                                .to_string(),
                                                    });
                                                    break;
                                                }
                                            }
                                        } else {
                                            error = Some(LexerError {
                                                msg: "Incomplete escape sequence".to_string(),
                                                col: col + 1,
                                                row: row + 1,
                                                line: line.to_string(),
                                                suggestion: "Valid sequences are [\\n, \\\\]"
                                                    .to_string(),
                                            });
                                            break;
                                        }
                                    } else {
                                        literal.push(*c1);
                                        line_iter.next();
                                    }
                                }
                            }
                        }

                        if !string_finished {
                            error = Some(LexerError {
                                msg: "Incomplete string literal".to_string(),
                                col: col + 1,
                                row: row + 1,
                                line: line.to_string(),
                                suggestion: "Did you miss a quotation mark '\"'?".to_string(),
                            });
                        }

                        if error.is_none() {
                            Ok(Tok::StringLiteral(literal))
                        } else {
                            Err(error.unwrap())
                        }
                    }
                    'A'..'Z' | 'a'..'z' | '_' => {
                        let mut identifier = String::new();
                        identifier.push(c);

                        while let Some((_, c1)) = line_iter.peek() {
                            match c1 {
                                'A'..'Z' | 'a'..'z' | '_' | '0'..'9' => {
                                    identifier.push(*c1);
                                    line_iter.next();
                                }
                                _ => {
                                    break;
                                }
                            }
                        }

                        match KeywordKind::map(identifier.as_str()) {
                            Some(k) => Ok(Tok::Keyword(k)),
                            _ => Ok(Tok::Identifier(identifier)),
                        }
                    }
                    ' ' | '\t' | '\n' | '\r' => Ok(Tok::Whitespace),
                    _ => Err(LexerError {
                        msg: "Invalid token".to_string(),
                        col: col + 1,
                        row: row + 1,
                        line: line.to_string(),
                        suggestion: "Did you perhaps forget to enclose it in quotes?".to_string(),
                    }),
                };

                match tok {
                    Ok(t) => {
                        if t != Tok::Whitespace {
                            tokens.push(Token {
                                token: t,
                                col: col + 1,
                                row: row + 1,
                            });
                        }
                    }
                    Err(e) => {
                        return Err(e);
                    }
                };
            }
        }
        return Ok(tokens);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn invalid() {
        let sources = vec![
            r#"
                bool main() {
                    ret "incomplete string literal
                }
            "#,
            r#"
                bool ident_23() {
                    ret "wrong escape sequence \a";
                }
            "#,
            r#"
                int ident_23() {
                    ret "incomplete escape sequence \
                }
            "#,
            "very_long_int_literal = 9293849128374982734",
            "?",
        ];
        for source in sources {
            assert!(super::Lexer::lex(source).is_err());
        }
    }

    #[test]
    fn valid() {
        let sources = vec![
            "
                int main() {
                    ret 2+4;
                }
            ",
            r#"
                int ident_23(a: int) {
                    let a_2 = "Hello, ";
                    let _b2 = "World";
                    ret a+b;
                }
            "#,
        ];
        for source in sources {
            assert!(super::Lexer::lex(source).is_ok());
        }
    }
}
