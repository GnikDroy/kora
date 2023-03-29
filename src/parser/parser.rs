use super::ast::*;
use super::errors::*;
use crate::lexer::{Keyword, Symbol, Token, TokenInfo};

pub struct Parser {
    tokens: Vec<TokenInfo>,
}

impl Parser {
    pub fn new(mut tokens: Vec<TokenInfo>) -> Parser {
        tokens.reverse();
        Parser { tokens }
    }

    fn peek(&mut self) -> Result<&TokenInfo, ParseErr> {
        self.tokens.last().ok_or(ParseErr {
            msg: "Unexpected EOF.",
            token: None,
        })
    }

    fn pop(&mut self) -> Result<TokenInfo, ParseErr> {
        self.tokens.pop().ok_or(ParseErr {
            msg: "Unexpected EOF.",
            token: None,
        })
    }

    fn pop_token(&mut self, t: Token, msg: &'static str) -> Result<(), ParseErr> {
        let token = self.pop()?;
        if token.token != t {
            Err(ParseErr {
                msg,
                token: Some(token),
            })
        } else {
            Ok(())
        }
    }

    fn parselet_integer_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::IntegerLiteral(num) = token.token {
            self.pop().unwrap();
            return Some(Ok(Expression::IntegerLiteral(num)))
        }
        None
    }

    fn parselet_string_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::StringLiteral(_) = token.token {
               let token = self.pop();
               if let Ok(token) = token
                  && let Token::StringLiteral(s) = token.token {
                    return Some(Ok(Expression::StringLiteral(s)))
               }
        }
        None
    }

    fn parselet_boolean_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token {
            if matches!(
                token.token,
                Token::Keyword(Keyword::True) | Token::Keyword(Keyword::False)
            ) {
                let token = self.pop().unwrap();
                return match token.token {
                    Token::Keyword(Keyword::True) => Some(Ok(Expression::BoolLiteral(true))),
                    Token::Keyword(Keyword::False) => Some(Ok(Expression::BoolLiteral(false))),
                    _ => None,
                };
            }
        }
        None
    }

    fn parselet_real_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::RealLiteral(r) = token.token {
               self.pop().unwrap();
               return Some(Ok(Expression::RealLiteral(r)))
        }
        None
    }

    fn parselet_identifier(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Identifier(_) = token.token {
               let token = self.pop();
               if let Ok(token) = token
                  && let Token::Identifier(r) = token.token {
                    return Some(Ok(Expression::Identifier(r)))
               }
        }
        None
    }

    fn parselet_array_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::LeftBracket) = token.token {
                let expr_list = self.parse_generic_delimited(
                    Token::Symbol(Symbol::LeftBracket),
                    Token::Symbol(Symbol::RightBracket),
                    Token::Symbol(Symbol::Comma),
                    |s| Parser::pratt_parser(s, 0),
                );
                let expr_list = expr_list.map(|e| Expression::Array(e));
                return Some(expr_list);
        }
        None
    }

    fn parselet_negate_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::Minus) = token.token {
                self.pop().unwrap();
                let expr = self.pratt_parser(UnaryOp::Negate.get_binding_power());
                let expr = expr.map(|e| Expression::Unary(UnaryOp::Negate, Box::new(e)));
                return Some(expr);
        }
        None
    }

    fn parselet_not_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::Exclam) = token.token {
                self.pop().unwrap();
                let expr = self.pratt_parser(UnaryOp::Not.get_binding_power());
                let expr = expr.map(|e| Expression::Unary(UnaryOp::Not, Box::new(e)));
                return Some(expr);
        }
        None
    }

    fn parselet_parenthesized_expression(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::LeftParen) = token.token {
                self.pop().unwrap();
                let expr = self.pratt_parser(0);
                let expr = expr.and_then(|e|
                        self.pop_token(
                            Token::Symbol(Symbol::RightParen),
                            "Expected closing paren ) in parenthesized expression: (<expr>)"
                        )
                        .map(|_| e)
                );
                return Some(expr);
        }
        None
    }

    fn parse_initial_expression(&mut self) -> Result<Expression, ParseErr> {
        let parselets = [
            Parser::parselet_integer_literal,
            Parser::parselet_string_literal,
            Parser::parselet_boolean_literal,
            Parser::parselet_real_literal,
            Parser::parselet_identifier,
            Parser::parselet_parenthesized_expression,
            Parser::parselet_array_literal,
            Parser::parselet_negate_operator,
            Parser::parselet_not_operator,
        ];

        for parselet in parselets.iter() {
            if let Some(v) = parselet(self) {
                return v;
            }
        }

        Err(ParseErr {
            msg: "Expected expression: <expr>",
            token: None,
        })
    }

    fn parselet_infix_function_call(
        &mut self,
        _: InfixOperator,
        term: Expression,
    ) -> Result<Expression, ParseErr> {
        let args = self.parse_expression_list();
        let expr = args.map(|args| Expression::Call(Box::new(term), args));
        expr
    }

    fn parselet_infix_binary_operators(
        &mut self,
        op: InfixOperator,
        binary_op: BinaryOp,
        left: Expression,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        let right = self.pratt_parser(op.get_binding_power());
        let expr =
            right.map(|right| Expression::Binary(Box::new(left), binary_op, Box::new(right)));
        expr
    }

    fn parselet_infix_operators(
        &mut self,
        op: InfixOperator,
        term: Expression,
    ) -> Result<Expression, ParseErr> {
        match op {
            InfixOperator::Binary(binary_op) => {
                self.parselet_infix_binary_operators(op, binary_op.clone(), term)
            }
            InfixOperator::FunctionCall => self.parselet_infix_function_call(op, term),
        }
    }

    fn pratt_parser(&mut self, current_binding_power: u32) -> Result<Expression, ParseErr> {
        let mut term = self.parse_initial_expression()?;
        loop {
            if matches!(self.peek(), Err(_)) {
                break Ok(term);
            }

            let token = self.peek().unwrap();
            if let Some(operator) = InfixOperator::get(&token.token) {
                let binding_power = operator.get_binding_power();
                if binding_power > current_binding_power {
                    term = self.parselet_infix_operators(operator, term)?;
                } else {
                    break Ok(term);
                }
            } else {
                break Ok(term);
            }
        }
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseErr> {
        self.pratt_parser(0)
    }

    fn parse_identifier(&mut self) -> Result<String, ParseErr> {
        let token = self.pop()?;
        match token.token {
            Token::Identifier(name) => Ok(name),
            _ => Err(ParseErr {
                msg: "Identifier expected: <identifier>",
                token: Some(token),
            }),
        }
    }

    fn parse_identifier_type_pair(&mut self) -> Result<IdentifierTypePair, ParseErr> {
        let name = self.parse_identifier()?;

        self.pop_token(
            Token::Symbol(Symbol::Colon),
            "Expected colon after identifier: <identifier> : <type>",
        )?;

        let typename = self.parse_typename()?;
        Ok(IdentifierTypePair { name, typename })
    }

    fn parse_generic_delimited<T>(
        &mut self,
        begin: Token,
        end: Token,
        delimiter: Token,
        f: fn(&mut Parser) -> Result<T, ParseErr>,
    ) -> Result<Vec<T>, ParseErr> {
        self.pop_token(
            begin,
            "Cannot parse multiple items. Beginning token not found.",
        )?;

        let mut args = vec![];
        let mut expecting_separator = false;
        while !self.tokens.is_empty() {
            let token = self.peek()?;
            if token.token == end {
                self.pop()?;
                return Ok(args);
            } else if token.token == delimiter && !expecting_separator {
                return Err(ParseErr {
                    msg: "Expected item, found separator.",
                    token: Some(token.clone()),
                });
            } else if token.token != delimiter && expecting_separator {
                return Err(ParseErr {
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

        Err(ParseErr {
            msg: "Cannot parse multiple items. Ending token not found.",
            token: self.tokens.last().cloned(),
        })
    }

    fn parse_compound_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Symbol(Symbol::LeftBrace),
            "Expected compound statement: { <stmt> <stmt> ... }",
        )?;

        let mut statements = vec![];
        while !self.tokens.is_empty() {
            let token = self.peek()?;
            match token.token {
                Token::Symbol(Symbol::RightBrace) => {
                    self.pop()?;
                    break;
                }
                _ => statements.push(self.parse_statement()?),
            }
        }

        Ok(Statement::Compound(statements))
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::Ret),
            "Expected return statement: ret <expr>;",
        )?;

        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon: ret <expr>;",
        )?;
        Ok(Statement::Return(expr))
    }

    fn parse_simple_statement(&mut self) -> Result<Statement, ParseErr> {
        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected expr-statement to end in semicolon: <expression>;",
        )?;

        Ok(Statement::Simple(expr))
    }

    fn parse_let_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::Let),
            "Expected let: let <identifier>:<type> = <expression>;",
        )?;

        let declaration = self.parse_identifier_type_pair()?;

        self.pop_token(
            Token::Symbol(Symbol::Equal),
            "Expected =: let <identifier>:<type> = <expression>;",
        )?;

        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon: let <identifier>:<type> = <expression>;",
        )?;

        Ok(Statement::Let(declaration, expr))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::If),
            "Expected if: if (<expression>) <statement> else <statement>",
        )?;

        self.pop_token(
            Token::Symbol(Symbol::LeftParen),
            "Expected ( before expression: if (<expr>) <statement>",
        )?;

        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::RightParen),
            "Expected ) after expression: if (<expr>) <statement>",
        )?;

        let stmt = self.parse_statement()?;

        if let Ok(token) = self.peek() {
            if let Token::Keyword(Keyword::Else) = token.token {
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

    fn parse_while_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::While),
            "Expected while: while (<expression>) <statement>;",
        )?;

        self.pop_token(
            Token::Symbol(Symbol::LeftParen),
            "Expected (: while (<expr>) <statement>",
        )?;

        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::RightParen),
            "Expected ): while (<expr>) <statement>",
        )?;

        let stmt = self.parse_statement()?;
        Ok(Statement::While(expr, Box::new(stmt)))
    }

    fn parse_empty_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Empty statements must end in semicolon",
        )?;
        Ok(Statement::Empty)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseErr> {
        let token = self.peek()?;
        match token.token {
            Token::Symbol(Symbol::Semicolon) => self.parse_empty_statement(),
            Token::Symbol(Symbol::LeftBrace) => self.parse_compound_statement(),
            Token::Keyword(Keyword::Ret) => self.parse_return_statement(),
            Token::Keyword(Keyword::Let) => self.parse_let_statement(),
            Token::Keyword(Keyword::While) => self.parse_while_statement(),
            Token::Keyword(Keyword::If) => self.parse_if_statement(),
            _ => self.parse_simple_statement(),
        }
    }

    fn parse_array_typename(&mut self) -> Result<Type, ParseErr> {
        self.pop_token(
            Token::Symbol(Symbol::LeftBracket),
            "Expected array type: [<type>, <size>]",
        )?;

        let typename = self.parse_typename()?;

        self.pop_token(
            Token::Symbol(Symbol::Comma),
            "Expected comma after type: [<type>, <size>]",
        )?;

        let token = self.pop()?;
        let size = if let Token::IntegerLiteral(num) = token.token {
            num
        } else {
            return Err(ParseErr {
                msg: "Expected number to specify size: [<type>, <size>]",
                token: Some(token.clone()),
            });
        };

        self.pop_token(
            Token::Symbol(Symbol::RightBracket),
            "Expected ] to end type specification: [<type>, <size>]",
        )?;

        Ok(Type::Array(Box::new(typename), size))
    }

    fn parse_typename(&mut self) -> Result<Type, ParseErr> {
        let token = self.pop()?;
        match token.token {
            Token::Keyword(Keyword::Nil) => Ok(Type::Nil),
            Token::Keyword(Keyword::Int) => Ok(Type::Int),
            Token::Keyword(Keyword::Real) => Ok(Type::Real),
            Token::Keyword(Keyword::Char) => Ok(Type::Char),
            Token::Keyword(Keyword::Bool) => Ok(Type::Bool),
            Token::Identifier(name) => Ok(Type::Struct(name)),
            Token::Symbol(Symbol::LeftBracket) => {
                self.tokens.push(token);
                self.parse_array_typename()
            }
            _ => Err(ParseErr {
                msg: "Expected type declaration: <type> | [<type>, <size>]",
                token: Some(token.clone()),
            }),
        }
    }

    fn parse_expression_list(&mut self) -> Result<Vec<Expression>, ParseErr> {
        self.parse_generic_delimited(
            Token::Symbol(Symbol::LeftParen),
            Token::Symbol(Symbol::RightParen),
            Token::Symbol(Symbol::Comma),
            Parser::parse_expression,
        )
    }

    fn parse_function_parameters(&mut self) -> Result<Vec<IdentifierTypePair>, ParseErr> {
        self.parse_generic_delimited(
            Token::Symbol(Symbol::LeftParen),
            Token::Symbol(Symbol::RightParen),
            Token::Symbol(Symbol::Comma),
            Parser::parse_identifier_type_pair,
        )
    }

    fn parse_function(&mut self) -> Result<Function, ParseErr> {
        Ok(Function {
            return_type: self.parse_typename()?,
            name: self.parse_identifier()?,
            arguments: self.parse_function_parameters()?,
            statement: self.parse_statement()?,
        })
    }

    fn parse_module(&mut self) -> Result<Module, ParseErr> {
        let mut module = Module { functions: vec![] };
        while !self.tokens.is_empty() {
            module.functions.push(self.parse_function()?);
        }
        Ok(module)
    }

    pub fn parse(&mut self) -> Result<Module, ParseErr> {
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
            assert!(
                node.is_ok() && parser.tokens.is_empty(),
                "{} {:?}",
                source,
                node
            );
            println!("{:#?}", node.unwrap());
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
