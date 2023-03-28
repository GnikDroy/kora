use crate::parser::*;

pub trait ASTVisitor: Sized {
    fn visit_integer_literal(&mut self, i: &isize) {}
    fn visit_real_literal(&mut self, r: &f64) {}
    fn visit_boolean_literal(&mut self, b: &bool) {}
    fn visit_string_literal(&mut self, s: &String) {}
    fn visit_identifier(&mut self, ident: &String) {}
    fn visit_typename(&mut self, typename: &Typename) {}

    fn visit_identifier_type_pair(&mut self, pair: &IdentifierTypePair) {
        walk_identifier_type_pair(self, pair);
    }

    fn visit_array(&mut self, exprs: &Vec<Expression>) {
        walk_array(self, exprs);
    }

    fn visit_binary_expression(
        &mut self,
        left: &Expression,
        op: &BinaryOperator,
        right: &Expression,
    ) {
        walk_binary_expression(self, left, op, right);
    }

    fn visit_unary_expression(&mut self, op: &UnaryOperator, expr: &Expression) {
        walk_unary_expression(self, op, expr);
    }

    fn visit_call_expression(&mut self, expr: &Expression, exprs: &Vec<Expression>) {
        walk_call_expression(self, expr, exprs);
    }

    fn visit_expression(&mut self, expr: &Expression) {
        walk_expression(self, expr);
    }

    fn visit_simple_statement(&mut self, expr: &Expression) {
        walk_simple_statement(self, expr);
    }

    fn visit_return_statement(&mut self, expr: &Expression) {
        walk_return_statement(self, expr);
    }

    fn visit_compound_statement(&mut self, stmts: &Vec<Statement>) {
        walk_compound_statement(self, stmts);
    }

    fn visit_let_statement(&mut self, pair: &IdentifierTypePair, expr: &Expression) {
        walk_let_statement(self, pair, expr);
    }

    fn visit_while_statement(&mut self, cond: &Expression, stmt: &Statement) {
        walk_while_statement(self, cond, stmt);
    }

    fn visit_if_statement(
        &mut self,
        cond: &Expression,
        if_case: &Statement,
        else_case: Option<&Statement>,
    ) {
        walk_if_statement(self, cond, if_case, else_case);
    }

    fn visit_statement(&mut self, stmt: &Statement) {
        walk_statement(self, stmt);
    }

    fn visit_function(&mut self, func: &Function) {
        walk_function(self, func);
    }

    fn visit_module(&mut self, module: &Module) {
        walk_module(self, module);
    }
}

pub fn walk_identifier_type_pair<V: ASTVisitor>(visitor: &mut V, pair: &IdentifierTypePair) {
    visitor.visit_identifier(&pair.name);
    visitor.visit_typename(&pair.typename);
}

pub fn walk_array<V: ASTVisitor>(visitor: &mut V, exprs: &Vec<Expression>) {
    for e in exprs.iter() {
        visitor.visit_expression(e);
    }
}

pub fn walk_call_expression<V: ASTVisitor>(
    visitor: &mut V,
    expr: &Expression,
    exprs: &Vec<Expression>,
) {
    visitor.visit_expression(expr);
    for expr in exprs.iter() {
        visitor.visit_expression(expr);
    }
}

pub fn walk_unary_expression<V: ASTVisitor>(visitor: &mut V, _: &UnaryOperator, expr: &Expression) {
    visitor.visit_expression(expr);
}

pub fn walk_binary_expression<V: ASTVisitor>(
    visitor: &mut V,
    left: &Expression,
    _: &BinaryOperator,
    right: &Expression,
) {
    visitor.visit_expression(left);
    visitor.visit_expression(right);
}

pub fn walk_expression<V: ASTVisitor>(visitor: &mut V, expr: &Expression) {
    match expr {
        Expression::IntegerLiteral(i) => {
            visitor.visit_integer_literal(i);
        }
        Expression::BooleanLiteral(b) => {
            visitor.visit_boolean_literal(b);
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
        Expression::Variable(var) => {
            visitor.visit_identifier(var);
        }
        Expression::UnaryExpression(op, expr) => {
            visitor.visit_unary_expression(op, expr);
        }
        Expression::BinaryExpression(left, op, right) => {
            visitor.visit_binary_expression(left, op, right);
        }
        Expression::CallExpression(expr, exprs) => {
            visitor.visit_call_expression(expr, exprs);
        }
    }
}

pub fn walk_simple_statement<V: ASTVisitor>(visitor: &mut V, expr: &Expression) {
    visitor.visit_expression(expr);
}

pub fn walk_compound_statement<V: ASTVisitor>(visitor: &mut V, stmts: &Vec<Statement>) {
    for s in stmts.iter() {
        visitor.visit_statement(s);
    }
}

pub fn walk_return_statement<V: ASTVisitor>(visitor: &mut V, expr: &Expression) {
    visitor.visit_expression(expr);
}

pub fn walk_let_statement<V: ASTVisitor>(
    visitor: &mut V,
    pair: &IdentifierTypePair,
    expr: &Expression,
) {
    visitor.visit_identifier_type_pair(pair);
    visitor.visit_expression(expr);
}

pub fn walk_while_statement<V: ASTVisitor>(visitor: &mut V, cond: &Expression, stmt: &Statement) {
    visitor.visit_expression(cond);
    visitor.visit_statement(stmt);
}

pub fn walk_if_statement<V: ASTVisitor>(
    visitor: &mut V,
    cond: &Expression,
    if_case: &Statement,
    else_case: Option<&Statement>,
) {
    visitor.visit_expression(cond);
    visitor.visit_statement(if_case);
    if let Some(else_case) = else_case {
        visitor.visit_statement(else_case);
    }
}

pub fn walk_statement<V: ASTVisitor>(visitor: &mut V, stmt: &Statement) {
    match stmt {
        Statement::Empty => {}
        Statement::Simple(expr) => {
            visitor.visit_simple_statement(expr);
        }
        Statement::CompoundStatement(stmts) => {
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

pub fn walk_function<V: ASTVisitor>(visitor: &mut V, func: &Function) {
    visitor.visit_typename(&func.ret_type);
    visitor.visit_identifier(&func.name);
    for arg in func.args.iter() {
        visitor.visit_identifier_type_pair(arg);
    }
    visitor.visit_statement(&func.statement);
}

pub fn walk_module<V: ASTVisitor>(visitor: &mut V, module: &Module) {
    for func in module.functions.iter() {
        visitor.visit_function(func);
    }
}
