mod ast;
mod errors;
mod visitor;

pub use ast::*;
pub use errors::*;
pub use visitor::*;

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

    fn parselet_char_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::CharLiteral(_) = token.token {
               let token = self.pop();
               if let Ok(token) = token
                  && let Token::CharLiteral(c) = token.token {
                    return Some(Ok(Expression::CharLiteral(c)))
               }
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
                let expr_list = expr_list.map(Expression::Array);
                return Some(expr_list);
        }
        None
    }

    fn parselet_negate_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::Minus) = token.token {
                self.pop().unwrap();
                let expr = self.pratt_parser(UnaryOp::Negate.get_binding_power())
                    .map(|e| Expression::Unary(UnaryOp::Negate, Box::new(e)));
                return Some(expr);
        }
        None
    }

    fn parselet_not_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::Exclam) = token.token {
                self.pop().unwrap();
                let expr = self.pratt_parser(UnaryOp::Not.get_binding_power())
                    .map(|e| Expression::Unary(UnaryOp::Not, Box::new(e)));
                return Some(expr);
        }
        None
    }

    fn parselet_new_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Keyword(Keyword::New) = token.token {
               let expr = || -> Result<Expression, ParseErr> {
                    self.pop().unwrap();
                    let typename = self.parse_typename()?;
                    let token = self.peek();
                    if let Ok(token) = token
                    && let Token::Symbol(Symbol::LeftBracket) = token.token {
                        self.pop().unwrap();
                        let expr = self.pratt_parser(0)
                            .map(|e| Expression::Construct(typename, Some(Box::new(e))))?;
                        self.pop_token(
                            Token::Symbol(Symbol::RightBracket),
                             "Expected ] after array constructor: new <type>[<expr>]"
                        )?;
                        Ok(expr)
                    }
                    else {
                        Ok(Expression::Construct(typename, None))
                    }
                }();
                return Some(expr);
        }
        None
    }

    fn parselet_parenthesized_expression(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
           && let Token::Symbol(Symbol::LeftParen) = token.token {
                self.pop().unwrap();
                let expr = self.pratt_parser(0)
                    .and_then(|e|
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
            Parser::parselet_char_literal,
            Parser::parselet_string_literal,
            Parser::parselet_boolean_literal,
            Parser::parselet_real_literal,
            Parser::parselet_identifier,
            Parser::parselet_parenthesized_expression,
            Parser::parselet_array_literal,
            Parser::parselet_negate_operator,
            Parser::parselet_not_operator,
            Parser::parselet_new_operator,
        ];

        parselets.iter().find_map(|f| f(self)).map_or_else(
            || {
                Err(ParseErr {
                    msg: "Expected expression: <expr>",
                    token: None,
                })
            },
            |r| r,
        )
    }

    fn parselet_infix_function_call(
        &mut self,
        _: InfixOperator,
        term: Expression,
    ) -> Result<Expression, ParseErr> {
        let args = self.parse_expression_list();
        args.map(|args| Expression::Call(Box::new(term), args))
    }

    fn parselet_infix_binary_operators(
        &mut self,
        op: InfixOperator,
        binary_op: BinaryOp,
        left: Expression,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        let right = self.pratt_parser(op.get_binding_power());
        right.map(|right| Expression::Binary(Box::new(left), binary_op, Box::new(right)))
    }

    fn parselet_infix_cast_operator(
        &mut self,
        _: InfixOperator,
        left: Expression,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        self.parse_typename()
            .map(|t| Expression::Cast(Box::new(left), t))
    }

    fn parselet_infix_array_index(
        &mut self,
        _: InfixOperator,
        term: Expression,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        let right = self.pratt_parser(0);
        self.pop_token(
            Token::Symbol(Symbol::RightBracket),
            "Expected closing bracket ] after expression: [<expr>]",
        )?;
        right.map(|right| Expression::ArrayIndex(Box::new(term), Box::new(right)))
    }

    fn parselet_infix_access(
        &mut self,
        op: InfixOperator,
        term: Expression,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        let right = self.pratt_parser(op.get_binding_power());
        right.map(|right| Expression::Access(Box::new(term), Box::new(right)))
    }

    fn parselet_infix_operators(
        &mut self,
        op: InfixOperator,
        term: Expression,
    ) -> Result<Expression, ParseErr> {
        match op {
            InfixOperator::Binary(BinaryOp::Cast) => self.parselet_infix_cast_operator(op, term),
            InfixOperator::Binary(o) => self.parselet_infix_binary_operators(op, o, term),
            InfixOperator::FunctionCall => self.parselet_infix_function_call(op, term),
            InfixOperator::ArrayIndex => self.parselet_infix_array_index(op, term),
            InfixOperator::Access => self.parselet_infix_access(op, term),
        }
    }

    fn pratt_parser(&mut self, current_binding_power: u32) -> Result<Expression, ParseErr> {
        let mut term = self.parse_initial_expression()?;
        loop {
            if !matches!(self.peek(), Err(_)) {
                let token = self.peek().unwrap();
                if let Ok(operator) = InfixOperator::try_from(token.token.clone()) {
                    let binding_power = operator.get_binding_power();
                    if binding_power > current_binding_power {
                        term = self.parselet_infix_operators(operator, term)?;
                        continue;
                    }
                }
            }
            break Ok(term);
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
                    return Ok(Statement::Compound(statements));
                }
                _ => statements.push(self.parse_statement()?),
            }
        }

        Err(ParseErr {
            msg: "Expected } in compound statement: { <stmt> <stmt> ... }",
            token: None,
        })
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
            "Expected array type: [<type>]",
        )?;

        let typename = self.parse_typename()?;

        self.pop_token(
            Token::Symbol(Symbol::RightBracket),
            "Expected ] to end type specification: [<type>]",
        )?;

        Ok(Type::Array(Box::new(typename)))
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

    fn parse_extern_function(&mut self) -> Result<ExternFunction, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::Extern),
            "Expected function declaration",
        )?;

        let return_type = self.parse_typename()?;
        let name = self.parse_identifier()?;
        let arguments = self.parse_function_parameters()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon ; to end extern function declaration",
        )?;

        Ok(ExternFunction {
            return_type,
            name,
            arguments,
        })
    }

    fn parse_function(&mut self) -> Result<Function, ParseErr> {
        Ok(Function {
            return_type: self.parse_typename()?,
            name: self.parse_identifier()?,
            arguments: self.parse_function_parameters()?,
            statement: self.parse_statement()?,
        })
    }

    fn parse_struct(&mut self) -> Result<Struct, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::Struct),
            "Expected struct declaration to start with 'struct': struct <name> {...}",
        )?;
        let name = self.parse_identifier()?;
        let members = self.parse_generic_delimited(
            Token::Symbol(Symbol::LeftBrace),
            Token::Symbol(Symbol::RightBrace),
            Token::Symbol(Symbol::Comma),
            Parser::parse_identifier_type_pair,
        )?;
        Ok(Struct { name, members })
    }

    fn parse_module(&mut self) -> Result<Module, ParseErr> {
        let mut module = Module {
            ..Default::default()
        };
        while !self.tokens.is_empty() {
            let token = self.peek()?;
            match token.token {
                Token::Keyword(Keyword::Struct) => {
                    module.structs.push(self.parse_struct()?);
                }
                Token::Keyword(Keyword::Extern) => {
                    module.extern_functions.push(self.parse_extern_function()?);
                }
                _ => {
                    module.functions.push(self.parse_function()?);
                }
            }
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

    use super::Parser;

    fn test_parser_valid<T: std::fmt::Debug>(
        sources: &[&str],
        f: fn(&mut parser::Parser) -> Result<T, parser::ParseErr>,
    ) {
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = f(&mut parser);
            assert!(
                node.is_ok() && parser.tokens.is_empty(),
                "source_text: {}, remaining_tokens: {:?}, parsed_element: {:#?}",
                source,
                parser.tokens,
                node
            );
        }
    }

    fn test_parser_invalid<T: std::fmt::Debug>(
        sources: &[&str],
        f: fn(&mut parser::Parser) -> Result<T, parser::ParseErr>,
    ) {
        for source in sources {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let mut parser = parser::Parser::new(tokens);
            let node = f(&mut parser);
            assert!(
                node.is_err(),
                "source_text: {}, parsed_element: {:#?}",
                source,
                node
            );
        }
    }

    fn test_parser<T: std::fmt::Debug>(
        valid_sources: &[&str],
        invalid_sources: &[&str],
        f: fn(&mut parser::Parser) -> Result<T, parser::ParseErr>,
    ) {
        test_parser_valid(valid_sources, f);
        test_parser_invalid(invalid_sources, f);
    }

    #[test]
    fn parse_module() {
        test_parser(
            &["", "int main();", "extern int a(); int b(); int c();"],
            &["i", "int main()", "int a(); int b(); int ();"],
            Parser::parse_module,
        );
    }

    #[test]
    fn parse_struct() {
        test_parser(
            &["struct Person { age: int, name: [char]}", "struct Foo {}"],
            &["struct Foo", "struct {}", "struct Foo { foo, bar }"],
            Parser::parse_struct,
        );
    }

    #[test]
    fn parse_identifier() {
        test_parser(
            &["foo", "_before_2000", "TestCase"],
            &["", "2000", "{ 0 }"],
            Parser::parse_identifier,
        );
    }

    #[test]
    fn parse_array_typename() {
        test_parser(
            &["[[int]]", "[real]", "[foo]"],
            &["", "[", "[int", "int]", "[[[["],
            Parser::parse_array_typename,
        );
    }

    #[test]
    fn parse_typename() {
        test_parser(
            &["nil", "int", "real", "char", "[[int]]", "custom_type"],
            &["", "2000", "{0}", "[int", "]"],
            Parser::parse_typename,
        );
    }

    #[test]
    fn parse_identifier_type_pair() {
        test_parser(
            &["a: int", "a: [[int]]", "ident: real", "ident: custom_type"],
            &["", "a: ", "a int", "1: int", "int: int"],
            Parser::parse_identifier_type_pair,
        );
    }

    #[test]
    fn parse_expression() {
        test_parser(
            &[
                "1-2-3%3",
                "(1.234 as real) as int",
                r#"'a'+"abc"+'a'"#,
                "[1,2,3][2]",
                "true == false & false | true",
                "a=b - a[2] != b + a | b + c & d",
                "arr.length / 2",
                "person_pair.first.age / 10",
                "-a + -b / !c",
                "a==b + c<d + a<=b + 1>2 + e>=f",
                "(1/2 + (x+4) / 4) / ((x-5)/2 + (x+4)/(x-5))",
                r#"a + b/2 - c/(x * 4) * (3 + 4/(5+"hello there"))"#,
                r#"a + func_call(a, "b" + 2, (a+b) * [1, "abc", (a+b)/2] / 2) / 2"#,
            ],
            &["", "let", "*", "a=", "(a", "a<="],
            Parser::parse_expression,
        );
    }

    #[test]
    fn parse_empty_statement() {
        test_parser(&[";"], &["", "1", "ret 2;"], Parser::parse_empty_statement)
    }

    #[test]
    fn parse_simple_statement() {
        test_parser(
            &["1;", "a+b;", "(a+b);"],
            &["", "1", "ret 2;", ";"],
            Parser::parse_simple_statement,
        )
    }

    #[test]
    fn parse_return_statement() {
        test_parser(
            &["ret 1;", "ret (a+b);", "ret func(call);"],
            &["ret", "ret ;", "ret 1"],
            Parser::parse_return_statement,
        )
    }

    #[test]
    fn parse_let_statement() {
        test_parser(
            &[
                r#"let msg : [char] = "Hello World";"#,
                "let numbers: [int] = [1,2,3,4];",
                "let primes_numbers: [real] = [2.0, 3.0, 5.0];",
            ],
            &["", "let count = 0;", "let count: int = 0", "count: int = 0"],
            Parser::parse_let_statement,
        )
    }

    #[test]
    fn parse_if_statement() {
        test_parser(
            &[
                "if (true);",
                "if (true); else;",
                "if ((a+b)/2) { a; } else ret 2;",
            ],
            &["if", "if (true)", "if (true) a", "if (true) a; else "],
            Parser::parse_if_statement,
        )
    }

    #[test]
    fn parse_while_statement() {
        test_parser(
            &[
                "while (true);",
                "while (true) ret 2;",
                "while ((a+b)/2) { a; }",
            ],
            &["while", "while (true)", "while (true) a", "while (true a"],
            Parser::parse_while_statement,
        )
    }

    #[test]
    fn parse_compound_statement() {
        test_parser(
            &[
                "{}",
                "{;}",
                "{ ret a; }",
                "{ let a: [int] = 4; }",
                r#"{ while (count <= 5) { print("Hello World"); } }"#,
            ],
            &["", "{", "}", "{ a = 2 }", "{ 2 }"],
            Parser::parse_compound_statement,
        )
    }

    #[test]
    fn parse_statement() {
        test_parser(
            &[
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
            ],
            &[
                "",
                "{",
                "x",
                "let",
                "*",
                "ret",
                "let a == 2;",
                "while a = b {",
                "if (a = b) }",
            ],
            Parser::parse_statement,
        );
    }

    #[test]
    fn parse_expression_list() {
        test_parser(
            &["(a, b, (c+d)/2 + b/4)", "((a + b/2 + c*(a+b)/d))"],
            &["", "(", "a: int", "(a, b,()"],
            Parser::parse_expression_list,
        )
    }

    #[test]
    fn parse_function_parameters() {
        test_parser(
            &[
                "(a: int, b: [bool])",
                "(a: [[int]])",
                "(a: int, b: bool, c: char, d: [int], e: real)",
            ],
            &["(1a: int)", "(a: _1", "a: int", "(a int)"],
            Parser::parse_function_parameters,
        );
    }

    #[test]
    fn parse_extern_function() {
        test_parser(
            &[
                "extern int main();",
                "extern bool main();",
                "extern int main(a: int, b : int, c: int);",
                "extern [bool] main();",
            ],
            &[
                "extern int main(){}",
                "extern int ();",
                "extern int main(c: int;",
                "extern int main(a: int)",
            ],
            Parser::parse_extern_function,
        );
    }

    #[test]
    fn parse_function() {
        test_parser(
            &[
                "int main();",
                "bool main(){}",
                "int main(a: int, b : int, c: int);",
                "[bool] main(){}",
            ],
            &[
                "int main);",
                "int ();",
                "int main(c: int;",
                "int main(a: int)",
            ],
            Parser::parse_function,
        );
    }

    #[test]
    fn complex() {
        test_parser_valid(
            &[r#"
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
            
            nil print(b: [char], a: int) {
                while (a) {
                    print(b);
                    a = a - 1;
                }

            }
            
            int sum(a: int, b: int) {
                ret a + b;
            }
        "#],
            Parser::parse,
        );
    }
}
