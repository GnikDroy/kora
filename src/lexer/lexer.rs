use super::errors::LexerErr;
use super::token::{Keyword, Symbol, Token, TokenInfo};

pub struct Lexer {}

impl Lexer {
    pub fn lex(source: &str) -> Result<Vec<TokenInfo>, LexerErr> {
        let mut tokens = Vec::new();
        let lines = source.split(|b| b == '\n');
        for (col, line) in lines.enumerate() {
            let mut line_iter = line.chars().enumerate().peekable();
            while let Some((row, c)) = line_iter.next() {
                let tok = match c {
                    '(' | ')' | '[' | ']' | '{' | '}' | ';' | ':' | ',' | '+' | '-' | '*' | '/'
                    | '|' | '&' => Ok(Token::Symbol(match c {
                        '(' => Symbol::LeftParen,
                        ')' => Symbol::RightParen,
                        '[' => Symbol::LeftBracket,
                        ']' => Symbol::RightBracket,
                        '{' => Symbol::LeftBrace,
                        '}' => Symbol::RightBrace,
                        ';' => Symbol::Semicolon,
                        ':' => Symbol::Colon,
                        ',' => Symbol::Comma,
                        '+' => Symbol::Plus,
                        '-' => Symbol::Minus,
                        '*' => Symbol::Star,
                        '/' => Symbol::Slash,
                        '|' => Symbol::Pipe,
                        '&' => Symbol::Ampersand,
                        _ => panic!(),
                    })),
                    '=' => {
                        if let Some((_, c1)) = line_iter.peek() && c1 == &'=' {
                            line_iter.next();
                            Ok(Token::Symbol(Symbol::EqualEqual))
                        } else {
                            Ok(Token::Symbol(Symbol::Equal))
                        }
                    }
                    '!' => {
                        if let Some((_, c1)) = line_iter.peek() && c1 == &'=' {
                            line_iter.next();
                            Ok(Token::Symbol(Symbol::ExclamEqual))
                        } else {
                            Ok(Token::Symbol(Symbol::Exclam))
                        }
                    }
                    '>' => {
                        if let Some((_, c1)) = line_iter.peek() && c1 == &'=' {
                            line_iter.next();
                            Ok(Token::Symbol(Symbol::GreaterEqual))
                        } else {
                            Ok(Token::Symbol(Symbol::Greater))
                        }
                    }
                    '<' => {
                        if let Some((_, c1)) = line_iter.peek() && c1 == &'=' {
                            line_iter.next();
                            Ok(Token::Symbol(Symbol::LessEqual))
                        } else {
                            Ok(Token::Symbol(Symbol::Less))
                        }
                    }
                    '0'..='9' => {
                        let mut is_real = false;
                        let mut literal = String::new();
                        literal.push(c);
                        while let Some((_, c1)) = line_iter.peek() {
                            match c1 {
                                '0'..='9' => {
                                    literal.push(*c1);
                                    line_iter.next();
                                }
                                '.' => {
                                    literal.push('.');
                                    line_iter.next();
                                    is_real = true;
                                    while let Some((_, c2)) = line_iter.peek() {
                                        match c2 {
                                            '0'..='9' => {
                                                literal.push(*c2);
                                                line_iter.next();
                                            }
                                            _ => break,
                                        }
                                    }
                                    break;
                                }
                                _ => break,
                            }
                        }

                        if is_real {
                            match literal.parse::<f64>() {
                            Ok(n) => Ok(Token::RealLiteral(n)),
                            Err(_) => Err(LexerErr{
                                msg: "Real number cannot be fit in 64 bits",
                                col: col + 1,
                                row: row + 1,
                                line: line.into(),
                                suggestion: "Select a smaller / less precise real number, or a different data type".to_string(),
                            })
                        }
                        } else {
                            match literal.parse::<isize>() {
                            Ok(n) => Ok(Token::IntegerLiteral(n)),
                            Err(_) => Err(LexerErr {
                                msg: "Integer too big to fit in 64 bits",
                                col: col + 1,
                                row: row + 1,
                                line: line.into(),
                                suggestion: "Select a smaller value for the integer, or a different data type".to_string(),
                            }),
                        }
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
                                                    error = Some(LexerErr {
                                                        msg: "Invalid escape sequence",
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
                                            error = Some(LexerErr {
                                                msg: "Incomplete escape sequence",
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
                            error = Some(LexerErr {
                                msg: "Incomplete string literal",
                                col: col + 1,
                                row: row + 1,
                                line: line.to_string(),
                                suggestion: "Did you miss a quotation mark '\"'?".to_string(),
                            });
                        }

                        if error.is_none() {
                            Ok(Token::StringLiteral(literal))
                        } else {
                            Err(error.unwrap())
                        }
                    }
                    'A'..='Z' | 'a'..='z' | '_' => {
                        let mut identifier = String::new();
                        identifier.push(c);

                        while let Some((_, c1)) = line_iter.peek() {
                            match c1 {
                                'A'..='Z' | 'a'..='z' | '_' | '0'..='9' => {
                                    identifier.push(*c1);
                                    line_iter.next();
                                }
                                _ => {
                                    break;
                                }
                            }
                        }

                        match Keyword::map(identifier.as_str()) {
                            Some(k) => Ok(Token::Keyword(k)),
                            _ => Ok(Token::Identifier(identifier)),
                        }
                    }
                    ' ' | '\t' | '\n' | '\r' => Ok(Token::Whitespace),
                    _ => Err(LexerErr {
                        msg: "Invalid token",
                        col: col + 1,
                        row: row + 1,
                        line: line.to_string(),
                        suggestion: "Did you perhaps forget to enclose it in quotes?".to_string(),
                    }),
                };

                let tok = tok?;
                if tok != Token::Whitespace {
                    tokens.push(TokenInfo {
                        token: tok,
                        col: col + 1,
                        row: row + 1,
                    });
                }
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
            let result = super::Lexer::lex(source);
            assert!(result.is_err(), "{:?}", result);
        }
    }

    #[test]
    fn valid() {
        let sources = vec![
            "1",
            "1981723.234424",
            "identz",
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
            let result = super::Lexer::lex(source);
            assert!(result.is_ok(), "{:?}", result);
        }
    }
}
