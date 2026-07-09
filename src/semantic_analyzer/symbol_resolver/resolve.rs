use super::super::errors::TypeErr;
use super::collect::GlobalsCollector;
use super::table::{SymbolId, SymbolTable, is_intrinsic};
use super::{Scope, check_typename};
use crate::parser::*;

#[derive(Default)]
pub struct Resolver {
    table: SymbolTable,
    scopes: Vec<Scope>,
    errors: Vec<TypeErr>,
    loop_depth: usize,
}

impl Resolver {
    pub fn new() -> Resolver {
        Resolver::default()
    }

    pub fn resolve(mut self, modules: &[&Module]) -> Result<SymbolTable, Vec<TypeErr>> {
        let scopes = GlobalsCollector::new(&mut self.table, &mut self.errors).collect(modules);
        for (module, scope) in modules.iter().zip(scopes) {
            self.scopes.push(scope);
            for func in module.functions.iter() {
                self.visit_function(func);
            }
            for impl_ in module.impls.iter() {
                for func in impl_.node.functions.iter() {
                    self.visit_function(func);
                }
            }
            self.scopes.pop();
        }
        if self.errors.is_empty() {
            Ok(self.table)
        } else {
            Err(self.errors)
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(
        &mut self,
        declaration_id: NodeId,
        name: String,
        ty: Option<Type>,
        span: &Span,
    ) -> SymbolId {
        if is_intrinsic(&name) {
            self.errors.push(TypeErr {
                msg: "Cannot declare a name reserved for a compiler intrinsic",
                span: span.clone(),
            });
        }

        // Same-scope redeclaration is an error; shadowing an outer scope is fine.
        let duplicate = self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(&name));
        if duplicate {
            self.errors.push(TypeErr {
                msg: "Redeclaration in the same scope",
                span: span.clone(),
            });
        }

        let id = self.table.add_symbol(declaration_id, name.clone(), ty);
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, id);
        }
        id
    }

    fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

impl ASTVisitor for Resolver {
    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.push_scope();
        if let Some(return_type) = &func.node.return_type {
            self.visit_typename(return_type);
        }
        for pair in func.node.arguments.iter() {
            self.visit_typename(&pair.node.typename);
            self.declare(
                pair.id,
                pair.node.name.clone(),
                Some(pair.node.typename.clone()),
                &pair.span,
            );
        }
        self.visit_statement(&func.node.statement);
        self.pop_scope();
    }

    fn visit_typename(&mut self, ty: &Type) {
        check_typename(&self.table, &mut self.errors, ty);
    }

    fn visit_compound_statement(&mut self, stmts: &[Spanned<Statement>]) {
        self.push_scope();
        for stmt in stmts.iter() {
            self.visit_statement(stmt);
        }
        self.pop_scope();
    }

    fn visit_let_statement(
        &mut self,
        name: &Spanned<String>,
        typename: Option<&Type>,
        expr: &Spanned<Expression>,
    ) {
        // The initializer is resolved before the name is bound, so `let x = x;`
        // refers to an outer `x`, not itself.
        self.visit_expression(expr);
        if let Some(typename) = typename {
            self.visit_typename(typename);
        }
        self.declare(name.id, name.node.clone(), typename.cloned(), &name.span);
    }

    fn visit_while_statement(&mut self, cond: &Spanned<Expression>, stmt: &Spanned<Statement>) {
        self.visit_expression(cond);
        self.loop_depth += 1;
        self.push_scope();
        self.visit_statement(stmt);
        self.pop_scope();
        self.loop_depth -= 1;
    }

    fn visit_if_statement(
        &mut self,
        cond: &Spanned<Expression>,
        if_case: &Spanned<Statement>,
        else_case: Option<&Spanned<Statement>>,
    ) {
        self.visit_expression(cond);
        self.push_scope();
        self.visit_statement(if_case);
        self.pop_scope();
        if let Some(else_case) = else_case {
            self.push_scope();
            self.visit_statement(else_case);
            self.pop_scope();
        }
    }

    fn visit_for_statement(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) {
        self.push_scope();
        self.visit_statement(init);
        self.visit_expression(cond);
        self.visit_expression(step);
        self.loop_depth += 1;
        self.visit_statement(body);
        self.loop_depth -= 1;
        self.pop_scope();
    }

    fn visit_break_statement(&mut self, span: &Span) {
        if self.loop_depth == 0 {
            self.errors.push(TypeErr {
                msg: "break outside of a loop",
                span: span.clone(),
            });
        }
    }

    fn visit_continue_statement(&mut self, span: &Span) {
        if self.loop_depth == 0 {
            self.errors.push(TypeErr {
                msg: "continue outside of a loop",
                span: span.clone(),
            });
        }
    }

    fn visit_expression(&mut self, expr: &Spanned<Expression>) {
        if let Expression::Identifier(name) = &expr.node
            && !is_intrinsic(name)
        {
            match self.lookup(name) {
                Some(id) => {
                    self.table.uses.insert(expr.id, id);
                }
                None => self.errors.push(TypeErr {
                    msg: "Undefined identifier",
                    span: expr.span.clone(),
                }),
            }
        }
        walk_expression(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::{lexer, parser};

    use super::Resolver;

    fn resolve(source: &str) -> Result<super::SymbolTable, Vec<super::TypeErr>> {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        Resolver::new().resolve(&[&module])
    }

    fn resolve_program(
        program: &crate::loader::LoadedProgram,
    ) -> Result<super::SymbolTable, Vec<super::TypeErr>> {
        let modules: Vec<&parser::Module> = program.modules.iter().map(|m| &m.module).collect();
        Resolver::new().resolve(&modules)
    }

    fn load_program(
        entry: &str,
        files: Vec<(&'static str, &'static str)>,
    ) -> crate::loader::LoadedProgram {
        let map: std::collections::HashMap<String, String> = files
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let provider = move |p: &std::path::Path| p.to_str().and_then(|s| map.get(s)).cloned();
        crate::loader::Loader::new(&provider)
            .load(entry)
            .expect("load")
    }

    fn source_module<'a>(
        program: &'a crate::loader::LoadedProgram,
        path: &str,
    ) -> &'a crate::loader::LoadedModule {
        program
            .modules
            .iter()
            .find(|m| program.sources[m.id.0 as usize].path.to_str() == Some(path))
            .expect("module present")
    }

    fn fn_symbol_name(
        symbols: &super::SymbolTable,
        module: &crate::loader::LoadedModule,
        i: usize,
    ) -> String {
        let id = symbols
            .symbol_id_of_declaration(module.module.functions[i].id)
            .expect("function declared");
        symbols.symbol(id).name.clone()
    }

    #[test]
    fn test_program_stores_bare_names() {
        // Names are stored bare regardless of source; the backend mangles later.
        let program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return 0; }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        let symbols = resolve_program(&program).expect("resolve");
        assert_eq!(
            fn_symbol_name(&symbols, source_module(&program, "util.kora"), 0),
            "helper"
        );
        assert_eq!(
            fn_symbol_name(&symbols, source_module(&program, "main.kora"), 0),
            "main"
        );
    }

    #[test]
    fn test_program_hides_other_sources_from_bare_names() {
        // `main` imports `util` but calls its `helper` unqualified — invisible
        // without qualification, so resolution fails.
        let program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return helper(); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        assert!(resolve_program(&program).is_err());
    }

    #[test]
    fn test_same_function_name_across_sources_is_distinct() {
        let program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "a.kora"; import "b.kora"; int main() { return 0; }"#,
                ),
                ("a.kora", "int helper() { return 1; }"),
                ("b.kora", "int helper() { return 2; }"),
            ],
        );
        let symbols = resolve_program(&program).expect("resolve");
        // Both keep the bare name, but are distinct symbols.
        let a = source_module(&program, "a.kora").module.functions[0].id;
        let b = source_module(&program, "b.kora").module.functions[0].id;
        let a_id = symbols.symbol_id_of_declaration(a).unwrap();
        let b_id = symbols.symbol_id_of_declaration(b).unwrap();
        assert_eq!(symbols.symbol(a_id).name, "helper");
        assert_eq!(symbols.symbol(b_id).name, "helper");
        assert_ne!(a_id, b_id);
    }

    #[test]
    fn test_resolves_all_identifiers() {
        let source = r#"
            extern void print(b: [char], a: int);

            int main() {
                let a: int = 5;
                if (a - a) {
                    print("Hello World", a);
                }
                return a;
            }

            int sum(a: int, b: int) {
                return a + b;
            }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_reports_undefined_identifiers() {
        let source = r#"
            int main() {
                let a: int = unident_1;
                unident_2;
                if (a) { unident_3; }
                return a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier errors");
        assert_eq!(errors.len(), 3, "errors: {:?}", errors);
    }

    #[test]
    fn test_reports_undefined_types() {
        let source = r#"
            struct Point { x: int, y: int }

            Bogus1 make(p: Bogus2) {
                let a: Point = new Point;
                let b: Bogus3 = new Bogus4;
                let c: [Bogus5] = new Point;
                return a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-type errors");
        assert_eq!(errors.len(), 5, "errors: {:?}", errors);
    }

    #[test]
    fn test_reports_same_scope_redeclarations() {
        let cases = [
            r#"int main() { let x: int = 1; let x: int = 2; return x; }"#,
            r#"int f(a: int, a: int) { return a; } int main() { return 0; }"#,
            r#"int f() { return 0; } int f() { return 1; } int main() { return 0; }"#,
            r#"extern int g(a: int); int g(a: int) { return a; } int main() { return 0; }"#,
            r#"struct P { x: int } struct P { y: int } int main() { return 0; }"#,
            r#"struct P { x: int, x: int } int main() { return 0; }"#,
        ];
        for source in cases {
            let errors = resolve(source).expect_err(source);
            assert_eq!(errors.len(), 1, "source: {}, errors: {:?}", source, errors);
        }
    }

    #[test]
    fn test_intrinsic_names_cannot_be_declared() {
        let cases = [
            r#"int copy(a: int) { return a; } int main() { return 0; }"#,
            r#"extern int copy(a: int); int main() { return 0; }"#,
            r#"int main() { let copy: int = 1; return copy; }"#,
            r#"int main(copy: int) { return copy; }"#,
        ];
        for source in cases {
            let errors = resolve(source).expect_err(source);
            assert!(
                errors.iter().any(|e| e.msg.contains("intrinsic")),
                "source: {}, errors: {:?}",
                source,
                errors
            );
        }
    }

    #[test]
    fn test_free_len_call_is_undefined() {
        // len is a method now: a.len(). The free form no longer resolves.
        let source = r#"int main() { let a: [int] = new int[3]; return len(a); }"#;
        assert!(resolve(source).is_err());
    }

    #[test]
    fn test_cross_scope_shadowing_is_allowed() {
        let source = r#"
            int main() {
                let x: int = 1;
                if (x == 1) {
                    let x: int = 2;
                    return x;
                }
                return x;
            }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_for_scopes_induction_variable_to_the_loop() {
        let source = r#"
            int main() {
                for (let i: int = 0; i < 3; i = i + 1) { i; }
                return i;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    #[test]
    fn test_break_and_continue_require_a_loop() {
        let ok = r#"
            int main() {
                while (true) { break; }
                for (let i: int = 0; i < 3; i = i + 1) { continue; }
                return 0;
            }
        "#;
        assert!(resolve(ok).is_ok());

        let bad = r#"int main() { break; continue; return 0; }"#;
        let errors = resolve(bad).expect_err("expected outside-loop errors");
        assert_eq!(errors.len(), 2, "errors: {:?}", errors);
    }

    #[test]
    fn test_self_and_forward_referential_structs_resolve() {
        let source = r#"
            struct Node { next: Node, value: int }
            struct A { b: B }
            struct B { n: int }

            int main() { return 0; }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_use_is_keyed_by_node_id() {
        use crate::parser::{Expression, Statement, Type};

        let source = "int f(a: int) { return a; }";
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

        // Reach the `a` in `return a;` and confirm its NodeId resolves to `int`.
        let body = &module.functions[0].node.statement;
        let Statement::Compound(stmts) = &body.node else {
            panic!("expected compound body");
        };
        let Statement::Return(Some(expr)) = &stmts[0].node else {
            panic!("expected return statement");
        };
        assert!(matches!(expr.node, Expression::Identifier(_)));
        assert_eq!(symbols.type_of_use(expr.id), Some(Type::Int));
    }

    #[test]
    fn test_declaration_is_keyed_by_node_id() {
        use crate::parser::{Statement, Type};

        let source = r#"
            int main(a: int) {
                let x: int = 1;
                if (true) {
                    let x: real = 2.0;
                    x;
                }
                x;
                return a;
            }
        "#;
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

        let func = &module.functions[0];
        let Statement::Compound(stmts) = &func.node.statement.node else {
            panic!("expected compound body");
        };
        let Statement::Let(outer_pair, _, _) = &stmts[0].node else {
            panic!("expected let statement");
        };
        let Statement::If(_, if_body, _) = &stmts[1].node else {
            panic!("expected if statement");
        };
        let Statement::Compound(if_stmts) = &if_body.node else {
            panic!("expected compound if body");
        };
        let Statement::Let(inner_pair, _, _) = &if_stmts[0].node else {
            panic!("expected inner let statement");
        };
        let Statement::Simple(inner_use) = &if_stmts[1].node else {
            panic!("expected inner use");
        };
        let Statement::Simple(outer_use) = &stmts[2].node else {
            panic!("expected outer use");
        };

        let outer = symbols.symbol_id_of_declaration(outer_pair.id).unwrap();
        let inner = symbols.symbol_id_of_declaration(inner_pair.id).unwrap();
        assert_ne!(outer, inner);
        assert_eq!(symbols.symbol_id_of_use(outer_use.id), Some(outer));
        assert_eq!(symbols.symbol_id_of_use(inner_use.id), Some(inner));
        assert_eq!(symbols.symbol(inner).ty, Some(Type::Real));

        let param = symbols
            .symbol_id_of_declaration(func.node.arguments[0].id)
            .unwrap();
        assert_eq!(symbols.symbol(param).ty, Some(Type::Int));
        assert!(symbols.symbol_id_of_declaration(func.id).is_some());
    }

    #[test]
    fn test_functions_can_be_forward_referenced() {
        let source = r#"
            int main() { return helper(); }
            int helper() { return 1; }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_recursive_function_resolves() {
        let source = r#"
            int fact(n: int) { return fact(n); }
            int main() { return 0; }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_let_initializer_uses_outer_scope() {
        // The inner let x = x; binds its initializer to the outer x.
        let ok = r#"
            int main() {
                let x: int = 1;
                if (x) { let x: int = x; return x; }
                return x;
            }
        "#;
        assert!(resolve(ok).is_ok());

        // With no outer binding, the self-referential initializer is undefined.
        let bad = r#"int main() { let x: int = x; return 0; }"#;
        let errors = resolve(bad).expect_err("expected undefined-identifier error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    #[test]
    fn test_block_scope_ends_at_brace() {
        let source = r#"
            int main() {
                if (true) { let a: int = 1; }
                return a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    #[test]
    fn test_sibling_scopes_at_same_depth_do_not_leak() {
        // A binding in one block is invisible to a later sibling block.
        let source = r#"
            int main() {
                { let a: int = 1; }
                { let b: int = a; }
                return 0;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    #[test]
    fn test_braceless_control_flow_body_declaration_is_scoped() {
        // A `let` in a braceless while/if body must not leak past the construct.
        let cases = [
            r#"int main() { while (true) let x: int = 1; return x; }"#,
            r#"int main() { if (true) let x: int = 1; return x; }"#,
        ];
        for source in cases {
            let errors = resolve(source).expect_err(source);
            assert_eq!(errors.len(), 1, "source: {}, errors: {:?}", source, errors);
        }
    }

    #[test]
    fn test_undefined_identifiers_in_expression_positions() {
        // Undefined names are flagged in binary, index, and unary positions.
        let source = r#"
            int main() {
                let a: int = 1;
                a + missing1;
                missing2[a];
                -missing3;
                return a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier errors");
        assert_eq!(errors.len(), 3, "errors: {:?}", errors);
    }

    #[test]
    fn test_call_resolves_callee_and_arguments() {
        // A non-intrinsic call flags an undefined callee and each undefined argument.
        let source = r#"
            int main() {
                let a: int = 1;
                missing_fn(a, missing_arg);
                return a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier errors");
        assert_eq!(errors.len(), 2, "errors: {:?}", errors);
    }

    #[test]
    fn test_undefined_type_inside_array_member() {
        let source = r#"
            struct S { a: [Undefined], b: int }
            int main() { return 0; }
        "#;
        let errors = resolve(source).expect_err("expected undefined-type error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    #[test]
    fn test_nested_loops_allow_break_and_continue() {
        let source = r#"
            int main() {
                while (true) {
                    for (let i: int = 0; i < 2; i = i + 1) {
                        break;
                        continue;
                    }
                }
                return 0;
            }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_loop_depth_resets_after_loop() {
        let source = r#"
            int main() {
                while (true) {}
                break;
                return 0;
            }
        "#;
        let errors = resolve(source).expect_err("expected outside-loop error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    #[test]
    fn test_struct_members_are_keyed_by_name() {
        use crate::parser::Type;

        let source = r#"
            struct Point { x: int, y: [char] }
            int main() { return 0; }
        "#;
        let symbols = resolve(source).expect("resolve");
        assert!(symbols.struct_exists("Point"));
        assert_eq!(symbols.struct_member("Point", "x"), Some(Type::Int));
        assert_eq!(
            symbols.struct_member("Point", "y"),
            Some(Type::Array(Box::new(Type::Char)))
        );
        assert_eq!(symbols.struct_member("Point", "z"), None);
        assert_eq!(symbols.struct_member("Missing", "x"), None);
    }

    #[test]
    fn test_impl_blocks_declare_methods() {
        let symbols = resolve(
            r#"
            struct P { x: int }
            impl P { int get(self) { return self.x; } }
            impl P { void set(self, v: int) { self.x = v; } }
            "#,
        )
        .expect("resolve");
        let get = symbols.struct_method("P", "get").expect("get");
        assert_eq!(symbols.symbol(get).name, "get");
        assert!(symbols.struct_method("P", "set").is_some());
        assert!(symbols.struct_method("P", "missing").is_none());
        assert!(symbols.struct_method("Q", "get").is_none());
    }

    #[test]
    fn test_impl_errors() {
        let cases = [
            "impl Missing { int f(self) { return 1; } }",
            "struct P { age: int } impl P { int age(self) { return self.age; } }",
            "struct P { x: int } impl P { int f(self) { return 1; } int f(self) { return 2; } }",
        ];
        for source in cases {
            assert!(resolve(source).is_err(), "source: {}", source);
        }
    }
}
