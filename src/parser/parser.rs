use core::panic;

use super::errors::*;
use super::parse_tree::*;
use crate::lexer::{KeywordKind, SymbolKind, Tok, Token};

pub struct Parser {
    tokens: Vec<Token>,
}

impl Parser {
    pub fn new(mut tokens: Vec<Token>) -> Parser {
        tokens.reverse();
        Parser { tokens }
    }

    fn peek(&mut self) -> Result<&Token, ParseError> {
        self.tokens.last().ok_or(ParseError {
            msg: "Unexpected end of file".to_string(),
            token: None,
        })
    }

    fn pop(&mut self) -> Result<Token, ParseError> {
        self.tokens.pop().ok_or(ParseError {
            msg: "Unexpected end of file".to_string(),
            token: None,
        })
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let token = self.pop()?;
        match token.token {
            Tok::Identifier(name) => Ok(name),
            _ => Err(ParseError {
                msg: "Identifier expected".to_string(),
                token: Some(token),
            }),
        }
    }

    fn parse_identifier_type_pair(&mut self) -> Result<FunctionParameter, ParseError> {
        let name = self.parse_identifier()?;

        let token = self.pop()?;
        match token.token {
            Tok::Symbol(SymbolKind::Colon) => {}
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

    fn parse_term(&mut self) -> Result<Term, ParseError> {
        let token = self.pop()?;
        match token.token {
            Tok::NumericLiteral(num) => Ok(Term::NumericLiteral(num)),
            Tok::StringLiteral(s) => Ok(Term::StringLiteral(s)),
            Tok::Identifier(name) => Ok(Term::Variable(name)),
            _ => Err(ParseError {
                msg: "Expected term".into(),
                token: Some(token),
            }),
        }
    }

    fn get_binary_operator(token: &Tok) -> Option<BinaryOperator> {
        match token {
            Tok::Symbol(SymbolKind::Plus) => Some(BinaryOperator::Plus),
            Tok::Symbol(SymbolKind::Minus) => Some(BinaryOperator::Minus),
            Tok::Symbol(SymbolKind::Star) => Some(BinaryOperator::Star),
            Tok::Symbol(SymbolKind::Slash) => Some(BinaryOperator::Star),
            _ => None,
        }
    }

    fn pratt_parser(&mut self, current_binding_power: u32) -> Result<Expression, ParseError> {
        let token = self.peek()?;
        match token.token {
            Tok::Symbol(SymbolKind::LeftParen) => {
                self.pop()?;
                let expr = self.pratt_parser(0)?;
                let token = self.peek()?;
                if let Tok::Symbol(SymbolKind::RightParen) = token.token {
                    self.pop()?;
                    Ok(expr)
                } else {
                    panic!();
                }
            }
            Tok::NumericLiteral(_) | Tok::StringLiteral(_) | Tok::Identifier(_) => {
                let mut term = Expression::ExpressionTerm(self.parse_term()?);
                loop {
                    if let Ok(token) = self.peek() {
                        if let Some(operator) = Parser::get_binary_operator(&token.token) {
                            self.pop()?;
                            let binding_power = if operator.is_left_associative() {
                                operator.get_binding_power()
                            } else {
                                operator.get_binding_power() - 1
                            };

                            if binding_power > current_binding_power {
                                let right_term = self.pratt_parser(binding_power)?;
                                term = Expression::BinaryExpression(
                                    Box::new(term),
                                    operator,
                                    Box::new(right_term),
                                );
                            } else {
                                break Ok(term);
                            }
                        } else {
                            break Ok(term);
                        }
                    } else {
                        break Ok(term);
                    }
                }
            }
            _ => Err(ParseError {
                msg: "Expected expression".to_string(),
                token: self.tokens.last().cloned(),
            }),
        }
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.pratt_parser(0)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        match token.token {
            Tok::Symbol(SymbolKind::Semicolon) => Ok(Statement::Empty),
            Tok::Keyword(KeywordKind::Ret) => {
                let expr = self.parse_expression()?;
                let token = self.pop()?;
                if let Tok::Symbol(SymbolKind::Semicolon) = token.token {
                    Ok(Statement::Return(expr))
                } else {
                    Err(ParseError {
                        msg: "Expected semicolon to end statement".to_string(),
                        token: None,
                    })
                }
            }
            Tok::Symbol(SymbolKind::LeftBrace) => {
                let mut statements = vec![];
                while !self.tokens.is_empty() {
                    let token = self.peek()?;
                    match token.token {
                        Tok::Symbol(SymbolKind::RightBrace) => {
                            self.pop()?;
                            break;
                        }
                        _ => statements.push(self.parse_statement()?),
                    }
                }
                Ok(Statement::CompoundStatement(statements))
            }
            _ => Err(ParseError {
                msg: "Expected statement".to_string(),
                token: Some(token),
            }),
        }
    }

    fn parse_array_typename(&mut self) -> Result<Typename, ParseError> {
        let token = self.pop()?;
        match token.token {
            Tok::Symbol(SymbolKind::LeftBracket) => {
                let typename = self.parse_typename()?;

                let token = self.pop()?;
                if let Tok::Symbol(SymbolKind::Comma) = token.token {
                } else {
                    return Err(ParseError {
                        msg: "Expected comma".to_string(),
                        token: Some(token.clone()),
                    });
                }

                let token = self.pop()?;
                let size = if let Tok::NumericLiteral(num) = token.token {
                    num
                } else {
                    return Err(ParseError {
                        msg: "Expected number".to_string(),
                        token: Some(token.clone()),
                    });
                };

                let token = self.pop()?;
                if let Tok::Symbol(SymbolKind::RightBracket) = token.token {
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
                token: Some(token),
            }),
        }
    }

    fn parse_typename(&mut self) -> Result<Typename, ParseError> {
        let token = self.peek()?;
        match token.token {
            Tok::Identifier(_) => {
                let token = self.pop()?;
                if let Tok::Identifier(name) = token.token {
                    Ok(Typename::Struct(name))
                } else {
                    panic!()
                }
            }
            Tok::Keyword(KeywordKind::Int) => {
                self.pop()?;
                Ok(Typename::Int)
            }
            Tok::Keyword(KeywordKind::Real) => {
                self.pop()?;
                Ok(Typename::Real)
            }
            Tok::Keyword(KeywordKind::Char) => {
                self.pop()?;
                Ok(Typename::Char)
            }
            Tok::Keyword(KeywordKind::Bool) => {
                self.pop()?;
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
        let token = self.pop()?;

        if let Tok::Symbol(SymbolKind::LeftParen) = token.token {
        } else {
            return Err(ParseError {
                msg: "Expected open paren (".to_string(),
                token: Some(token),
            });
        }

        let mut expecting_separator = false;
        while !self.tokens.is_empty() {
            let token = self.peek()?;
            match &token.token {
                Tok::Symbol(SymbolKind::Comma) => {
                    if !expecting_separator {
                        return Err(ParseError {
                            msg: "Expected identifier".to_string(),
                            token: Some(token.clone()),
                        });
                    }
                    self.pop()?;
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
                    self.pop()?;
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
        self.parse_module()
    }
}

#[cfg(test)]
mod tests {
    use crate::{lexer, parser};

    #[test]
    fn module_parser_valid() {
        let sources = vec!["", "int main();", "int a(); int b(); int c();"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_module();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn module_parser_invalid() {
        let sources = vec!["i", "int main()", "int a(); int b(); int ();"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_module();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn identifier_parser_valid() {
        let sources = vec!["ident", "abc", "_wfk23fb"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_identifier();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn identifier_parser_invalid() {
        let sources = vec!["", "23bl", "*l"];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_identifier();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn array_typename_parser_valid() {
        let sources = vec!["[[int, 5], 10]", "[int, 5]", "[t, 5]"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_array_typename();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn array_typename_parser_invalid() {
        let sources = vec!["", "1_", "*", "[", "[int, 10", "[int 10]", "[[[["];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_array_typename();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn typename_parser_valid() {
        let sources = vec!["int", "[[int, 5], 10]", "real", "custom_type"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_typename();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn typename_parser_invalid() {
        let sources = vec!["", "1_", "*", "[int]", "[int 10]"];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_typename();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn identifier_type_pair_parser_valid() {
        let sources = vec![
            "a: int",
            "a: [[int, 5], 10]",
            "ident: real",
            "ident: custom_type",
        ];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_identifier_type_pair();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn identifier_type_pair_parser_invalid() {
        let sources = vec!["", "a: ", "a int", "1: int", "int: int"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_identifier_type_pair();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn expression_parser_valid() {
        let sources = vec![
            "1",
            r#""1""#,
            r#""hello world""#,
            r#""escaped string \" hello there \"""#,
        ];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_expression();
            assert!(node.is_ok(), "{} {}", source, node.err().unwrap());
        }
    }

    #[test]
    fn expression_parser_invalid() {
        let sources = vec!["", "let", "*"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_expression();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn statement_parser_valid() {
        let sources = vec!["{}", ";", r#"ret "hello world";"#, "{ ret 1; }"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_statement();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn statement_parser_invalid() {
        let sources = vec!["", "let", "*", "ret"];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_statement();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn function_arguments_parser_vaild() {
        let sources = vec![
            "(a: int, b: [bool, 5])",
            "(a: [[int, 5], 10])",
            "(a: int, b: bool, c: char, d: [int, 5], e: real)",
        ];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_function_arguments();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn function_arguments_parser_invaild() {
        let sources = vec!["(1a: int)", "(a: _1", "a: int", "(a int)"];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_function_arguments();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn function_parser_valid() {
        let sources = vec![
            "int main();",
            "bool main(){}",
            "int main(a: int, b : int, c: int);",
            "[bool, 5] main(){}",
        ];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_function();
            assert!(node.is_ok(), "{}", node.err().unwrap());
        }
    }

    #[test]
    fn function_parser_invalid() {
        let sources = vec![
            "int main);",
            "int ();",
            "int main(c: int;",
            "int main(a: int)",
        ];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");

            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_function();
            assert!(node.is_err(), "{}", source);
        }
    }
}
