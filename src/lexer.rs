use colored::Colorize;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct LexerError {
    msg: String,
    col: usize,
    row: usize,
    line: String,
    suggestion: String,
}

impl Error for LexerError {}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.line.len() < 60 {}
        let error_line = format!("{}: {}\n", "error".red().bold(), self.msg.white().bold());
        let src_line = format!(
            "{} {}:{}\n",
            "-->".blue().bold(),
            self.col.to_string().bold(),
            self.row.to_string().bold()
        );
        let len = self.col.to_string().len();
        let display_line = format!(
            "{: >width$} {}\n{: >width$} {} {}\n{: >width$} {}{: >col$}{}\n",
            "",
            "|".blue().bold(),
            self.col.to_string().bold().blue(),
            "|".blue().bold(),
            self.line,
            "",
            "|".blue().bold(),
            "",
            "^".yellow().bold(),
            width = len,
            col = self.row
        );
        let suggestion_line = format!(
            "{: >width$} {} {}",
            "",
            "=".blue().bold(),
            self.suggestion.yellow().bold(),
            width = len
        );
        write!(
            f,
            "{}{}{}{}",
            error_line, src_line, display_line, suggestion_line
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordKind {
    Ret,
    Let,
    If,
    Else,
    While,
    Int,
    Real,
    Char,
    Bool,
    True,
    False,
}

impl KeywordKind {
    pub fn map(s: &str) -> Option<KeywordKind> {
        match s {
            "ret" => Some(KeywordKind::Ret),
            "let" => Some(KeywordKind::Let),
            "if" => Some(KeywordKind::If),
            "else" => Some(KeywordKind::Else),
            "while" => Some(KeywordKind::While),
            "int" => Some(KeywordKind::Int),
            "real" => Some(KeywordKind::Real),
            "char" => Some(KeywordKind::Char),
            "bool" => Some(KeywordKind::Bool),
            "true" => Some(KeywordKind::True),
            "false" => Some(KeywordKind::False),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Semicolon,
    Colon,
    Comma,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Greater,
    Less,
    Equal,
    EqualEqual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Keyword(KeywordKind),
    Symbol(SymbolKind),
    Identifier(String),
    StringLiteral(String),
    NumericLiteral(i64),
    Whitespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub token: Tok,
    pub col: usize,
    pub row: usize,
}

pub fn lex(source: String) -> Result<Vec<Token>, LexerError> {
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
                                                    suggestion: "Valid sequences are [\\n, \\\\]"
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
            assert!(super::lex(source.into()).is_err());
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
            assert!(super::lex(source.into()).is_ok());
        }
    }
}
