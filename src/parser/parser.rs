use super::ast::*;
use super::errors::*;
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
            msg: "Unexpected EOF.",
            token: None,
        })
    }

    fn pop(&mut self) -> Result<Token, ParseError> {
        self.tokens.pop().ok_or(ParseError {
            msg: "Unexpected EOF.",
            token: None,
        })
    }

    fn parse_generic_delimited<T>(
        &mut self,
        begin: Tok,
        end: Tok,
        delimiter: Tok,
        f: fn(&mut Parser) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        let token = self.pop()?;
        if token.token != begin {
            return Err(ParseError {
                msg: "Parsing failed while parsing multiple items. Beginning token not found.",
                token: Some(token),
            });
        }

        let mut args = vec![];
        let mut expecting_separator = false;
        while !self.tokens.is_empty() {
            let token = self.peek()?;
            if token.token == end {
                self.pop()?;
                return Ok(args);
            } else if token.token == delimiter && !expecting_separator {
                return Err(ParseError {
                    msg: "Expected item, found separator.",
                    token: Some(token.clone()),
                });
            } else if token.token != delimiter && expecting_separator {
                return Err(ParseError {
                    msg: "Expected separator, found something else.",
                    token: Some(token.clone()),
                });
            } else if token.token == delimiter {
                self.pop()?;
            } else {
                args.push(f(self)?);
            }
            expecting_separator = !expecting_separator;
        }

        Err(ParseError {
            msg: "Parsing failed while parsing multiple items. Ending token not found.",
            token: self.tokens.last().cloned(),
        })
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let token = self.pop()?;
        match token.token {
            Tok::Identifier(name) => Ok(name),
            _ => Err(ParseError {
                msg: "Identifier expected: <identifier>",
                token: Some(token),
            }),
        }
    }

    fn parse_identifier_type_pair(&mut self) -> Result<IdentifierTypePair, ParseError> {
        let name = self.parse_identifier()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Colon) {
            return Err(ParseError {
                msg: "Expected colon after identifier: <identifier> : <type>",
                token: Some(token.clone()),
            });
        }

        let typename = self.parse_typename()?;
        Ok(IdentifierTypePair { name, typename })
    }

    fn parse_initial_expression(&mut self) -> Result<Expression, ParseError> {
        let token = self.pop()?;
        match token.token {
            Tok::IntegerLiteral(num) => Ok(Expression::IntegerLiteral(num)),
            Tok::StringLiteral(s) => Ok(Expression::StringLiteral(s)),
            Tok::Keyword(KeywordKind::True) => Ok(Expression::BooleanLiteral(true)),
            Tok::Keyword(KeywordKind::False) => Ok(Expression::BooleanLiteral(false)),
            Tok::RealLiteral(r) => Ok(Expression::RealLiteral(r)),
            Tok::Identifier(name) => Ok(Expression::Variable(name)),
            Tok::Symbol(SymbolKind::Minus) => Ok(Expression::UnaryExpression(
                UnaryOperator::Negate,
                Box::new(self.pratt_parser(UnaryOperator::Negate.get_binding_power())?),
            )),
            Tok::Symbol(SymbolKind::Exclam) => Ok(Expression::UnaryExpression(
                UnaryOperator::Not,
                Box::new(self.pratt_parser(UnaryOperator::Not.get_binding_power())?),
            )),
            Tok::Symbol(SymbolKind::LeftParen) => {
                let expr = self.pratt_parser(0)?;
                let token = self.pop()?;
                if token.token == Tok::Symbol(SymbolKind::RightParen) {
                    Ok(expr)
                } else {
                    Err(ParseError {
                        msg: "Expected closing paren ) in parenthesized expression: (<expr>)"
                            .into(),
                        token: Some(token),
                    })
                }
            }
            Tok::Symbol(SymbolKind::LeftBracket) => {
                self.tokens.push(token);
                Ok(Expression::Array(self.parse_generic_delimited(
                    Tok::Symbol(SymbolKind::LeftBracket),
                    Tok::Symbol(SymbolKind::RightBracket),
                    Tok::Symbol(SymbolKind::Comma),
                    |s| Parser::pratt_parser(s, 0),
                )?))
            }
            _ => Err(ParseError {
                msg: "Expected expression: <expr>",
                token: Some(token),
            }),
        }
    }

    // An implementation of pratt expression parsing
    fn pratt_parser(&mut self, current_binding_power: u32) -> Result<Expression, ParseError> {
        let mut term = self.parse_initial_expression()?;
        loop {
            if let Ok(token) = self.peek().cloned() {
                match token.token {
                    Tok::Symbol(SymbolKind::LeftParen) => {
                        let args = self.parse_expression_list()?;
                        break Ok(Expression::CallExpression(Box::new(term), args));
                    }
                    _ => {}
                }

                if let Some(operator) = BinaryOperator::get(&token.token) {
                    let binding_power = if operator.is_left_associative() {
                        operator.get_binding_power()
                    } else {
                        operator.get_binding_power() - 1
                    };

                    if binding_power <= current_binding_power {
                        break Ok(term);
                    } else {
                        self.pop()?;
                        let right_term = self.pratt_parser(binding_power)?;
                        term = Expression::BinaryExpression(
                            Box::new(term),
                            operator,
                            Box::new(right_term),
                        );
                    }
                } else {
                    break Ok(term);
                }
            } else {
                break Ok(term);
            }
        }
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.pratt_parser(0)
    }

    fn parse_compound_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::LeftBrace) {
            return Err(ParseError {
                msg: "Expected compound statement: { <stmt> <stmt> ... }",
                token: Some(token.clone()),
            });
        }

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

    fn parse_return_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Keyword(KeywordKind::Ret) {
            return Err(ParseError {
                msg: "Expected return statement: ret <expr>;",
                token: Some(token.clone()),
            });
        }

        let expr = self.parse_expression()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Semicolon) {
            return Err(ParseError {
                msg: "Expected semicolon: ret <expr>;",
                token: Some(token.clone()),
            });
        }
        Ok(Statement::Return(expr))
    }

    fn parse_simple_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_expression()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Semicolon) {
            return Err(ParseError {
                msg: "Expected expr-statement to end in semicolon: <expression>;",
                token: Some(token),
            });
        }

        Ok(Statement::Simple(expr))
    }

    fn parse_let_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Keyword(KeywordKind::Let) {
            return Err(ParseError {
                msg: "Expected let statement: let <identifier>:<typename> = <expression>;",
                token: Some(token.clone()),
            });
        }

        let declaration = self.parse_identifier_type_pair()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Equal) {
            return Err(ParseError {
                msg: "Expected = in statement: let <identifier>:<typename> = <expression>;",
                token: Some(token),
            });
        }

        let expr = self.parse_expression()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Semicolon) {
            return Err(ParseError {
                msg: "Expected semicolon: let <identifier>:<typename> = <expression>;",
                token: Some(token),
            });
        }

        Ok(Statement::Let(declaration, expr))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Keyword(KeywordKind::If) {
            return Err(ParseError {
                msg: "Expected if statement: if (<expression>) <statement> else <statement>",
                token: Some(token.clone()),
            });
        }

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::LeftParen) {
            return Err(ParseError {
                msg: "Expected starting brace ( before expression: if (<expr>) <statement>",
                token: Some(token),
            });
        }

        let expr = self.parse_expression()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::RightParen) {
            return Err(ParseError {
                msg: "Expected closing brace ) after expression: if (<expr>) <statement>".into(),
                token: Some(token),
            });
        }

        let stmt = self.parse_statement()?;

        if let Ok(token) = self.peek() {
            if let Tok::Keyword(KeywordKind::Else) = token.token {
                self.pop()?;
                let else_stmt = self.parse_statement()?;
                return Ok(Statement::If(
                    expr,
                    Box::new(stmt),
                    Some(Box::new(else_stmt)),
                ));
            }
        }

        Ok(Statement::If(expr, Box::new(stmt), None))
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Keyword(KeywordKind::While) {
            return Err(ParseError {
                msg: "Expected while statement: while (<expression>) <statement>;",
                token: Some(token.clone()),
            });
        }

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::LeftParen) {
            return Err(ParseError {
                msg: "Expected starting brace ( before expression: while (<expr>) <statement>",
                token: Some(token),
            });
        }

        let expr = self.parse_expression()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::RightParen) {
            return Err(ParseError {
                msg: "Expected closing brace ) after expression: while (<expr>) <statement>",
                token: Some(token),
            });
        }

        let stmt = self.parse_statement()?;
        Ok(Statement::While(expr, Box::new(stmt)))
    }

    fn parse_empty_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Semicolon) {
            return Err(ParseError {
                msg: "Empty statements must end in semicolon",
                token: Some(token),
            });
        }
        Ok(Statement::Empty)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.peek()?;
        match token.token {
            Tok::Symbol(SymbolKind::Semicolon) => self.parse_empty_statement(),
            Tok::Symbol(SymbolKind::LeftBrace) => self.parse_compound_statement(),
            Tok::Keyword(KeywordKind::Ret) => self.parse_return_statement(),
            Tok::Keyword(KeywordKind::Let) => self.parse_let_statement(),
            Tok::Keyword(KeywordKind::While) => self.parse_while_statement(),
            Tok::Keyword(KeywordKind::If) => self.parse_if_statement(),
            _ => self.parse_simple_statement(),
        }
    }

    fn parse_array_typename(&mut self) -> Result<Typename, ParseError> {
        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::LeftBracket) {
            return Err(ParseError {
                msg: "Expected array type: [<type>, <size>]",
                token: Some(token),
            });
        }

        let typename = self.parse_typename()?;

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::Comma) {
            return Err(ParseError {
                msg: "Expected comma after type to specify size: [<type>, <size>]",
                token: Some(token.clone()),
            });
        }

        let token = self.pop()?;
        let size = if let Tok::IntegerLiteral(num) = token.token {
            num
        } else {
            return Err(ParseError {
                msg: "Expected number to specify size: [<type>, <size>]",
                token: Some(token.clone()),
            });
        };

        let token = self.pop()?;
        if token.token != Tok::Symbol(SymbolKind::RightBracket) {
            return Err(ParseError {
                msg: "Expected closing bracket ] to end type specification: [<type>, <size>]",
                token: Some(token.clone()),
            });
        }

        Ok(Typename::Array(Box::new(typename), size))
    }

    fn parse_typename(&mut self) -> Result<Typename, ParseError> {
        let token = self.peek()?;
        match token.token {
            Tok::Symbol(SymbolKind::LeftBracket) => self.parse_array_typename(),
            Tok::Identifier(_) => {
                let identifier = self.parse_identifier()?;
                Ok(Typename::Struct(identifier))
            }
            Tok::Keyword(KeywordKind::Nil) => {
                self.pop()?;
                Ok(Typename::Nil)
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
            _ => Err(ParseError {
                msg: "Expected type declaration: <type> | [<type>, <size>]",
                token: Some(token.clone()),
            }),
        }
    }

    fn parse_expression_list(&mut self) -> Result<Vec<Expression>, ParseError> {
        self.parse_generic_delimited(
            Tok::Symbol(SymbolKind::LeftParen),
            Tok::Symbol(SymbolKind::RightParen),
            Tok::Symbol(SymbolKind::Comma),
            Parser::parse_expression,
        )
    }

    fn parse_function_parameters(&mut self) -> Result<Vec<IdentifierTypePair>, ParseError> {
        self.parse_generic_delimited(
            Tok::Symbol(SymbolKind::LeftParen),
            Tok::Symbol(SymbolKind::RightParen),
            Tok::Symbol(SymbolKind::Comma),
            Parser::parse_identifier_type_pair,
        )
    }

    fn parse_function(&mut self) -> Result<Function, ParseError> {
        Ok(Function {
            ret_type: self.parse_typename()?,
            name: self.parse_identifier()?,
            args: self.parse_function_parameters()?,
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
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
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
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
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
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
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
        let sources = vec![
            "nil",
            "int",
            "real",
            "char",
            "[[int, 5], 10]",
            "custom_type",
        ];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_typename();
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
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
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
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
            "1-2-3",
            "1.234",
            "[1,2,3]",
            "true == false & false | true",
            "a=b - a != b + a | b + c & d",
            "-a + -b / !c",
            "a==b + c<d + a<=b + 1>2 + e>=f",
            "(1/2 + (x+4) / 4) / ((x-5)/2 + (x+4)/(x-5))",
            r#"a + b/2 - c/(x * 4) * (3 + 4/(5+"hello there"))"#,
            r#"a + func_call(a, "b" + 2, (a+b) * [1, "abc", (a+b)/2] / 2) / 2"#,
        ];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_expression();
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
        }
    }

    #[test]
    fn expression_parser_invalid() {
        let sources = vec!["", "let", "*", "a=", "(a", "a<="];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_expression();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn statement_parser_valid() {
        let sources = vec![
            "{}",
            ";",
            "1;",
            "ret 1;",
            "let a : int = b;",
            "if (1);",
            "if (1); else ;",
            "while (1) ;",
            "{ ret 1; let a : int = b; }",
            "if (a + b) { while (1) { a = b; } ret 1; }",
            "if (true) { a = b; let a: bool = true; } else { c = d; print(a); }",
        ];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_statement();
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
        }
    }

    #[test]
    fn statement_parser_invalid() {
        let sources = vec![
            "",
            "let",
            "*",
            "ret",
            "let a == 2;",
            "while a = b {",
            "if (a = b) }",
        ];
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_statement();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn expression_list_parser_vaild() {
        let sources = vec!["(a, b, (c+d)/2 + b/4)", "((a + b/2 + c*(a+b)/d))"];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_expression_list();
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
        }
    }

    #[test]
    fn expression_list_parser_invaild() {
        let sources = vec!["", "(", "a: int", "(a, b,()"];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_expression_list();
            assert!(node.is_err(), "{}", source);
        }
    }

    #[test]
    fn function_parameter_parser_vaild() {
        let sources = vec![
            "(a: int, b: [bool, 5])",
            "(a: [[int, 5], 10])",
            "(a: int, b: bool, c: char, d: [int, 5], e: real)",
        ];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_function_parameters();
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
        }
    }

    #[test]
    fn function_parameter_parser_invalid() {
        let sources = vec!["(1a: int)", "(a: _1", "a: int", "(a int)"];

        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = parser.parse_function_parameters();
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
            assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
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

    #[test]
    fn complex() {
        let source = r#"
            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                if (a - b) {
                    print("Hello World", 5);
                }
                print("Oh no!", 5);
                ret a;
            }
            
            nil print(b: [char, 1], a: int) {
                while (a) {
                    print(b);
                    a = a - 1;
                }

            }
            
            int sum(a: int, b: int) {
                ret a + b;
            }
        "#;
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let node = parser.parse();
        assert!(node.is_ok() && parser.tokens.is_empty(), "{:?}", node);
    }
}
