use crate::{
    lexer,
    parser::{self, ASTVisitor},
    semantic_analyzer::symbol_resolver::Resolver,
};

use super::TypeChecker;
use crate::loader::LoadedProgram;
use crate::semantic_analyzer::symbol_resolver::test_support::{load_program, resolve_program};

fn program_type_checks(program: &LoadedProgram) -> bool {
    let symbols = resolve_program(program).expect("resolve");
    let mut checker = TypeChecker::new(&symbols);
    for module in &program.modules {
        checker.visit_module(&module.module);
    }
    checker.check().is_ok()
}

#[test]
fn test_module_qualified_call_type_checks() {
    let program = load_program(
        "main.kora",
        vec![
            (
                "main.kora",
                r#"import "util.kora"; int main() { return util.helper(); }"#,
            ),
            ("util.kora", "int helper() { return 1; }"),
        ],
    );
    assert!(program_type_checks(&program));
}

#[test]
fn test_module_qualified_call_arity_is_checked() {
    let program = load_program(
        "main.kora",
        vec![
            (
                "main.kora",
                r#"import "util.kora"; int main() { return util.helper(1); }"#,
            ),
            ("util.kora", "int helper() { return 1; }"),
        ],
    );
    assert!(!program_type_checks(&program));
}

#[test]
fn test_valid() {
    let source = r#"
            struct Person {
                name: [char],
                age: int,
            }
            
            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                let d: char = 'a';
                let e: [Person] = [];
                e.push(new Person);
                e[0].name = "Name";
                e[0].age = 23;
                e.push(new Person);
                if (a - b == 1) {
                    print(a, b);
                }
                if (c / 2.0 == 10.0) {
                    let d: real = c / 2.0 + 10.0;
                }
                return a;
            }
            
            void print(a: int, b: int) {
                while (a == 10) {
                    print(b, 1);
                    a = a - 1;
                }
            }
            
            int sum(a: int, b: int) {
                return a + b;
            }
        "#;

    let tokens = lexer::Lexer::lex(source).expect("lex");
    let mut parser = parser::Parser::new(tokens);
    let module = parser.parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);
    assert_eq!(
        checker.check().is_ok(),
        true,
        "source_text: {}, errors: {:?}",
        source,
        checker.check().unwrap_err()
    );
}

#[test]
fn test_invalid() {
    let source = r#"
            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                if (a - b == 1) {
                    print(a, b);
                }
                if (c / 2.0 == 10.0) {
                    let d: real = c / 2.0 + 10;
                }
                2 = 4;
                return a;
            }
            
            void print(a: int, b: int) {
                while (a) {
                    print("Hello, World", 1);
                    a = a - 1;
                }
            }
            
            int sum(a: int, b: int) {
                return a + b;
            }
        "#;

    let tokens = lexer::Lexer::lex(source).expect("lex");
    let mut parser = parser::Parser::new(tokens);
    let module = parser.parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);
    assert_eq!(
        checker.check().is_err() && checker.check().unwrap_err().len() == 4,
        true,
        "source_text: {}, errors: {:?}",
        source,
        checker.check().unwrap()
    );
}

#[test]
fn test_error_carries_span() {
    // The `true` mismatched against `int` sits on line 3; the type error
    // must point there rather than at a default (0, 0) location.
    let source = "int main() {\n\n    let x: int = true;\n}\n";

    let tokens = lexer::Lexer::lex(source).expect("lex");
    let mut parser = parser::Parser::new(tokens);
    let module = parser.parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);

    let errors = checker.check().expect_err("expected a type error");
    assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    assert_eq!(errors[0].span.start.row, 3, "span: {:?}", errors[0].span);
    assert!(errors[0].span.start.col > 0, "span: {:?}", errors[0].span);
}

#[test]
fn test_int_division_yields_int() {
    let ok = r#"int main() { let x: int = 7 / 2; return 0; }"#;
    let bad = r#"int main() { let x: real = 7 / 2; return 0; }"#;

    for (source, expect_ok) in [(ok, true), (bad, false)] {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
    }
}

#[test]
fn test_array_method_types() {
    let cases = [
        (
            r#"int main() { let a: [int] = new int[3]; return a.len(); }"#,
            true,
        ),
        (r#"int main() { return "hi".len(); }"#, true),
        (
            r#"int main() { let a = [1]; a.push(2); a.insert(0, 3); return a.remove(1); }"#,
            true,
        ),
        (
            r#"int main() { let a = [1]; a.remove(0); return 0; }"#,
            true,
        ),
        (
            r#"int main() { let m = [[1], [2]]; m[0].push(3); return m.remove(0).len(); }"#,
            true,
        ),
        (r#"int main() { let a = [1, 2, 3]; return a.pop(); }"#, true),
        (
            r#"int main() { let a = [1, 2, 3]; return a.slice(1, 3).len(); }"#,
            true,
        ),
        (
            r#"int main() { let a = [1]; let b = [2]; a.extend(b); return a.len(); }"#,
            true,
        ),
        (
            r#"int main() { let s = "abc".slice(0, 1); return s.len(); }"#,
            true,
        ),
        (
            r#"int main() { let a = [[1]]; a.extend([[2]]); return a[1][0]; }"#,
            true,
        ),
        (
            r#"int main() { let a = [1]; let x = a.extend([2]); return 0; }"#,
            false,
        ),
        (r#"int main() { let a = [1]; a.pop(1); return 0; }"#, false),
        (
            r#"int main() { let a = [1]; let x: bool = a.pop(); return 0; }"#,
            false,
        ),
        (r#"int main() { let a = [1]; return a.slice(1); }"#, false),
        (
            r#"int main() { let a = [1]; return a.slice(0, 1); }"#,
            false,
        ),
        (
            r#"int main() { let a = [1]; a.extend(2); return 0; }"#,
            false,
        ),
        (
            r#"int main() { let a = [1]; let b = ["x"]; a.extend(b); return 0; }"#,
            false,
        ),
        (
            r#"real main() { let a: [int] = new int[3]; return a.len(); }"#,
            false,
        ),
        (r#"int main() { let a = [1]; return a.len(1); }"#, false),
        (
            r#"int main() { let a = [1]; a.push(true); return 0; }"#,
            false,
        ),
        (
            r#"int main() { let a = [1]; a.push(1, 2); return 0; }"#,
            false,
        ),
        (
            r#"int main() { let a = [1]; a.insert(1.0, 2); return 0; }"#,
            false,
        ),
        (
            r#"int main() { let a = [1]; let x: bool = a.remove(0); return 0; }"#,
            false,
        ),
        (r#"int main() { let a = [1]; return a.push(2); }"#, false),
        (
            r#"int main() { let a = [1]; let f = a.len; return 0; }"#,
            false,
        ),
        (r#"int main() { let n = 5; return n.len(); }"#, false),
    ];

    for (source, expect_ok) in cases {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
    }
}

#[test]
fn test_void_function_semantics() {
    let cases = [
        (r#"void f() { } int main() { f(); return 0; }"#, true),
        (r#"void f() { return 1; }"#, false),
        (
            r#"void f() { } int main() { let x: int = f(); return x; }"#,
            false,
        ),
        (r#"void f() { } int main() { return f(); }"#, false),
        (
            r#"void f() { } int g(a: int) { return a; } int main() { return g(f()); }"#,
            false,
        ),
    ];

    for (source, expect_ok) in cases {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
    }
}

#[test]
fn test_call_result_is_not_assignable() {
    let source = r#"
            int f() { return 1; }

            int main() {
                f() = 1;
                return 0;
            }
        "#;

    let tokens = lexer::Lexer::lex(source).expect("lex");
    let mut parser = parser::Parser::new(tokens);
    let module = parser.parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);

    let errors = checker
        .check()
        .expect_err("expected an unassignable-LHS error");
    assert_eq!(errors.len(), 1, "errors: {:?}", errors);
}

fn check_cases(cases: &[(&str, bool)]) {
    for (source, expect_ok) in cases {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        assert_eq!(checker.check().is_ok(), *expect_ok, "source: {}", source);
    }
}

#[test]
fn test_empty_array_literals_where_type_is_expected() {
    check_cases(&[
        (r#"int main() { let a: [int] = []; return a.len(); }"#, true),
        (
            r#"int main() { let a: [int] = []; a = []; a.push(1); return a[0]; }"#,
            true,
        ),
        (
            r#"int count(v: [int]) { return v.len(); } int main() { return count([]); }"#,
            true,
        ),
        (
            r#"[int] empty() { return []; } int main() { return empty().len(); }"#,
            true,
        ),
        (
            r#"int main() { let m: [[int]] = [[], [1]]; return m[0].len(); }"#,
            true,
        ),
        (
            r#"int main() { let m: [[int]] = []; m.push([]); m[0].push(1); return m[0][0]; }"#,
            true,
        ),
        (r#"int main() { let a = []; return 0; }"#, false),
        (r#"int main() { let a: int = []; return 0; }"#, false),
        (r#"int main() { let a: [int] = [true]; return 0; }"#, false),
        (r#"int main() { let a: [int] = [[]]; return 0; }"#, false),
        (r#"int main() { []; return 0; }"#, false),
    ]);
}

#[test]
fn test_struct_literals() {
    let prelude = r#"
            struct Point { x: int, y: int }
            struct Line { a: Point, b: Point }
            struct Bag { items: [int] }
        "#;
    let cases = [
        (
            r#"int main() { let p = new Point { x: 1, y: 2 }; return p.x + p.y; }"#,
            true,
        ),
        (
            r#"int main() { let l = new Line { a: new Point { x: 0, y: 0 }, b: new Point { x: 1, y: 1 } }; return l.b.x; }"#,
            true,
        ),
        (
            r#"int main() { let b = new Bag { items: [] }; b.items.push(3); return b.items[0]; }"#,
            true,
        ),
        (
            r#"int main() { return (new Point { x: 1, y: 2 }).x; }"#,
            true,
        ),
        (
            r#"int main() { let p = new Point { x: 1 }; return 0; }"#,
            false,
        ),
        (
            r#"int main() { let p = new Point { x: 1, y: 2, x: 3 }; return 0; }"#,
            false,
        ),
        (
            r#"int main() { let p = new Point { x: 1, y: 2, z: 3 }; return 0; }"#,
            false,
        ),
        (
            r#"int main() { let p = new Point { x: true, y: 2 }; return 0; }"#,
            false,
        ),
        (
            r#"int main() { let p = new int { x: 1 }; return 0; }"#,
            false,
        ),
        (
            r#"int main() { new Point { x: 1, y: 2 } = new Point { x: 3, y: 4 }; return 0; }"#,
            false,
        ),
    ];
    let sources: Vec<(String, bool)> = cases
        .iter()
        .map(|(body, ok)| (format!("{prelude}{body}"), *ok))
        .collect();
    let cases: Vec<(&str, bool)> = sources.iter().map(|(s, ok)| (s.as_str(), *ok)).collect();
    check_cases(&cases);
}

#[test]
fn test_method_calls() {
    let prelude = r#"
            struct P { x: int }
            impl P {
                int get(self) { return self.x; }
                void set(self, v: int) { self.x = v; }
                P me(self) { return self; }
            }
        "#;
    let cases = [
        (
            "int main() { let p = new P; p.set(3); return p.get(); }",
            true,
        ),
        (
            "int main() { let p = new P; return p.me().me().get(); }",
            true,
        ),
        ("int main() { return (new P).get(); }", true),
        ("int main() { let p = new P; return p.get(1); }", false),
        (
            "int main() { let p = new P; p.set(true); return 0; }",
            false,
        ),
        (
            "int main() { let p = new P; let v = p.set(1); return 0; }",
            false,
        ),
        (
            "int main() { let p = new P; let f = p.get; return 0; }",
            false,
        ),
        ("int main() { let p = new P; return p.nope(); }", false),
        ("int main() { let p = new P; return p.x(); }", false),
        ("int main() { let n = 5; return n.get(); }", false),
    ];
    let sources: Vec<(String, bool)> = cases
        .iter()
        .map(|(body, ok)| (format!("{prelude}{body}"), *ok))
        .collect();
    let cases: Vec<(&str, bool)> = sources.iter().map(|(s, ok)| (s.as_str(), *ok)).collect();
    check_cases(&cases);
}

#[test]
fn test_method_call_resolutions_are_recorded() {
    let source = r#"
            struct P { x: int }
            impl P { int get(self) { return self.x; } }
            int main() { let p = new P; return p.get(); }
        "#;
    let tokens = lexer::Lexer::lex(source).expect("lex");
    let module = parser::Parser::new(tokens).parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);
    checker.check().expect("check");
    let (_, method) = checker.method_calls.iter().next().expect("one method call");
    assert_eq!(symbols.symbol(*method).name, "get");
}

#[test]
fn test_modulo_is_int_only() {
    check_cases(&[
        (r#"int main() { return 7 % 2; }"#, true),
        (
            r#"int main() { let x: real = 7.0 % 2.0; return 0; }"#,
            false,
        ),
        (
            r#"int main() { let b: bool = true % false; return 0; }"#,
            false,
        ),
    ]);
}

#[test]
fn test_sizeless_new_is_structs_only() {
    check_cases(&[
        (
            r#"struct P { x: int } int main() { let p: P = new P; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let a: [int] = new int[3]; return 0; }"#,
            true,
        ),
        (r#"int main() { let x: int = new int; return x; }"#, false),
        (
            r#"int main() { let a: [int] = new [int]; return 0; }"#,
            false,
        ),
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
fn test_function_names_are_not_assignable() {
    check_cases(&[(
        r#"
                int f() { return 1; }
                int g() { return 2; }
                int main() { f = g; return 0; }
            "#,
        false,
    )]);
}

#[test]
fn test_binary_operator_type_rules() {
    check_cases(&[
        (r#"int main() { let x: int = 1 + 2 * 3; return x; }"#, true),
        (r#"int main() { let x: real = 1.0 + 2.0; return 0; }"#, true),
        (r#"int main() { let x: int = 1 + 2.0; return x; }"#, false),
        (r#"int main() { let x: bool = 1 < 2; return 0; }"#, true),
        (r#"int main() { let x: bool = 'a' < 'b'; return 0; }"#, true),
        (
            r#"int main() { let x: bool = true && false || true; return 0; }"#,
            true,
        ),
        (r#"int main() { let x: bool = 1 && 2; return 0; }"#, false),
        (
            r#"int main() { let x: char = 'a' + 'b'; return 0; }"#,
            false,
        ),
        (r#"int main() { let x: bool = 1 == 2.0; return 0; }"#, false),
    ]);
}

#[test]
fn test_unary_operator_type_rules() {
    check_cases(&[
        (r#"int main() { let x: int = -5; return x; }"#, true),
        (r#"int main() { let x: real = -5.0; return 0; }"#, true),
        (r#"int main() { let x: bool = !true; return 0; }"#, true),
        (r#"int main() { let x: bool = -true; return 0; }"#, false),
        (r#"int main() { let x: int = !5; return x; }"#, false),
    ]);
}

#[test]
fn test_array_literal_homogeneity() {
    check_cases(&[
        (
            r#"int main() { let a: [int] = [1, 2, 3]; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let a: [[int]] = [[1], [2]]; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let a: [int] = [1, 2.0]; return 0; }"#,
            false,
        ),
        (r#"int main() { let a: [int] = []; return 0; }"#, true),
    ]);
}

#[test]
fn test_array_indexing_types() {
    check_cases(&[
        (
            r#"int main() { let a: [int] = new int[3]; let x: int = a[0]; return x; }"#,
            true,
        ),
        (
            r#"int main() { let a: [int] = new int[3]; let x: int = a[1.0]; return x; }"#,
            false,
        ),
        (
            r#"int main() { let x: int = 5; let y: int = x[0]; return y; }"#,
            false,
        ),
    ]);
}

#[test]
fn test_struct_member_access_types() {
    check_cases(&[
        (
            r#"struct P { x: int } int main() { let p: P = new P; let a: int = p.x; return a; }"#,
            true,
        ),
        (
            r#"struct P { x: int } int main() { let p: P = new P; let a: int = p.y; return a; }"#,
            false,
        ),
        (
            r#"int main() { let x: int = 5; let a: int = x.y; return a; }"#,
            false,
        ),
        (
            r#"struct P { x: int } int main() { let p: P = new P; let a: real = p.x; return 0; }"#,
            false,
        ),
    ]);
}

#[test]
fn test_cast_rules() {
    check_cases(&[
        (r#"int main() { let x: real = 1 as real; return 0; }"#, true),
        (r#"int main() { let x: int = 1.5 as int; return x; }"#, true),
        (r#"int main() { let x: int = 'a' as int; return x; }"#, true),
        (
            r#"int main() { let x: int = true as int; return x; }"#,
            false,
        ),
        (r#"int main() { let x: int = 5 as int; return x; }"#, false),
    ]);
}

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
fn test_assignment_type_must_match() {
    check_cases(&[
        (r#"int main() { let x: int = 1; x = 2; return x; }"#, true),
        (
            r#"int main() { let x: real = 1.0; x = 2.0; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let x: int = 1; x = 2.0; return x; }"#,
            false,
        ),
        (
            r#"int main() { let a: [int] = new int[2]; a[0] = 5; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let a: int = 1; let b: int = 2; a = b = 3; return a; }"#,
            true,
        ),
    ]);
}

#[test]
fn test_unassignable_lhs_is_rejected() {
    check_cases(&[
        (r#"int main() { 2 = 4; return 0; }"#, false),
        (r#"int main() { let x: int = 1; -x = 2; return 0; }"#, false),
        (
            r#"int main() { let x: int = 1; (x + 1) = 2; return 0; }"#,
            false,
        ),
    ]);
}

#[test]
fn test_call_arity_and_argument_types() {
    check_cases(&[
        (
            r#"int add(a: int, b: int) { return a + b; } int main() { return add(1, 2); }"#,
            true,
        ),
        (
            r#"int add(a: int, b: int) { return a + b; } int main() { return add(1); }"#,
            false,
        ),
        (
            r#"int add(a: int, b: int) { return a + b; } int main() { return add(1, 2.0); }"#,
            false,
        ),
        (r#"int main() { let x: int = 5; return x(1); }"#, false),
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

#[test]
fn test_bitwise_operators_are_int_only() {
    check_cases(&[
        (r#"int main() { return 12 & 10 | 5 ^ 3; }"#, true),
        (r#"int main() { return 1 << 4 >> 2; }"#, true),
        (
            r#"int main() { let b = 5 & 2 == 0; if (b) { return 1; } return 0; }"#,
            true,
        ),
        (r#"int main() { let b = true & false; return 0; }"#, false),
        (r#"int main() { let r = 1.5 | 1.0; return 0; }"#, false),
        (r#"int main() { let r = 1.0 << 2; return 0; }"#, false),
    ]);
}

#[test]
fn test_array_equality_is_structural_and_recursive() {
    check_cases(&[
        (
            r#"int main() { let b: bool = [1, 2] == [1, 2]; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let s: string = "abc"; if (s == "quit") { return 1; } return 0; }"#,
            true,
        ),
        (
            r#"int main() { let s = "abc"; let b = s != "abc"; return 0; }"#,
            true,
        ),
        (
            r#"int main() { let b = [[1], [2]] == [[1], [2]]; return 0; }"#,
            true,
        ),
        (r#"int main() { let b = [1] == [1.0]; return 0; }"#, false),
        (r#"int main() { let b = [1] == 1; return 0; }"#, false),
        (r#"int main() { let b = "a" == 'a'; return 0; }"#, false),
        (r#"int main() { let b = [1] < [2]; return 0; }"#, false),
        (
            r#"struct P { x: int } int main() { let b = new P[1] == new P[1]; return 0; }"#,
            false,
        ),
    ]);
}

#[test]
fn test_method_names_are_not_reserved() {
    check_cases(&[
        (
            r#"int len(x: int) { return x; } int main() { let a = [1]; return len(a.len()); }"#,
            true,
        ),
        (
            r#"int push(x: int) { return x; } int main() { return push(3); }"#,
            true,
        ),
    ]);
}

#[test]
fn test_copy_intrinsic() {
    check_cases(&[
        (
            r#"int main() { let a = [1, 2]; let b = copy(a); b.push(3); return a.len(); }"#,
            true,
        ),
        (
            r#"struct P { x: int } int main() { let p = new P { x: 1 }; let q = copy(p); return q.x; }"#,
            true,
        ),
        (
            r#"int main() { let s = copy("hi"); return s.len(); }"#,
            true,
        ),
        (
            r#"int main() { let m = [[1]]; let n = copy(m); return n[0][0]; }"#,
            true,
        ),
        (r#"int main() { copy([1]); return 0; }"#, true),
        (r#"int main() { let x = copy(5); return 0; }"#, false),
        (r#"int main() { let x = copy('a'); return 0; }"#, false),
        (
            r#"int main() { let a = [1]; copy(a, a); return 0; }"#,
            false,
        ),
        (r#"int main() { let x = copy(); return 0; }"#, false),
    ]);
}

#[test]
fn test_sized_new_is_scalar_only() {
    check_cases(&[
        (r#"int main() { let a = new int[3]; return a[0]; }"#, true),
        (r#"int main() { let a = new char[8]; return 0; }"#, true),
        (r#"int main() { let a = new real[2]; return 0; }"#, true),
        (r#"int main() { let a = new bool[2]; return 0; }"#, true),
        (
            r#"struct P { x: int } int main() { let a = new P[3]; return 0; }"#,
            false,
        ),
        (r#"int main() { let a = new [int][3]; return 0; }"#, false),
        (r#"int main() { let a = new string[3]; return 0; }"#, false),
    ]);
}

#[test]
fn test_array_plus_is_concatenation() {
    check_cases(&[
        (r#"int main() { let a = [1] + [2]; return a.len(); }"#, true),
        (r#"int main() { let a = [1, 2] + [3]; return a[2]; }"#, true),
        (
            r#"int main() { let s: string = "ab" + "cd"; return s.len(); }"#,
            true,
        ),
        (
            r#"int main() { let m = [[1]] + [[2]]; return m[1][0]; }"#,
            true,
        ),
        (r#"int main() { let a = [1] + [2.0]; return 0; }"#, false),
        (r#"int main() { let a = [1] + 2; return 0; }"#, false),
        (r#"int main() { let a = 1 + [2]; return 0; }"#, false),
        (r#"int main() { let a = [1] - [2]; return 0; }"#, false),
    ]);
}

#[test]
fn test_string_alias_is_interchangeable_with_char_array() {
    check_cases(&[
        (
            r#"int main() { let s: string = "abc"; return s.len(); }"#,
            true,
        ),
        (
            r#"int main() { let s: string = "abc"; let t: [char] = s; t = s; s = t; return 0; }"#,
            true,
        ),
        (
            r#"string first_word() { return "hi"; }
                   void take(s: [char]) { }
                   int main() { take(first_word()); return 0; }"#,
            true,
        ),
        (
            r#"int main() { let words: [string] = ["a", "b"]; return words.len(); }"#,
            true,
        ),
        (
            r#"int main() { let s: string = "abc"; s[0] = 'x'; return s[0] as int; }"#,
            true,
        ),
        (r#"int main() { let s: string = 5; return 0; }"#, false),
        (r#"int main() { let s: string = 'a'; return 0; }"#, false),
    ]);
}

#[test]
fn test_expression_types_are_recorded() {
    use crate::parser::{Expression, Statement, Type};

    let source = r#"
            void f() { }
            int main() { let x: real = 1.5 + 2.5; f(); return 0; }
        "#;
    let tokens = lexer::Lexer::lex(source).expect("lex");
    let module = parser::Parser::new(tokens).parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);
    checker.check().expect("check");

    let body = &module.functions[1].node.statement;
    let Statement::Compound(stmts) = &body.node else {
        panic!("expected compound body");
    };
    let Statement::Let(_, _, init) = &stmts[0].node else {
        panic!("expected let statement");
    };
    let Expression::Binary(left, _, right) = &init.node else {
        panic!("expected binary initializer");
    };
    assert_eq!(checker.types.get(&init.id), Some(&Type::Real));
    assert_eq!(checker.types.get(&left.id), Some(&Type::Real));
    assert_eq!(checker.types.get(&right.id), Some(&Type::Real));

    let Statement::Simple(call) = &stmts[1].node else {
        panic!("expected call statement");
    };
    assert_eq!(checker.types.get(&call.id), None);
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
