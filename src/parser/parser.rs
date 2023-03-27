use super::errors::*;
use super::parse_tree::*;
use crate::lexer::{KeywordKind, SymbolKind, Tok, Token};

pub struct Parser {
    pub tokens: Vec<Token>,
}

impl Parser {
    fn peek_token(&mut self) -> Result<&Token, ParseError> {
        self.tokens.last().ok_or(ParseError {
            msg: "Unexpected end of file".to_string(),
            token: None,
        })
    }

    fn pop_token(&mut self) -> Result<Token, ParseError> {
        self.tokens.pop().ok_or(ParseError {
            msg: "Unexpected end of file".to_string(),
            token: None,
        })
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let token = self.peek_token()?;
        match &token.token {
            Tok::Identifier(_) => {
                let token = self.pop_token()?;
                if let Tok::Identifier(name) = token.token {
                    Ok(name)
                } else {
                    panic!()
                }
            }
            _ => Err(ParseError {
                msg: "Identifier expected".to_string(),
                token: Some(token.clone()),
            }),
        }
    }

    fn parse_identifier_type_pair(&mut self) -> Result<FunctionParameter, ParseError> {
        let name = self.parse_identifier()?;

        let token = self.peek_token()?;
        match token.token {
            Tok::Symbol(SymbolKind::Colon) => {
                self.pop_token()?;
            }
            _ => {
                return Err(ParseError {
                    msg: "Expected :".to_string(),
                    token: Some(token.clone()),
                })
            }
        };

        let typename = self.parse_typename()?;
        return Ok(FunctionParameter { name, typename });
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        let token = self.peek_token()?;
        match token.token {
            Tok::NumericLiteral(_) => {
                let token = self.pop_token()?;
                if let Tok::NumericLiteral(num) = token.token {
                    Ok(Expression::NumericLiteral(num))
                } else {
                    panic!()
                }
            }
            Tok::StringLiteral(_) => {
                let token = self.pop_token()?;
                if let Tok::StringLiteral(s) = token.token {
                    Ok(Expression::StringLiteral(s))
                } else {
                    panic!()
                }
            }
            _ => Err(ParseError {
                msg: "Expected expression".to_string(),
                token: self.tokens.last().cloned(),
            }),
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.peek_token()?;
        match token.token {
            Tok::Symbol(SymbolKind::Semicolon) => {
                self.pop_token()?;
                Ok(Statement::Empty)
            }
            Tok::Keyword(KeywordKind::Ret) => {
                self.pop_token()?;
                let expr = self.parse_expression()?;

                let token = self.peek_token()?;
                if let Tok::Symbol(SymbolKind::Semicolon) = token.token {
                    self.pop_token()?;
                    Ok(Statement::Return(expr))
                } else {
                    Err(ParseError {
                        msg: "Expected semicolon to end statement".to_string(),
                        token: None,
                    })
                }
            }
            Tok::Symbol(SymbolKind::LeftBrace) => {
                self.pop_token()?;

                let mut statements = vec![];
                while !self.tokens.is_empty() {
                    let token = self.peek_token()?;
                    match token.token {
                        Tok::Symbol(SymbolKind::RightBrace) => {
                            self.pop_token()?;
                            break;
                        }
                        _ => statements.push(self.parse_statement()?),
                    }
                }
                Ok(Statement::CompoundStatement(statements))
            }
            _ => Err(ParseError {
                msg: "Expected statement".to_string(),
                token: Some(token).cloned(),
            }),
        }
    }

    fn parse_array_typename(&mut self) -> Result<Typename, ParseError> {
        let token = self.peek_token()?;
        match token.token {
            Tok::Symbol(SymbolKind::LeftBracket) => {
                self.pop_token()?;
                let typename = self.parse_typename()?;

                let token = self.peek_token()?;
                if let Tok::Symbol(SymbolKind::Comma) = token.token {
                    self.pop_token()?;
                } else {
                    return Err(ParseError {
                        msg: "Expected comma".to_string(),
                        token: Some(token.clone()),
                    });
                }

                let token = self.peek_token()?;
                let size = if let Tok::NumericLiteral(num) = token.token {
                    self.pop_token()?;
                    num
                } else {
                    return Err(ParseError {
                        msg: "Expected number".to_string(),
                        token: Some(token.clone()),
                    });
                };

                let token = self.peek_token()?;
                if let Tok::Symbol(SymbolKind::RightBracket) = token.token {
                    self.pop_token()?;
                    Ok(Typename::Array(Box::new(typename), size))
                } else {
                    Err(ParseError {
                        msg: "Expected bracket ]".to_string(),
                        token: Some(token.clone()),
                    })
                }
            }
            _ => Err(ParseError {
                msg: "Expected array type".to_string(),
                token: Some(token.clone()),
            }),
        }
    }

    fn parse_typename(&mut self) -> Result<Typename, ParseError> {
        let token = self.peek_token()?;
        match token.token {
            Tok::Identifier(_) => {
                let token = self.pop_token()?;
                if let Tok::Identifier(name) = token.token {
                    Ok(Typename::Struct(name))
                } else {
                    panic!()
                }
            }
            Tok::Keyword(KeywordKind::Int) => {
                self.pop_token()?;
                Ok(Typename::Int)
            }
            Tok::Keyword(KeywordKind::Real) => {
                self.pop_token()?;
                Ok(Typename::Real)
            }
            Tok::Keyword(KeywordKind::Char) => {
                self.pop_token()?;
                Ok(Typename::Char)
            }
            Tok::Keyword(KeywordKind::Bool) => {
                self.pop_token()?;
                Ok(Typename::Bool)
            }
            Tok::Symbol(SymbolKind::LeftBracket) => self.parse_array_typename(),
            _ => Err(ParseError {
                msg: "Expected type declaration".to_string(),
                token: Some(token.clone()),
            }),
        }
    }

    fn parse_function_arguments(&mut self) -> Result<Vec<FunctionParameter>, ParseError> {
        let mut args = vec![];
        let token = self.peek_token()?;

        if let Tok::Symbol(SymbolKind::LeftParen) = token.token {
            self.pop_token()?;
        } else {
            return Err(ParseError {
                msg: "Expected open paren (".to_string(),
                token: Some(token).cloned(),
            });
        }

        let mut expecting_separator = false;
        while !self.tokens.is_empty() {
            let token = self.peek_token()?;
            match &token.token {
                Tok::Symbol(SymbolKind::Comma) => {
                    if !expecting_separator {
                        return Err(ParseError {
                            msg: "Expected identifier".to_string(),
                            token: Some(token.clone()),
                        });
                    }
                    self.pop_token()?;
                    expecting_separator = false;
                }
                Tok::Identifier(_) => {
                    if expecting_separator {
                        return Err(ParseError {
                            msg: "Expected comma".to_string(),
                            token: Some(token.clone()),
                        });
                    }
                    args.push(self.parse_identifier_type_pair()?);
                    expecting_separator = true;
                }
                Tok::Symbol(SymbolKind::RightParen) => {
                    self.pop_token()?;
                    return Ok(args);
                }
                _ => {
                    return Err(ParseError {
                        msg: "Expected function arguments".to_string(),
                        token: Some(token.clone()),
                    });
                }
            }
        }

        Err(ParseError {
            msg: "Expected closing paren )".to_string(),
            token: self.tokens.last().cloned(),
        })
    }

    fn parse_function(&mut self) -> Result<Function, ParseError> {
        Ok(Function {
            ret_type: self.parse_typename()?,
            name: self.parse_identifier()?,
            args: self.parse_function_arguments()?,
            statement: self.parse_statement()?,
        })
    }

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut module = Module { functions: vec![] };
        while !self.tokens.is_empty() {
            module.functions.push(self.parse_function()?);
        }
        return Ok(module);
    }

    pub fn parse(&mut self) -> Result<Module, ParseError> {
        self.tokens.reverse();
        self.parse_module()
    }
}

#[cfg(test)]
mod tests {
    use crate::{lexer, parser};

    #[test]
    fn function_parser_invalid() {
        let sources = vec![
            "int main);",
            "int ();",
            "int main(c: int;",
            "int main(a: int)",
        ];

        for source in sources {
            let tokens = lexer::Lexer::lex(source);
            assert!(tokens.is_ok());

            let tokens = tokens.unwrap();
            assert!(parser::Parser { tokens }.parse().is_err());
        }
    }

    #[test]
    fn function_parser_valid() {
        let sources = vec![
            "",
            "int main();",
            "bool main(){}",
            "int main(a: int, b : int, c: int);",
            "[bool, 5] main(){}",
        ];

        for source in sources {
            let tokens = lexer::Lexer::lex(source);
            assert!(tokens.is_ok());
            let tokens = tokens.unwrap();
            assert!(parser::Parser { tokens }.parse().is_ok());
        }
    }
}
