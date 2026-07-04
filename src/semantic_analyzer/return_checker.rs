use super::errors::TypeErr;
use crate::parser::*;

/// Verifies that every non-`nil` function returns a value on all control-flow
/// paths. A `nil` function needs no explicit return.
pub struct ReturnChecker {
    errors: Vec<TypeErr>,
}

impl Default for ReturnChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl ReturnChecker {
    pub fn new() -> ReturnChecker {
        ReturnChecker { errors: Vec::new() }
    }

    pub fn check(&self) -> Result<(), &[TypeErr]> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(&self.errors)
        }
    }

    fn always_returns(stmt: &Statement) -> bool {
        match stmt {
            Statement::While(_, _) => false,
            Statement::Empty | Statement::Simple(_) | Statement::Let(_, _) => false,
            Statement::If(_, _, None) => false,
            Statement::Return(_) => true,
            Statement::If(_, if_case, Some(else_case)) => {
                Self::always_returns(&if_case.node) && Self::always_returns(&else_case.node)
            }
            Statement::Compound(stmts) => stmts.iter().any(|s| Self::always_returns(&s.node)),
            // TODO: compound statements shouldn't have any statements after the first
            // always_returns statement. This should be a warning and not an error.
        }
    }
}

impl ASTVisitor for ReturnChecker {
    fn visit_function(&mut self, func: &Spanned<Function>) {
        if func.node.return_type.is_some() && !Self::always_returns(&func.node.statement.node) {
            self.errors.push(TypeErr {
                msg: "Function with non-nil return type does not return on all paths",
                span: func.span.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer,
        parser::{self, ASTVisitor},
    };

    use super::ReturnChecker;

    fn returns_ok(source: &str) -> bool {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let mut checker = ReturnChecker::new();
        checker.visit_module(&module);
        checker.check().is_ok()
    }

    #[test]
    fn straight_line_return_is_ok() {
        assert!(returns_ok("int f() { ret 1; }"));
    }

    #[test]
    fn missing_return_is_error() {
        assert!(!returns_ok("int f() { let x: int = 1; }"));
    }

    #[test]
    fn nil_function_needs_no_return() {
        assert!(returns_ok("nil f() { let x: int = 1; }"));
    }

    #[test]
    fn both_if_branches_return_is_ok() {
        assert!(returns_ok(
            "int f(a: bool) { if (a) { ret 1; } else { ret 2; } }"
        ));
    }

    #[test]
    fn if_without_else_is_error() {
        assert!(!returns_ok("int f(a: bool) { if (a) { ret 1; } }"));
    }

    #[test]
    fn only_one_branch_returns_is_error() {
        assert!(!returns_ok(
            "int f(a: bool) { if (a) { ret 1; } else { let x: int = 2; } }"
        ));
    }

    #[test]
    fn while_does_not_guarantee_return() {
        assert!(!returns_ok("int f(a: bool) { while (a) { ret 1; } }"));
    }

    #[test]
    fn return_after_while_is_ok() {
        assert!(returns_ok("int f(a: bool) { while (a) { } ret 0; }"));
    }
}
