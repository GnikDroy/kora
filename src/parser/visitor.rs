use crate::parser::*;

pub trait ASTVisitor: Sized {
    fn visit_enter_scope(&mut self) {}

    fn visit_exit_scope(&mut self) {}

    fn visit_integer_literal(&mut self, _: &isize) {}

    fn visit_real_literal(&mut self, _: &f64) {}

    fn visit_boolean_literal(&mut self, _: &bool) {}

    fn visit_char_literal(&mut self, _: &u8) {}

    fn visit_string_literal(&mut self, _: &String) {}

    fn visit_identifier(&mut self, _: &String) {}

    fn visit_typename(&mut self, _: &Type) {}

    fn visit_identifier_type_pair(&mut self, pair: &Spanned<IdentifierTypePair>) {
        walk_identifier_type_pair(self, pair);
    }

    fn visit_array(&mut self, exprs: &[Spanned<Expression>]) {
        walk_array(self, exprs);
    }

    fn visit_binary_expression(
        &mut self,
        left: &Spanned<Expression>,
        op: &BinaryOp,
        right: &Spanned<Expression>,
    ) {
        walk_binary_expression(self, left, op, right);
    }

    fn visit_unary_expression(&mut self, op: &UnaryOp, expr: &Spanned<Expression>) {
        walk_unary_expression(self, op, expr);
    }

    fn visit_call_expression(&mut self, expr: &Spanned<Expression>, exprs: &[Spanned<Expression>]) {
        walk_call_expression(self, expr, exprs);
    }

    fn visit_cast_expression(&mut self, expr: &Spanned<Expression>, typename: &Type) {
        walk_cast_expression(self, expr, typename);
    }

    fn visit_array_index_expression(
        &mut self,
        left: &Spanned<Expression>,
        right: &Spanned<Expression>,
    ) {
        walk_array_index_expression(self, left, right);
    }

    fn visit_access_expression(&mut self, left: &Spanned<Expression>, member: &str) {
        walk_access_expression(self, left, member);
    }

    fn visit_construct_expression(
        &mut self,
        typename: &Type,
        size: &Option<Box<Spanned<Expression>>>,
    ) {
        walk_construct_expression(self, typename, size);
    }

    fn visit_expression(&mut self, expr: &Spanned<Expression>) {
        walk_expression(self, expr);
    }

    fn visit_empty_statement(&mut self) {}

    fn visit_simple_statement(&mut self, expr: &Spanned<Expression>) {
        walk_simple_statement(self, expr);
    }

    fn visit_return_statement(&mut self, expr: &Spanned<Expression>) {
        walk_return_statement(self, expr);
    }

    fn visit_compound_statement(&mut self, stmts: &[Spanned<Statement>]) {
        walk_compound_statement(self, stmts);
    }

    fn visit_let_statement(
        &mut self,
        pair: &Spanned<IdentifierTypePair>,
        expr: &Spanned<Expression>,
    ) {
        walk_let_statement(self, pair, expr);
    }

    fn visit_while_statement(&mut self, cond: &Spanned<Expression>, stmt: &Spanned<Statement>) {
        walk_while_statement(self, cond, stmt);
    }

    fn visit_if_statement(
        &mut self,
        cond: &Spanned<Expression>,
        if_case: &Spanned<Statement>,
        else_case: Option<&Spanned<Statement>>,
    ) {
        walk_if_statement(self, cond, if_case, else_case);
    }

    fn visit_statement(&mut self, stmt: &Spanned<Statement>) {
        walk_statement(self, stmt);
    }

    fn visit_function(&mut self, func: &Spanned<Function>) {
        walk_function(self, func);
    }

    fn visit_struct(&mut self, _: &Spanned<Struct>) {}

    fn visit_extern_function(&mut self, func: &Spanned<ExternFunction>) {
        walk_extern_function(self, func);
    }

    fn visit_module(&mut self, module: &Module) {
        walk_module(self, module);
    }
}

pub fn walk_identifier_type_pair<V: ASTVisitor>(
    visitor: &mut V,
    pair: &Spanned<IdentifierTypePair>,
) {
    visitor.visit_identifier(&pair.node.name);
    visitor.visit_typename(&pair.node.typename);
}

pub fn walk_array<V: ASTVisitor>(visitor: &mut V, exprs: &[Spanned<Expression>]) {
    for e in exprs.iter() {
        visitor.visit_expression(e);
    }
}

pub fn walk_call_expression<V: ASTVisitor>(
    visitor: &mut V,
    expr: &Spanned<Expression>,
    exprs: &[Spanned<Expression>],
) {
    visitor.visit_expression(expr);
    for expr in exprs.iter() {
        visitor.visit_expression(expr);
    }
}

pub fn walk_cast_expression<V: ASTVisitor>(
    visitor: &mut V,
    expr: &Spanned<Expression>,
    typename: &Type,
) {
    visitor.visit_expression(expr);
    visitor.visit_typename(typename);
}

pub fn walk_array_index_expression<V: ASTVisitor>(
    visitor: &mut V,
    left: &Spanned<Expression>,
    right: &Spanned<Expression>,
) {
    visitor.visit_expression(left);
    visitor.visit_expression(right);
}

pub fn walk_access_expression<V: ASTVisitor>(
    visitor: &mut V,
    left: &Spanned<Expression>,
    _member: &str,
) {
    visitor.visit_expression(left);
}

pub fn walk_construct_expression<V: ASTVisitor>(
    visitor: &mut V,
    typename: &Type,
    size: &Option<Box<Spanned<Expression>>>,
) {
    visitor.visit_typename(typename);
    if let Some(size) = size {
        visitor.visit_expression(size);
    }
}

pub fn walk_unary_expression<V: ASTVisitor>(
    visitor: &mut V,
    _: &UnaryOp,
    expr: &Spanned<Expression>,
) {
    visitor.visit_expression(expr);
}

pub fn walk_binary_expression<V: ASTVisitor>(
    visitor: &mut V,
    left: &Spanned<Expression>,
    _: &BinaryOp,
    right: &Spanned<Expression>,
) {
    visitor.visit_expression(left);
    visitor.visit_expression(right);
}

pub fn walk_expression<V: ASTVisitor>(visitor: &mut V, expr: &Spanned<Expression>) {
    match &expr.node {
        Expression::IntegerLiteral(i) => {
            visitor.visit_integer_literal(i);
        }
        Expression::BoolLiteral(b) => {
            visitor.visit_boolean_literal(b);
        }
        Expression::CharLiteral(c) => {
            visitor.visit_char_literal(c);
        }
        Expression::StringLiteral(s) => {
            visitor.visit_string_literal(s);
        }
        Expression::RealLiteral(r) => {
            visitor.visit_real_literal(r);
        }
        Expression::Array(exprs) => {
            visitor.visit_array(exprs);
        }
        Expression::Identifier(var) => {
            visitor.visit_identifier(var);
        }
        Expression::Unary(op, expr) => {
            visitor.visit_unary_expression(op, expr);
        }
        Expression::Binary(left, op, right) => {
            visitor.visit_binary_expression(left, op, right);
        }
        Expression::Call(expr, exprs) => {
            visitor.visit_call_expression(expr, exprs);
        }
        Expression::Cast(expr, typename) => {
            visitor.visit_cast_expression(expr, typename);
        }
        Expression::ArrayIndex(left, right) => {
            visitor.visit_array_index_expression(left, right);
        }
        Expression::Access(left, member) => {
            visitor.visit_access_expression(left, member);
        }
        Expression::Construct(typename, size) => {
            visitor.visit_construct_expression(typename, size);
        }
    }
}

pub fn walk_simple_statement<V: ASTVisitor>(visitor: &mut V, expr: &Spanned<Expression>) {
    visitor.visit_expression(expr);
}

pub fn walk_compound_statement<V: ASTVisitor>(visitor: &mut V, stmts: &[Spanned<Statement>]) {
    visitor.visit_enter_scope();
    for s in stmts.iter() {
        visitor.visit_statement(s);
    }
    visitor.visit_exit_scope();
}

pub fn walk_return_statement<V: ASTVisitor>(visitor: &mut V, expr: &Spanned<Expression>) {
    visitor.visit_expression(expr);
}

pub fn walk_let_statement<V: ASTVisitor>(
    visitor: &mut V,
    pair: &Spanned<IdentifierTypePair>,
    expr: &Spanned<Expression>,
) {
    visitor.visit_identifier_type_pair(pair);
    visitor.visit_expression(expr);
}

pub fn walk_while_statement<V: ASTVisitor>(
    visitor: &mut V,
    cond: &Spanned<Expression>,
    stmt: &Spanned<Statement>,
) {
    visitor.visit_expression(cond);
    visitor.visit_statement(stmt);
}

pub fn walk_if_statement<V: ASTVisitor>(
    visitor: &mut V,
    cond: &Spanned<Expression>,
    if_case: &Spanned<Statement>,
    else_case: Option<&Spanned<Statement>>,
) {
    visitor.visit_expression(cond);
    visitor.visit_statement(if_case);
    if let Some(else_case) = else_case {
        visitor.visit_statement(else_case);
    }
}

pub fn walk_statement<V: ASTVisitor>(visitor: &mut V, stmt: &Spanned<Statement>) {
    match &stmt.node {
        Statement::Empty => visitor.visit_empty_statement(),
        Statement::Simple(expr) => {
            visitor.visit_simple_statement(expr);
        }
        Statement::Compound(stmts) => {
            visitor.visit_compound_statement(stmts);
        }
        Statement::Return(expr) => {
            visitor.visit_return_statement(expr);
        }
        Statement::Let(pair, expr) => {
            visitor.visit_let_statement(pair, expr);
        }
        Statement::While(cond, stmt) => {
            visitor.visit_while_statement(cond, stmt);
        }
        Statement::If(cond, if_case, else_case) => {
            visitor.visit_if_statement(cond, if_case, else_case.as_deref())
        }
    }
}

pub fn walk_function<V: ASTVisitor>(visitor: &mut V, func: &Spanned<Function>) {
    visitor.visit_enter_scope();
    visitor.visit_typename(&func.node.return_type);
    visitor.visit_identifier(&func.node.name);
    for arg in func.node.arguments.iter() {
        visitor.visit_identifier_type_pair(arg);
    }
    visitor.visit_statement(&func.node.statement);
    visitor.visit_exit_scope();
}

pub fn walk_extern_function<V: ASTVisitor>(visitor: &mut V, func: &Spanned<ExternFunction>) {
    visitor.visit_enter_scope();
    visitor.visit_typename(&func.node.return_type);
    visitor.visit_identifier(&func.node.name);
    for arg in func.node.arguments.iter() {
        visitor.visit_identifier_type_pair(arg);
    }
    visitor.visit_exit_scope();
}

pub fn walk_module<V: ASTVisitor>(visitor: &mut V, module: &Module) {
    visitor.visit_enter_scope();
    for func in module.extern_functions.iter() {
        visitor.visit_extern_function(func);
    }

    for _struct in module.structs.iter() {
        visitor.visit_struct(_struct);
    }

    for func in module.functions.iter() {
        visitor.visit_function(func);
    }

    visitor.visit_exit_scope();
}
