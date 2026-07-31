use super::super::errors::TypeErr;
use super::check_typename;
use super::scope::Scope;
use super::table::{StructDef, SymbolId, SymbolTable, is_intrinsic};
use crate::parser::*;

pub(super) struct GlobalsCollector<'a> {
    table: &'a mut SymbolTable,
    errors: &'a mut Vec<TypeErr>,
}

impl<'a> GlobalsCollector<'a> {
    pub(super) fn new(table: &'a mut SymbolTable, errors: &'a mut Vec<TypeErr>) -> Self {
        GlobalsCollector { table, errors }
    }

    pub(super) fn collect(&mut self, modules: &[&Module]) -> Vec<Scope> {
        for &module in modules {
            self.collect_struct_names(module);
        }
        let mut scopes = Vec::new();
        for &module in modules {
            self.collect_struct_members(module);
            scopes.push(self.collect_function_signatures(module));
        }
        for &module in modules {
            self.collect_methods(module);
        }
        scopes
    }

    fn collect_struct_names(&mut self, module: &Module) {
        for struct_ in module.structs.iter() {
            if self.table.struct_exists(&struct_.node.name) {
                self.errors.push(TypeErr {
                    msg: "Redeclaration of struct",
                    span: struct_.span.clone(),
                });
            } else {
                self.table
                    .structs
                    .insert(struct_.node.name.clone(), StructDef::default());
            }
        }
    }

    fn collect_struct_members(&mut self, module: &Module) {
        for struct_ in module.structs.iter() {
            for member in struct_.node.members.iter() {
                check_typename(self.table, self.errors, &member.node.typename);
                let already = self.table.structs[&struct_.node.name]
                    .members
                    .iter()
                    .any(|(field, _)| field == &member.node.name);
                if already {
                    self.errors.push(TypeErr {
                        msg: "Redeclaration of struct member",
                        span: member.span.clone(),
                    });
                } else {
                    self.table
                        .structs
                        .get_mut(&struct_.node.name)
                        .unwrap()
                        .members
                        .push((member.node.name.clone(), member.node.typename.clone()));
                }
            }
        }
    }

    fn collect_function_signatures(&mut self, module: &Module) -> Scope {
        let mut scope = Scope::new();
        for func in module.extern_functions.iter() {
            let id =
                self.table
                    .add_symbol(func.id, func.node.name.clone(), Some(func.node.get_type()));
            self.bind(&mut scope, func.node.name.clone(), id, &func.span);
        }
        for func in module.functions.iter() {
            let id =
                self.table
                    .add_symbol(func.id, func.node.name.clone(), Some(func.node.get_type()));
            self.bind(&mut scope, func.node.name.clone(), id, &func.span);
        }
        scope
    }

    fn collect_methods(&mut self, module: &Module) {
        for impl_ in module.impls.iter() {
            let struct_name = &impl_.node.struct_name;
            if !self.table.struct_exists(&struct_name.node) {
                self.errors.push(TypeErr {
                    msg: "impl block for an undefined struct",
                    span: struct_name.span.clone(),
                });
                continue;
            }
            for func in impl_.node.functions.iter() {
                let struct_def = &self.table.structs[&struct_name.node];
                if struct_def
                    .members
                    .iter()
                    .any(|(field, _)| field == &func.node.name)
                {
                    self.errors.push(TypeErr {
                        msg: "A method cannot have the same name as a struct member",
                        span: func.span.clone(),
                    });
                }
                if struct_def.methods.contains_key(&func.node.name) {
                    self.errors.push(TypeErr {
                        msg: "Redeclaration of method",
                        span: func.span.clone(),
                    });
                    continue;
                }
                let id = self.table.add_symbol(
                    func.id,
                    func.node.name.clone(),
                    Some(func.node.get_type()),
                );
                self.table
                    .structs
                    .get_mut(&struct_name.node)
                    .unwrap()
                    .methods
                    .insert(func.node.name.clone(), id);
            }
        }
    }

    fn bind(&mut self, scope: &mut Scope, name: String, id: SymbolId, span: &Span) {
        if is_intrinsic(&name) {
            self.errors.push(TypeErr {
                msg: "Cannot declare a name reserved for a compiler intrinsic",
                span: span.clone(),
            });
        }
        if scope.contains_key(&name) {
            self.errors.push(TypeErr {
                msg: "Redeclaration in the same scope",
                span: span.clone(),
            });
        }
        scope.insert(name, id);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use crate::parser::Type;

    #[test]
    fn test_collects_struct_members() {
        let symbols =
            resolve(r#"struct Point { x: int, y: [char] } int main() { return 0; }"#).expect("ok");
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
    fn test_collects_methods() {
        let symbols = resolve(
            r#"
            struct P { x: int }
            impl P { int get(self) { return self.x; } }
            impl P { void set(self, v: int) { self.x = v; } }
            "#,
        )
        .expect("ok");
        let get = symbols.struct_method("P", "get").expect("get");
        assert_eq!(symbols.symbol(get).name, "get");
        assert!(symbols.struct_method("P", "set").is_some());
        assert!(symbols.struct_method("P", "missing").is_none());
        assert!(symbols.struct_method("Q", "get").is_none());
    }

    #[test]
    fn test_top_level_symbols_are_bare_and_distinct() {
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
        let symbols = resolve_program(&program).expect("ok");
        let a = source_module(&program, "a.kora").module.functions[0].id;
        let b = source_module(&program, "b.kora").module.functions[0].id;
        let a_id = symbols.symbol_id_of_declaration(a).unwrap();
        let b_id = symbols.symbol_id_of_declaration(b).unwrap();
        assert_eq!(symbols.symbol(a_id).name, "helper");
        assert_eq!(symbols.symbol(b_id).name, "helper");
        assert_ne!(a_id, b_id);
    }

    #[test]
    fn test_rejects_duplicate_globals() {
        let cases = [
            r#"int f() { return 0; } int f() { return 1; } int main() { return 0; }"#,
            r#"extern int64 g(a: int64); int g(a: int) { return a; } int main() { return 0; }"#,
            r#"struct P { x: int } struct P { y: int } int main() { return 0; }"#,
            r#"struct P { x: int, x: int } int main() { return 0; }"#,
        ];
        for source in cases {
            assert_eq!(resolve(source).expect_err(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn test_rejects_intrinsic_global() {
        for source in [
            r#"int copy(a: int) { return a; } int main() { return 0; }"#,
            r#"extern int64 copy(a: int64); int main() { return 0; }"#,
        ] {
            let errors = resolve(source).expect_err(source);
            assert!(
                errors.iter().any(|e| e.msg.contains("intrinsic")),
                "{source}"
            );
        }
    }

    #[test]
    fn test_rejects_undefined_type_in_member() {
        let source = r#"struct S { a: [Undefined], b: int } int main() { return 0; }"#;
        assert_eq!(resolve(source).expect_err("undefined type").len(), 1);
    }

    #[test]
    fn test_impl_errors() {
        for source in [
            "impl Missing { int f(self) { return 1; } }",
            "struct P { age: int } impl P { int age(self) { return self.age; } }",
            "struct P { x: int } impl P { int f(self) { return 1; } int f(self) { return 2; } }",
        ] {
            assert!(resolve(source).is_err(), "{source}");
        }
    }

    #[test]
    fn test_forward_and_self_referential_structs_resolve() {
        let source = r#"
            struct Node { next: Node, value: int }
            struct A { b: B }
            struct B { n: int }
            int main() { return 0; }
        "#;
        assert!(resolve(source).is_ok());
    }
}
