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

#[cfg(test)]
mod tests {
    use super::super::test_support::check_cases;

    #[test]
    fn test_conditions_must_be_bool() {
        check_cases(&[
            (r#"int main() { if (true) { } return 0; }"#, true),
            (r#"int main() { while (false) { } return 0; }"#, true),
            (r#"int main() { if (1) { } return 0; }"#, false),
            (r#"int main() { while (1) { } return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_for_statement_types() {
        check_cases(&[
            (
                r#"int main() { for (let i: int = 0; i < 3; i = i + 1) { i; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { for (let i: int = 0; i; i = i + 1) { } return 0; }"#,
                false,
            ),
            (
                r#"void f() { } int main() { for (let i: int = 0; i < 3; f()) { } return 0; }"#,
                true,
            ),
        ]);
    }

    #[test]
    fn test_for_range_statement_types() {
        check_cases(&[
            (
                "int main() { let s = 0; for x | [1, 2, 3] { s = s + x; } return s; }",
                true,
            ),
            (
                r#"int main() { let n = 0; for c | "hi" { if (c == 'h') { n = n + 1; } } return n; }"#,
                true,
            ),
            (
                "int main() { for row | [[1], [2]] { for v | row { return v; } } return 0; }",
                true,
            ),
            ("int main() { for x | 5 { } return 0; }", false),
        ]);
    }

    #[test]
    fn test_bare_return_matrix() {
        check_cases(&[
            (
                r#"void f(a: bool) { if (a) { return; } a = !a; } int main() { return 0; }"#,
                true,
            ),
            (r#"int f() { return 1; } int main() { return 0; }"#, true),
            (r#"int f() { return; } int main() { return 0; }"#, false),
            (r#"void f() { return 1; } int main() { return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_return_type_must_match() {
        check_cases(&[
            (r#"int f() { return 1; } int main() { return 0; }"#, true),
            (r#"real f() { return 1.0; } int main() { return 0; }"#, true),
            (
                r#"int f() { return true; } int main() { return 0; }"#,
                false,
            ),
            (r#"real f() { return 1; } int main() { return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_let_type_inference() {
        check_cases(&[
            (r#"int main() { let x = 5; return x; }"#, true),
            (
                r#"int main() { let x = 5; let y = x + 1; return y; }"#,
                true,
            ),
            (
                r#"int main() { let r = 1.5 * 2.0; let ok = r > 2.0; if (ok) { return 1; } return 0; }"#,
                true,
            ),
            (r#"int main() { let a = [1, 2, 3]; return a[0]; }"#, true),
            (
                r#"struct P { x: int } int main() { let p = new P; return p.x; }"#,
                true,
            ),
            (
                r#"int f(a: int) { return a; } int main() { let x = f(41) + 1; return x; }"#,
                true,
            ),
            (r#"int main() { let x = 5; return x + 1.0; }"#, false),
            (r#"int main() { let x = 5.0; return x; }"#, false),
            (
                r#"void f() { } int main() { let x = f(); return 0; }"#,
                false,
            ),
            (r#"int main() { let x = []; return 0; }"#, false),
            (
                r#"int main() { for (let i = 0; i < 3; i = i + 1) { i; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { for (let b = true; b; b = !b) { } return 0; }"#,
                true,
            ),
        ]);
    }
}
