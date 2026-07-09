mod ast;
mod errors;
mod visitor;

pub use ast::*;
pub use errors::*;
pub use visitor::*;

use crate::lexer::{Keyword, Position, Symbol, Token, TokenInfo};

pub struct Parser {
    tokens: Vec<TokenInfo>,
    last_end: Position,
    source: SourceId,
    next_index: u32,
}

impl Parser {
    pub fn new(tokens: Vec<TokenInfo>) -> Parser {
        Parser::with_source(tokens, SourceId::ANON)
    }

    pub fn with_source(mut tokens: Vec<TokenInfo>, source: SourceId) -> Parser {
        tokens.reverse();
        Parser {
            tokens,
            last_end: Position::default(),
            source,
            next_index: 0,
        }
    }

    fn spanned<T>(&mut self, node: T, span: Span) -> Spanned<T> {
        let id = NodeId {
            source: self.source,
            index: self.next_index,
        };
        self.next_index += 1;
        Spanned::new(node, span, id)
    }

    fn peek(&mut self) -> Result<&TokenInfo, ParseErr> {
        self.tokens.last().ok_or(ParseErr {
            msg: "Unexpected EOF.",
            token: None,
        })
    }

    fn pop(&mut self) -> Result<TokenInfo, ParseErr> {
        let token = self.tokens.pop().ok_or(ParseErr {
            msg: "Unexpected EOF.",
            token: None,
        })?;
        self.last_end = token.end.clone();
        Ok(token)
    }

    fn current_start(&self) -> Position {
        self.tokens
            .last()
            .map_or_else(|| self.last_end.clone(), |t| t.start.clone())
    }

    fn span_from(&self, start: Position) -> Span {
        Span {
            source: self.source,
            start,
            end: self.last_end.clone(),
        }
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
            && let Token::IntegerLiteral(num) = token.token
        {
            self.pop().unwrap();
            return Some(Ok(Expression::IntegerLiteral(num)));
        }
        None
    }

    fn parselet_char_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::CharLiteral(_) = token.token
        {
            let token = self.pop();
            if let Ok(token) = token
                && let Token::CharLiteral(c) = token.token
            {
                return Some(Ok(Expression::CharLiteral(c)));
            }
        }
        None
    }

    fn parselet_string_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::StringLiteral(_) = token.token
        {
            let token = self.pop();
            if let Ok(token) = token
                && let Token::StringLiteral(s) = token.token
            {
                return Some(Ok(Expression::StringLiteral(s)));
            }
        }
        None
    }

    fn parselet_boolean_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && matches!(
                token.token,
                Token::Keyword(Keyword::True) | Token::Keyword(Keyword::False)
            )
        {
            let token = self.pop().unwrap();
            return match token.token {
                Token::Keyword(Keyword::True) => Some(Ok(Expression::BoolLiteral(true))),
                Token::Keyword(Keyword::False) => Some(Ok(Expression::BoolLiteral(false))),
                _ => None,
            };
        }
        None
    }

    fn parselet_none_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && matches!(token.token, Token::Keyword(Keyword::None))
        {
            self.pop().unwrap();
            return Some(Ok(Expression::NoneLiteral));
        }
        None
    }

    fn parselet_real_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::RealLiteral(r) = token.token
        {
            self.pop().unwrap();
            return Some(Ok(Expression::RealLiteral(r)));
        }
        None
    }

    fn parselet_identifier(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::Identifier(_) = token.token
        {
            let token = self.pop();
            if let Ok(token) = token
                && let Token::Identifier(r) = token.token
            {
                return Some(Ok(Expression::Identifier(r)));
            }
        }
        None
    }

    fn parselet_array_literal(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::Symbol(Symbol::LeftBracket) = token.token
        {
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
            && let Token::Symbol(Symbol::Minus) = token.token
        {
            self.pop().unwrap();
            let expr = self
                .pratt_parser(UnaryOp::Negate.get_binding_power())
                .map(|e| Expression::Unary(UnaryOp::Negate, Box::new(e)));
            return Some(expr);
        }
        None
    }

    fn parselet_not_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::Symbol(Symbol::Exclam) = token.token
        {
            self.pop().unwrap();
            let expr = self
                .pratt_parser(UnaryOp::Not.get_binding_power())
                .map(|e| Expression::Unary(UnaryOp::Not, Box::new(e)));
            return Some(expr);
        }
        None
    }

    fn parselet_new_operator(&mut self) -> Option<Result<Expression, ParseErr>> {
        let token = self.peek();
        if let Ok(token) = token
            && let Token::Keyword(Keyword::New) = token.token
        {
            let expr = || -> Result<Expression, ParseErr> {
                self.pop().unwrap();
                let typename = self.parse_typename()?;
                let token = self.peek();
                if let Ok(token) = token
                    && let Token::Symbol(Symbol::LeftBracket) = token.token
                {
                    self.pop().unwrap();
                    let expr = self
                        .pratt_parser(0)
                        .map(|e| Expression::Construct(typename, Some(Box::new(e))))?;
                    self.pop_token(
                        Token::Symbol(Symbol::RightBracket),
                        "Expected ] after array constructor: new <type>[<expr>]",
                    )?;
                    Ok(expr)
                } else if let Ok(token) = token
                    && let Token::Symbol(Symbol::LeftBrace) = token.token
                {
                    let fields = self.parse_generic_delimited(
                        Token::Symbol(Symbol::LeftBrace),
                        Token::Symbol(Symbol::RightBrace),
                        Token::Symbol(Symbol::Comma),
                        Parser::parse_field_initializer,
                    )?;
                    Ok(Expression::StructLiteral(typename, fields))
                } else {
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
            && let Token::Symbol(Symbol::LeftParen) = token.token
        {
            self.pop().unwrap();
            let expr = self.pratt_parser(0).and_then(|e| {
                self.pop_token(
                    Token::Symbol(Symbol::RightParen),
                    "Expected closing paren ) in parenthesized expression: (<expr>)",
                )
                .map(|_| e.node)
            });
            return Some(expr);
        }
        None
    }

    fn parse_initial_expression(&mut self) -> Result<Spanned<Expression>, ParseErr> {
        let parselets = [
            Parser::parselet_integer_literal,
            Parser::parselet_char_literal,
            Parser::parselet_string_literal,
            Parser::parselet_boolean_literal,
            Parser::parselet_real_literal,
            Parser::parselet_none_literal,
            Parser::parselet_identifier,
            Parser::parselet_parenthesized_expression,
            Parser::parselet_array_literal,
            Parser::parselet_negate_operator,
            Parser::parselet_not_operator,
            Parser::parselet_new_operator,
        ];

        let start = self.current_start();
        let node = parselets.iter().find_map(|f| f(self)).unwrap_or_else(|| {
            Err(ParseErr {
                msg: "Expected expression: <expr>",
                token: self.tokens.last().cloned(),
            })
        })?;
        let span = self.span_from(start);
        Ok(self.spanned(node, span))
    }

    fn parselet_infix_function_call(
        &mut self,
        _: InfixOperator,
        term: Spanned<Expression>,
    ) -> Result<Expression, ParseErr> {
        let args = self.parse_expression_list();
        args.map(|args| Expression::Call(Box::new(term), args))
    }

    fn parselet_infix_binary_operators(
        &mut self,
        op: InfixOperator,
        binary_op: BinaryOp,
        left: Spanned<Expression>,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        let right = self.pratt_parser(op.get_binding_power());
        right.map(|right| Expression::Binary(Box::new(left), binary_op, Box::new(right)))
    }

    fn parselet_infix_cast_operator(
        &mut self,
        _: InfixOperator,
        left: Spanned<Expression>,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        self.parse_typename()
            .map(|t| Expression::Cast(Box::new(left), t))
    }

    fn parselet_infix_array_index(
        &mut self,
        _: InfixOperator,
        term: Spanned<Expression>,
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
        _: InfixOperator,
        term: Spanned<Expression>,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        let member = self.parse_identifier()?;
        Ok(Expression::Access(Box::new(term), member))
    }

    fn parselet_infix_unwrap(
        &mut self,
        _: InfixOperator,
        term: Spanned<Expression>,
    ) -> Result<Expression, ParseErr> {
        self.pop().unwrap();
        Ok(Expression::Unwrap(Box::new(term)))
    }

    fn parselet_infix_operators(
        &mut self,
        op: InfixOperator,
        term: Spanned<Expression>,
    ) -> Result<Expression, ParseErr> {
        match op {
            InfixOperator::Binary(BinaryOp::Cast) => self.parselet_infix_cast_operator(op, term),
            InfixOperator::Binary(o) => self.parselet_infix_binary_operators(op, o, term),
            InfixOperator::FunctionCall => self.parselet_infix_function_call(op, term),
            InfixOperator::ArrayIndex => self.parselet_infix_array_index(op, term),
            InfixOperator::Access => self.parselet_infix_access(op, term),
            InfixOperator::Unwrap => self.parselet_infix_unwrap(op, term),
        }
    }

    fn pratt_parser(
        &mut self,
        current_binding_power: u32,
    ) -> Result<Spanned<Expression>, ParseErr> {
        let mut term = self.parse_initial_expression()?;
        loop {
            if self.peek().is_ok() {
                let token = self.peek().unwrap();
                if let Ok(operator) = InfixOperator::try_from(token.token.clone()) {
                    let binding_power = operator.get_binding_power_real();
                    if binding_power > current_binding_power {
                        let start = term.span.start.clone();
                        let node = self.parselet_infix_operators(operator, term)?;
                        let span = self.span_from(start);
                        term = self.spanned(node, span);
                        continue;
                    }
                }
            }
            break Ok(term);
        }
    }

    fn parse_expression(&mut self) -> Result<Spanned<Expression>, ParseErr> {
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

    fn parse_field_initializer(
        &mut self,
    ) -> Result<(Spanned<String>, Spanned<Expression>), ParseErr> {
        let start = self.current_start();
        let name = self.parse_identifier()?;
        let name_span = self.span_from(start);
        let name = self.spanned(name, name_span);
        self.pop_token(
            Token::Symbol(Symbol::Colon),
            "Expected colon after struct literal field: <field>: <expr>",
        )?;
        let value = self.parse_expression()?;
        Ok((name, value))
    }

    fn parse_identifier_type_pair(&mut self) -> Result<Spanned<IdentifierTypePair>, ParseErr> {
        let start = self.current_start();
        let name = self.parse_identifier()?;

        self.pop_token(
            Token::Symbol(Symbol::Colon),
            "Expected colon after identifier: <identifier> : <type>",
        )?;

        let typename = self.parse_typename()?;
        let span = self.span_from(start);
        Ok(self.spanned(IdentifierTypePair { name, typename }, span))
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
            Token::Keyword(Keyword::Return),
            "Expected return statement: return <expr>;",
        )?;

        if self.peek()?.token == Token::Symbol(Symbol::Semicolon) {
            self.pop()?;
            return Ok(Statement::Return(None));
        }

        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon: return <expr>;",
        )?;
        Ok(Statement::Return(Some(expr)))
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
            "Expected let: let <identifier>[: <type>] = <expression>;",
        )?;

        let start = self.current_start();
        let name = self.parse_identifier()?;
        let span = self.span_from(start);
        let name = self.spanned(name, span);

        let typename = match self.peek() {
            Ok(token) if token.token == Token::Symbol(Symbol::Colon) => {
                self.pop().unwrap();
                Some(self.parse_typename()?)
            }
            _ => None,
        };

        self.pop_token(
            Token::Symbol(Symbol::Equal),
            "Expected =: let <identifier>[: <type>] = <expression>;",
        )?;

        let expr = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon: let <identifier>[: <type>] = <expression>;",
        )?;

        Ok(Statement::Let(name, typename, expr))
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

        if let Ok(token) = self.peek()
            && let Token::Keyword(Keyword::Else) = token.token
        {
            self.pop()?;
            let else_stmt = self.parse_statement()?;
            return Ok(Statement::If(
                expr,
                Box::new(stmt),
                Some(Box::new(else_stmt)),
            ));
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

    fn parse_for_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::For),
            "Expected for: for (<init> <cond>; <step>) <statement>",
        )?;

        self.pop_token(
            Token::Symbol(Symbol::LeftParen),
            "Expected (: for (<init> <cond>; <step>) <statement>",
        )?;

        // The init clause is a full statement (consumes its own `;`), restricted
        // to declarations, expressions, or nothing.
        let init_start = self.current_start();
        let init = match self.peek()?.token {
            Token::Symbol(Symbol::Semicolon) => self.parse_empty_statement(),
            Token::Keyword(Keyword::Let) => self.parse_let_statement(),
            _ => self.parse_simple_statement(),
        }?;
        let init_span = self.span_from(init_start);
        let init = self.spanned(init, init_span);

        let cond = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected ; after loop condition: for (<init> <cond>; <step>) <statement>",
        )?;

        let step = self.parse_expression()?;

        self.pop_token(
            Token::Symbol(Symbol::RightParen),
            "Expected ): for (<init> <cond>; <step>) <statement>",
        )?;

        let body = self.parse_statement()?;
        Ok(Statement::For(Box::new(init), cond, step, Box::new(body)))
    }

    fn parse_break_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(Token::Keyword(Keyword::Break), "Expected break: break;")?;
        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon: break;",
        )?;
        Ok(Statement::Break)
    }

    fn parse_continue_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Keyword(Keyword::Continue),
            "Expected continue: continue;",
        )?;
        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon: continue;",
        )?;
        Ok(Statement::Continue)
    }

    fn parse_empty_statement(&mut self) -> Result<Statement, ParseErr> {
        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Empty statements must end in semicolon",
        )?;
        Ok(Statement::Empty)
    }

    fn parse_statement(&mut self) -> Result<Spanned<Statement>, ParseErr> {
        let start = self.current_start();
        let token = self.peek()?;
        let node = match token.token {
            Token::Symbol(Symbol::Semicolon) => self.parse_empty_statement(),
            Token::Symbol(Symbol::LeftBrace) => self.parse_compound_statement(),
            Token::Keyword(Keyword::Return) => self.parse_return_statement(),
            Token::Keyword(Keyword::Let) => self.parse_let_statement(),
            Token::Keyword(Keyword::While) => self.parse_while_statement(),
            Token::Keyword(Keyword::For) => self.parse_for_statement(),
            Token::Keyword(Keyword::Break) => self.parse_break_statement(),
            Token::Keyword(Keyword::Continue) => self.parse_continue_statement(),
            Token::Keyword(Keyword::If) => self.parse_if_statement(),
            _ => self.parse_simple_statement(),
        }?;
        let span = self.span_from(start);
        Ok(self.spanned(node, span))
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

    fn parse_return_type(&mut self) -> Result<Option<Type>, ParseErr> {
        if matches!(self.peek()?.token, Token::Keyword(Keyword::Void)) {
            self.pop()?;
            Ok(None)
        } else {
            Ok(Some(self.parse_typename()?))
        }
    }

    fn parse_typename(&mut self) -> Result<Type, ParseErr> {
        let token = self.pop()?;
        let span = Span {
            source: self.source,
            start: token.start.clone(),
            end: token.end.clone(),
        };
        let base = match token.token {
            Token::Keyword(Keyword::Int) => Ok(Type::Int),
            Token::Keyword(Keyword::Real) => Ok(Type::Real),
            Token::Keyword(Keyword::Char) => Ok(Type::Char),
            Token::Keyword(Keyword::Bool) => Ok(Type::Bool),
            Token::Keyword(Keyword::String) => Ok(Type::Array(Box::new(Type::Char))),
            Token::Identifier(name) => Ok(Type::Struct(self.spanned(name, span))),
            Token::Symbol(Symbol::LeftBracket) => {
                self.tokens.push(token);
                self.parse_array_typename()
            }
            _ => Err(ParseErr {
                msg: "Expected type declaration: <type> | [<type>]",
                token: Some(token.clone()),
            }),
        }?;

        if let Ok(next) = self.peek()
            && matches!(next.token, Token::Symbol(Symbol::Question))
        {
            self.pop()?;
            if let Ok(next) = self.peek()
                && matches!(next.token, Token::Symbol(Symbol::Question))
            {
                return Err(ParseErr {
                    msg: "Nested optionals are not supported: write T?, not T??",
                    token: Some(self.peek()?.clone()),
                });
            }
            Ok(Type::Optional(Box::new(base)))
        } else {
            Ok(base)
        }
    }

    fn parse_expression_list(&mut self) -> Result<Vec<Spanned<Expression>>, ParseErr> {
        self.parse_generic_delimited(
            Token::Symbol(Symbol::LeftParen),
            Token::Symbol(Symbol::RightParen),
            Token::Symbol(Symbol::Comma),
            Parser::parse_expression,
        )
    }

    fn parse_function_parameters(&mut self) -> Result<Vec<Spanned<IdentifierTypePair>>, ParseErr> {
        self.parse_generic_delimited(
            Token::Symbol(Symbol::LeftParen),
            Token::Symbol(Symbol::RightParen),
            Token::Symbol(Symbol::Comma),
            Parser::parse_identifier_type_pair,
        )
    }

    fn parse_import(&mut self) -> Result<Spanned<Import>, ParseErr> {
        let start = self.current_start();
        self.pop_token(
            Token::Keyword(Keyword::Import),
            "Expected import declaration",
        )?;

        let token = self.pop()?;
        let Token::StringLiteral(path) = token.token else {
            return Err(ParseErr {
                msg: "Expected a quoted path after import: import \"path.kora\";",
                token: Some(token),
            });
        };

        let alias = if let Ok(TokenInfo {
            token: Token::Identifier(_),
            ..
        }) = self.peek()
        {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon ; to end import declaration",
        )?;
        let span = self.span_from(start);
        Ok(self.spanned(Import { path, alias }, span))
    }

    fn parse_extern_function(&mut self) -> Result<Spanned<ExternFunction>, ParseErr> {
        let start = self.current_start();
        self.pop_token(
            Token::Keyword(Keyword::Extern),
            "Expected function declaration",
        )?;

        let return_type = self.parse_return_type()?;
        let name = self.parse_identifier()?;
        let arguments = self.parse_function_parameters()?;

        self.pop_token(
            Token::Symbol(Symbol::Semicolon),
            "Expected semicolon ; to end extern function declaration",
        )?;

        let span = self.span_from(start);
        Ok(self.spanned(
            ExternFunction {
                return_type,
                name,
                arguments,
            },
            span,
        ))
    }

    fn parse_function(&mut self) -> Result<Spanned<Function>, ParseErr> {
        let start = self.current_start();
        let return_type = self.parse_return_type()?;
        let name = self.parse_identifier()?;
        let arguments = self.parse_function_parameters()?;

        // A function body must be a compound statement; there are no forward
        // declarations (all top-level functions are pre-declared by the resolver).
        let token = self.peek()?;
        if token.token != Token::Symbol(Symbol::LeftBrace) {
            return Err(ParseErr {
                msg: "Expected function body: <type> <name>(<params>) { <stmt> ... }",
                token: Some(token.clone()),
            });
        }
        let statement = self.parse_statement()?;

        let function = Function {
            return_type,
            name,
            arguments,
            statement,
        };
        let span = self.span_from(start);
        Ok(self.spanned(function, span))
    }

    fn parse_method_parameters(
        &mut self,
        struct_name: &Spanned<String>,
    ) -> Result<Vec<Spanned<IdentifierTypePair>>, ParseErr> {
        self.pop_token(
            Token::Symbol(Symbol::LeftParen),
            "Expected ( to start the method parameter list",
        )?;

        let start = self.current_start();
        let token = self.pop()?;
        if !matches!(&token.token, Token::Identifier(name) if name == "self") {
            return Err(ParseErr {
                msg: "The first parameter of a method must be self: <type> <name>(self, <params>)",
                token: Some(token),
            });
        }
        let self_span = self.span_from(start);
        let self_type =
            Type::Struct(self.spanned(struct_name.node.clone(), struct_name.span.clone()));
        let mut arguments = vec![self.spanned(
            IdentifierTypePair {
                name: "self".to_string(),
                typename: self_type,
            },
            self_span,
        )];

        while self.peek()?.token != Token::Symbol(Symbol::RightParen) {
            let token = self.peek()?;
            match &token.token {
                Token::Symbol(Symbol::Comma) => {
                    self.pop()?;
                    if self.peek()?.token == Token::Symbol(Symbol::RightParen) {
                        break;
                    }
                    arguments.push(self.parse_identifier_type_pair()?);
                }
                Token::Symbol(Symbol::Colon) => {
                    return Err(ParseErr {
                        msg: "self takes no type annotation; it has the type of the impl'd struct",
                        token: Some(token.clone()),
                    });
                }
                _ => {
                    return Err(ParseErr {
                        msg: "Expected , or ) in the method parameter list",
                        token: Some(token.clone()),
                    });
                }
            }
        }
        self.pop()?;
        Ok(arguments)
    }

    fn parse_method(
        &mut self,
        struct_name: &Spanned<String>,
    ) -> Result<Spanned<Function>, ParseErr> {
        let start = self.current_start();
        let return_type = self.parse_return_type()?;
        let name = self.parse_identifier()?;
        let arguments = self.parse_method_parameters(struct_name)?;

        let token = self.peek()?;
        if token.token != Token::Symbol(Symbol::LeftBrace) {
            return Err(ParseErr {
                msg: "Expected method body: <type> <name>(self, <params>) { <stmt> ... }",
                token: Some(token.clone()),
            });
        }
        let statement = self.parse_statement()?;

        let function = Function {
            return_type,
            name,
            arguments,
            statement,
        };
        let span = self.span_from(start);
        Ok(self.spanned(function, span))
    }

    fn parse_impl(&mut self) -> Result<Spanned<Impl>, ParseErr> {
        let start = self.current_start();
        self.pop_token(
            Token::Keyword(Keyword::Impl),
            "Expected method block to start with 'impl': impl <struct> {...}",
        )?;
        let name_start = self.current_start();
        let name = self.parse_identifier()?;
        let name_span = self.span_from(name_start);
        let struct_name = self.spanned(name, name_span);

        self.pop_token(
            Token::Symbol(Symbol::LeftBrace),
            "Expected { to open the impl block: impl <struct> {...}",
        )?;
        let mut functions = vec![];
        while self.peek()?.token != Token::Symbol(Symbol::RightBrace) {
            functions.push(self.parse_method(&struct_name)?);
        }
        self.pop()?;

        let span = self.span_from(start);
        Ok(self.spanned(
            Impl {
                struct_name,
                functions,
            },
            span,
        ))
    }

    fn parse_struct(&mut self) -> Result<Spanned<Struct>, ParseErr> {
        let start = self.current_start();
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
        let span = self.span_from(start);
        Ok(self.spanned(Struct { name, members }, span))
    }

    fn parse_module(&mut self) -> Result<Module, ParseErr> {
        let mut module = Module {
            ..Default::default()
        };
        while !self.tokens.is_empty() {
            let token = self.peek()?;
            match token.token {
                Token::Keyword(Keyword::Import) => {
                    module.imports.push(self.parse_import()?);
                }
                Token::Keyword(Keyword::Struct) => {
                    module.structs.push(self.parse_struct()?);
                }
                Token::Keyword(Keyword::Extern) => {
                    module.extern_functions.push(self.parse_extern_function()?);
                }
                Token::Keyword(Keyword::Impl) => {
                    module.impls.push(self.parse_impl()?);
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

    use super::{Parser, SourceId};

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
    fn test_parse_module() {
        test_parser(
            &["", "int main(){}", "extern int a(); int b(){} int c(){}"],
            &[
                "i",
                "int main()",
                "int main();",
                "int a(){} int b(){} int (){}",
            ],
            Parser::parse_module,
        );
    }

    #[test]
    fn test_parse_import() {
        test_parser(
            &[
                r#"import "util.kora";"#,
                r#"import "a.kora"; import "b.kora"; int main(){}"#,
                r#"import "sub/thing.kora"; struct P { x: int }"#,
                r#"import "sub/util.kora" u; int main(){ return 0; }"#,
            ],
            &[
                r#"import util;"#,
                r#"import "util.kora""#,
                r#"import;"#,
                r#"import "util.kora" 3;"#,
            ],
            Parser::parse_module,
        );
    }

    #[test]
    fn test_node_ids_are_unique_per_source() {
        let toks_a = lexer::Lexer::lex("int a(){ return 1; }").expect("lex");
        let ma = Parser::with_source(toks_a, SourceId(1))
            .parse()
            .expect("parse a");

        let toks_b = lexer::Lexer::lex("int b(){ return 2; }").expect("lex");
        let mb = Parser::with_source(toks_b, SourceId(2))
            .parse()
            .expect("parse b");

        let id_a = ma.functions[0].id;
        let id_b = mb.functions[0].id;
        assert_eq!(id_a.index, id_b.index);
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn test_parse_struct() {
        test_parser(
            &["struct Person { age: int, name: [char]}", "struct Foo {}"],
            &["struct Foo", "struct {}", "struct Foo { foo, bar }"],
            Parser::parse_struct,
        );
    }

    #[test]
    fn test_parse_struct_literal() {
        test_parser(
            &[
                "new Point { x: 1, y: 2 }",
                "new Point { x: 1, y: 2, }",
                "new Foo {}",
                "new Line { a: new Point { x: 0, y: 0 }, b: p }",
                "new Bag { items: [1, 2], total: 1 + 2 }",
            ],
            &[
                "new Point { x }",
                "new Point { x: }",
                "new Point { x: 1 y: 2 }",
                "new Point { 1: 2 }",
                "new Point { x: 1",
            ],
            Parser::parse_expression,
        );
    }

    #[test]
    fn test_parse_impl() {
        test_parser(
            &[
                "impl Person {}",
                "impl Person { int age(self) { return 1; } }",
                "impl Person { void grow(self, by: int) {} void reset(self,) {} }",
                "impl P { P me(self) { return self; } bool near(self, other: P, d: int) { return true; } }",
            ],
            &[
                "impl {}",
                "impl Person",
                "impl Person { int age() { return 1; } }",
                "impl Person { int age(self: Person) { return 1; } }",
                "impl Person { int age(by: int) { return 1; } }",
                "impl Person { int age(self, by) { return 1; } }",
                "impl Person { int age(self) }",
                "impl Person { struct Inner {} }",
            ],
            Parser::parse_impl,
        );
    }

    #[test]
    fn test_parse_identifier() {
        test_parser(
            &["foo", "_before_2000", "TestCase"],
            &["", "2000", "{ 0 }"],
            Parser::parse_identifier,
        );
    }

    #[test]
    fn test_parse_array_typename() {
        test_parser(
            &["[[int]]", "[real]", "[foo]"],
            &["", "[", "[int", "int]", "[[[["],
            Parser::parse_array_typename,
        );
    }

    #[test]
    fn test_parse_typename() {
        test_parser(
            &[
                "int",
                "real",
                "char",
                "[[int]]",
                "custom_type",
                "string",
                "[string]",
                "int?",
                "[int]?",
                "[int?]",
                "Node?",
                "string?",
            ],
            &[
                "", "2000", "{0}", "[int", "]", "void", "[void]", "int??", "?int",
            ],
            Parser::parse_typename,
        );
    }

    #[test]
    fn test_parse_optionals() {
        test_parser(
            &[
                "none",
                "x!",
                "a.b!",
                "x!.next",
                "arr[0]!",
                "f()!",
                "x == none",
                "node.next != none",
                "-x!",
                "x!!",
                "!x",
            ],
            &["!", "== none"],
            Parser::parse_expression,
        );
    }

    #[test]
    fn test_string_is_an_alias_for_char_array() {
        let tokens = lexer::Lexer::lex("string").expect("lex");
        let typename = parser::Parser::new(tokens).parse_typename().expect("parse");
        assert_eq!(typename, parser::Type::Array(Box::new(parser::Type::Char)));
    }

    #[test]
    fn test_parse_return_type() {
        test_parser(
            &["void", "int", "[char]", "custom_type"],
            &["", "2000", "]"],
            Parser::parse_return_type,
        );
    }

    #[test]
    fn test_parse_identifier_type_pair() {
        test_parser(
            &["a: int", "a: [[int]]", "ident: real", "ident: custom_type"],
            &["", "a: ", "a int", "1: int", "int: int"],
            Parser::parse_identifier_type_pair,
        );
    }

    #[test]
    fn test_parse_expression() {
        test_parser(
            &[
                "1-2-3%3",
                "(1.234 as real) as int",
                r#"'a'+"abc"+'a'"#,
                "[1,2,3][2]",
                "true == false && false || true",
                "a=b - a[2] != b + a || b + c && d",
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
    fn test_parse_empty_statement() {
        test_parser(
            &[";"],
            &["", "1", "return 2;"],
            Parser::parse_empty_statement,
        )
    }

    #[test]
    fn test_parse_simple_statement() {
        test_parser(
            &["1;", "a+b;", "(a+b);"],
            &["", "1", "return 2;", ";"],
            Parser::parse_simple_statement,
        )
    }

    #[test]
    fn test_parse_return_statement() {
        test_parser(
            &[
                "return 1;",
                "return (a+b);",
                "return func(call);",
                "return;",
                "return ;",
            ],
            &["return", "return 1"],
            Parser::parse_return_statement,
        )
    }

    #[test]
    fn test_parse_let_statement() {
        test_parser(
            &[
                r#"let msg : [char] = "Hello World";"#,
                "let numbers: [int] = [1,2,3,4];",
                "let primes_numbers: [real] = [2.0, 3.0, 5.0];",
                "let count = 0;",
                "let half = 1.0 / 2.0;",
            ],
            &[
                "",
                "let count: int = 0",
                "count: int = 0",
                "let count: = 0;",
            ],
            Parser::parse_let_statement,
        )
    }

    #[test]
    fn test_parse_if_statement() {
        test_parser(
            &[
                "if (true);",
                "if (true); else;",
                "if ((a+b)/2) { a; } else return 2;",
            ],
            &["if", "if (true)", "if (true) a", "if (true) a; else "],
            Parser::parse_if_statement,
        )
    }

    #[test]
    fn test_parse_while_statement() {
        test_parser(
            &[
                "while (true);",
                "while (true) return 2;",
                "while ((a+b)/2) { a; }",
            ],
            &["while", "while (true)", "while (true) a", "while (true a"],
            Parser::parse_while_statement,
        )
    }

    #[test]
    fn test_parse_for_statement() {
        test_parser(
            &[
                "for (let i: int = 0; i < 10; i = i + 1) { i; }",
                "for (; true; x) ;",
                "for (x; x == y; f(x)) { break; continue; }",
            ],
            &[
                "for",
                "for (let i: int = 0 i < 10; i = i + 1) ;",
                "for (;;) ;",
                "for (; true; x)",
                "for (; true; x;) ;",
            ],
            Parser::parse_for_statement,
        )
    }

    #[test]
    fn test_parse_break_and_continue() {
        test_parser(
            &["break;"],
            &["break", "break 1;"],
            Parser::parse_break_statement,
        );
        test_parser(
            &["continue;"],
            &["continue", "continue 1;"],
            Parser::parse_continue_statement,
        );
    }

    #[test]
    fn test_parse_compound_statement() {
        test_parser(
            &[
                "{}",
                "{;}",
                "{ return a; }",
                "{ let a: [int] = 4; }",
                r#"{ while (count <= 5) { print("Hello World"); } }"#,
            ],
            &["", "{", "}", "{ a = 2 }", "{ 2 }"],
            Parser::parse_compound_statement,
        )
    }

    #[test]
    fn test_parse_statement() {
        test_parser(
            &[
                "{}",
                ";",
                "1;",
                "return 1;",
                "let a : int = b;",
                "if (1);",
                "if (1); else ;",
                "while (1) ;",
                "{ return 1; let a : int = b; }",
                "if (a + b) { while (1) { a = b; } return 1; }",
                "if (true) { a = b; let a: bool = true; } else { c = d; print(a); }",
            ],
            &[
                "",
                "{",
                "x",
                "let",
                "*",
                "return",
                "let a == 2;",
                "while a = b {",
                "if (a = b) }",
            ],
            Parser::parse_statement,
        );
    }

    #[test]
    fn test_parse_expression_list() {
        test_parser(
            &["(a, b, (c+d)/2 + b/4)", "((a + b/2 + c*(a+b)/d))"],
            &["", "(", "a: int", "(a, b,()"],
            Parser::parse_expression_list,
        )
    }

    #[test]
    fn test_parse_function_parameters() {
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
    fn test_parse_extern_function() {
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
    fn test_parse_function() {
        test_parser(
            &[
                "int main(){}",
                "bool main(){ return true; }",
                "int main(a: int, b : int, c: int){ return a; }",
                "[bool] main(){}",
            ],
            &[
                "int main);",
                "int ();",
                "int main(c: int;",
                "int main(a: int)",
                // Function bodies must be compound statements.
                "int main();",
                "int main() return 1;",
                "void main() let x: int = 1;",
            ],
            Parser::parse_function,
        );
    }

    #[test]
    fn test_expression_error_carries_offending_token() {
        let tokens = lexer::Lexer::lex("int main() { let x: int = ); }").expect("lex");
        let err = Parser::new(tokens)
            .parse()
            .expect_err("expected parse error");
        assert!(err.token.is_some(), "err: {:?}", err);
    }

    #[test]
    fn test_complex() {
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
                return a;
            }
            
            void print(b: [char], a: int) {
                while (a) {
                    print(b);
                    a = a - 1;
                }

            }

            int sum(a: int, b: int) {
                return a + b;
            }
        "#],
            Parser::parse,
        );
    }

    #[test]
    fn test_parse_literal_expressions() {
        test_parser(
            &[
                "0",
                "42",
                "3.14",
                "0.0",
                "'a'",
                "'\\n'",
                r#""hello""#,
                r#""""#,
                "true",
                "false",
            ],
            &["", ".5", ")", "="],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_array_literal_expressions() {
        test_parser(
            &[
                "[]",
                "[1]",
                "[1, 2, 3]",
                "[1, 2, 3,]",
                "[[1], [2, 3]]",
                r#"["a", "b"]"#,
                "[a + b, c * d]",
            ],
            &["[", "[1", "[,]", "[1 2]", "[1,, 2]", "[1, 2"],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_new_expression() {
        test_parser(
            &[
                "new Foo",
                "new int",
                "new [int]",
                "new int[10]",
                "new [int][n]",
                "new custom_type[a + b]",
            ],
            &["new", "new 2", "new int[", "new int[10", "new int[]"],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_unary_expressions() {
        test_parser(
            &[
                "-a",
                "!b",
                "- -a",
                "!!a",
                "-(a + b)",
                "!(a == b)",
                "-arr[0]",
            ],
            &["-", "!", "-!", "! ="],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_member_access_and_index() {
        test_parser(
            &[
                "a.b",
                "a.b.c",
                "a[0]",
                "a[0][1]",
                "a.b[0].c",
                "f().x",
                "arr[i].length",
                "person.name[0]",
            ],
            &["a.", "a[", "a.1", "a[]", "a.b.", ".a"],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_cast_expressions() {
        test_parser(
            &[
                "a as int",
                "a as real as int",
                "(a + b) as [int]",
                "arr[0] as char",
                "-x as real",
            ],
            &["a as", "a as 2", "a as as", "as int"],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_call_expressions() {
        test_parser(
            &[
                "f()",
                "f(a)",
                "f(a, b, c)",
                "f(g(x), h(y))",
                "obj.method(a)",
                "f(a + b, c * d)",
            ],
            &["f(", "f(a b)", "f(,)"],
            Parser::parse_expression,
        )
    }

    #[test]
    fn test_parse_nested_struct() {
        test_parser(
            &[
                "struct Point { x: int, y: int }",
                "struct Line { start: Point, end: Point }",
                "struct Matrix { rows: [[real]] }",
                "struct Trailing { a: int, }",
            ],
            &[
                "struct { x: int }",
                "struct S { x: int",
                "struct S { x }",
                "struct S { x: int y: int }",
            ],
            Parser::parse_struct,
        )
    }

    #[test]
    fn test_parse_nested_control_flow() {
        test_parser(
            &[
                "if (a) if (b); else;",
                "while (a) while (b) ;",
                "for (let i: int = 0; i < n; i = i + 1) { if (i) continue; else break; }",
                "if (a) { while (b) { for (;c;d) ; } }",
            ],
            &["if (a) else ;", "while () ;", "for () ;"],
            Parser::parse_statement,
        )
    }
}
