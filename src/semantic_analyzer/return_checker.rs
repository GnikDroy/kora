use super::errors::TypeErr;
use crate::parser::*;

/// Verifies that every non-`void` function returns a value on all control-flow
/// paths. A `void` function needs no explicit return.
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
            Statement::While(_, _) | Statement::For(_, _, _, _) => false,
            Statement::Break | Statement::Continue => false,
            Statement::Empty | Statement::Simple(_) | Statement::Let(_, _, _) => false,
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
                msg: "Function with non-void return type does not return on all paths",
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
    fn test_straight_line_return_is_ok() {
        assert!(returns_ok("int f() { return 1; }"));
    }

    #[test]
    fn test_missing_return_is_error() {
        assert!(!returns_ok("int f() { let x: int = 1; }"));
    }

    #[test]
    fn test_nil_function_needs_no_return() {
        assert!(returns_ok("void f() { let x: int = 1; }"));
    }

    #[test]
    fn test_both_if_branches_return_is_ok() {
        assert!(returns_ok(
            "int f(a: bool) { if (a) { return 1; } else { return 2; } }"
        ));
    }

    #[test]
    fn test_if_without_else_is_error() {
        assert!(!returns_ok("int f(a: bool) { if (a) { return 1; } }"));
    }

    #[test]
    fn test_only_one_branch_returns_is_error() {
        assert!(!returns_ok(
            "int f(a: bool) { if (a) { return 1; } else { let x: int = 2; } }"
        ));
    }

    #[test]
    fn test_while_does_not_guarantee_return() {
        assert!(!returns_ok("int f(a: bool) { while (a) { return 1; } }"));
    }

    #[test]
    fn test_return_after_while_is_ok() {
        assert!(returns_ok("int f(a: bool) { while (a) { } return 0; }"));
    }

    #[test]
    fn test_for_does_not_guarantee_return() {
        assert!(!returns_ok(
            "int f() { for (let i: int = 0; true; i) { return 1; } }"
        ));
        assert!(returns_ok(
            "int f() { for (let i: int = 0; true; i) { } return 0; }"
        ));
    }

    #[test]
    fn test_empty_body_non_void_is_error() {
        assert!(!returns_ok("int f() { }"));
        assert!(returns_ok("void f() { }"));
    }

    #[test]
    fn test_nested_compound_return_is_ok() {
        assert!(returns_ok("int f() { { return 1; } }"));
    }

    #[test]
    fn test_method_bodies_are_checked() {
        assert!(returns_ok(
            "struct P { x: int } impl P { int f(self) { return 1; } }"
        ));
        assert!(!returns_ok(
            "struct P { x: int } impl P { int f(self) { let y: int = 1; } }"
        ));
    }

    #[test]
    fn test_return_followed_by_dead_code_is_ok() {
        // A `return` anywhere in the block satisfies the check, even with
        // (currently unwarned) statements after it.
        assert!(returns_ok("int f() { return 1; let x: int = 2; }"));
    }

    #[test]
    fn test_if_then_trailing_return_is_ok() {
        assert!(returns_ok(
            "int f(a: bool) { if (a) { return 1; } return 2; }"
        ));
    }

    #[test]
    fn test_nested_if_all_branches_return_is_ok() {
        assert!(returns_ok(
            "int f(a: bool) { if (a) { if (a) { return 1; } else { return 2; } } else { return 3; } }"
        ));
    }

    #[test]
    fn test_nested_if_missing_inner_else_is_error() {
        assert!(!returns_ok(
            "int f(a: bool) { if (a) { if (a) { return 1; } } else { return 3; } }"
        ));
    }

    #[test]
    fn test_bare_return_satisfies_path_check() {
        assert!(returns_ok("int f() { return; }"));
    }

    #[test]
    fn test_void_function_with_early_return_is_ok() {
        assert!(returns_ok("void f(a: bool) { if (a) { return; } }"));
    }

    #[test]
    fn test_functions_are_checked_independently() {
        assert!(returns_ok("int f() { return 1; } int g() { return 2; }"));
        assert!(!returns_ok("int f() { return 1; } int g() { }"));
    }
}
