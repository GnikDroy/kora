use super::TypeChecker;
use crate::parser::*;
use crate::semantic_analyzer::errors::TypeErr;

impl ASTVisitor for TypeChecker<'_> {
    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.current_return_type = func.node.return_type.clone();
        walk_function(self, func);
    }

    fn visit_let_statement(
        &mut self,
        name: &Spanned<String>,
        typename: Option<&Type>,
        expr: &Spanned<Expression>,
    ) {
        match typename {
            Some(typename) => self.ensure_type(expr, typename),
            None => match self.get_expression_type(expr) {
                Ok(typename) => {
                    let id = self.symbols.symbol_id_of_declaration(name.id).unwrap();
                    self.inferred.insert(id, typename);
                }
                Err(e) => self.errors.push(e),
            },
        }
        walk_let_statement(self, name, typename, expr);
    }

    fn visit_if_statement(
        &mut self,
        cond: &Spanned<Expression>,
        if_case: &Spanned<Statement>,
        else_case: Option<&Spanned<Statement>>,
    ) {
        self.ensure_type(cond, &Type::Bool);
        walk_if_statement(self, cond, if_case, else_case);
    }

    fn visit_while_statement(&mut self, cond: &Spanned<Expression>, stmt: &Spanned<Statement>) {
        self.ensure_type(cond, &Type::Bool);
        walk_while_statement(self, cond, stmt);
    }

    fn visit_simple_statement(&mut self, expr: &Spanned<Expression>) {
        self.check_statement_expression(expr);
        walk_simple_statement(self, expr);
    }

    fn visit_for_statement(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) {
        // The init must be visited first so an unannotated `let` is inferred
        // before the condition and step refer to it.
        self.visit_statement(init);
        self.ensure_type(cond, &Type::Bool);
        self.check_statement_expression(step);
        self.visit_statement(body);
    }

    fn visit_return_statement(&mut self, expr: Option<&Spanned<Expression>>, span: &Span) {
        match (self.current_return_type.clone(), expr) {
            (Some(ret_type), Some(expr)) => self.ensure_type(expr, &ret_type),
            (Some(_), None) => self.errors.push(TypeErr {
                msg: "A function with a return type must return a value",
                span: span.clone(),
            }),
            (None, Some(expr)) => self.errors.push(TypeErr {
                msg: "A void function cannot return a value",
                span: expr.span.clone(),
            }),
            (None, None) => {}
        }
        walk_return_statement(self, expr);
    }
}
