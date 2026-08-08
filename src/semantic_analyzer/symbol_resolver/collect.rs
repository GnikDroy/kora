use std::collections::{HashMap, HashSet};

use super::super::errors::TypeErr;
use super::check_typename;
use super::scope::Scope;
use super::table::{StructDef, SymbolId, SymbolTable, is_intrinsic};
use crate::instantiate::Instantiated;
use crate::parser::*;

pub(super) struct GlobalsCollector<'a> {
    table: &'a mut SymbolTable,
    errors: &'a mut Vec<TypeErr>,
    instances: &'a Instantiated,
    struct_names: HashSet<String>,
}

impl<'a> GlobalsCollector<'a> {
    pub(super) fn new(
        table: &'a mut SymbolTable,
        errors: &'a mut Vec<TypeErr>,
        instances: &'a Instantiated,
    ) -> Self {
        GlobalsCollector {
            table,
            errors,
            instances,
            struct_names: HashSet::new(),
        }
    }

    pub(super) fn collect(&mut self, modules: &[&Module]) -> Vec<Scope> {
        for (m, &module) in modules.iter().enumerate() {
            self.collect_struct_names(m, module);
        }
        let mut scopes = Vec::new();
        for &module in modules {
            self.collect_struct_members(module);
            scopes.push(self.collect_function_signatures(module));
        }
        for &module in modules {
            self.collect_methods(modules, module);
        }
        scopes
    }

    fn collect_struct_names(&mut self, m: usize, module: &Module) {
        for (index, struct_) in module.structs.iter().enumerate() {
            if !self.instances.struct_instances.contains(&struct_.id)
                && !self.struct_names.insert(struct_.node.name.clone())
            {
                self.errors.push(TypeErr {
                    msg: "Redeclaration of struct",
                    span: struct_.span.clone(),
                });
                continue;
            }
            self.table.structs.insert(
                struct_.id,
                StructDef {
                    module: m,
                    index,
                    methods: HashMap::new(),
                },
            );
        }
    }

    fn collect_struct_members(&mut self, module: &Module) {
        for struct_ in module.structs.iter() {
            if !self.table.structs.contains_key(&struct_.id) {
                continue;
            }
            for (k, member) in struct_.node.members.iter().enumerate() {
                check_typename(self.table, self.errors, &member.node.typename);
                let already = struct_.node.members[..k]
                    .iter()
                    .any(|prev| prev.node.name == member.node.name);
                if already {
                    self.errors.push(TypeErr {
                        msg: "Redeclaration of struct member",
                        span: member.span.clone(),
                    });
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
            if !self.instances.fn_instances.contains(&func.id) {
                self.bind(&mut scope, func.node.name.clone(), id, &func.span);
            }
        }
        for global in module.globals.iter() {
            let name = global.node.name.node.clone();
            let id = self
                .table
                .add_symbol(global.id, name.clone(), global.node.typename.clone());
            self.bind(&mut scope, name, id, &global.span);
        }
        scope
    }

    fn collect_methods(&mut self, modules: &[&Module], module: &Module) {
        for impl_ in module.impls.iter() {
            let Some(decl) = self.table.struct_decl_of(&impl_.node.struct_ref) else {
                self.errors.push(TypeErr {
                    msg: "impl block for an undefined struct",
                    span: impl_.node.struct_ref.name.span.clone(),
                });
                continue;
            };
            for func in impl_.node.functions.iter() {
                if self
                    .table
                    .struct_members(modules, decl)
                    .iter()
                    .any(|m| m.node.name == func.node.name)
                {
                    self.errors.push(TypeErr {
                        msg: "A method cannot have the same name as a struct member",
                        span: func.span.clone(),
                    });
                }
                if self.table.structs[&decl]
                    .methods
                    .contains_key(&func.node.name)
                {
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
                    .get_mut(&decl)
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
    use crate::parser::{Module, Type};

    #[test]
    fn test_collects_struct_members() {
        let mut program = load_program(
            "main.kora",
            vec![(
                "main.kora",
                r#"struct Point { x: int, y: [char] } int main() { return 0; }"#,
            )],
        );
        let symbols = resolve_program(&mut program).expect("ok");
        let modules: Vec<&Module> = program.modules.iter().map(|m| &m.module).collect();
        assert_eq!(symbols.structs.len(), 1);
        let (&point, _) = symbols.structs.iter().next().unwrap();
        assert_eq!(symbols.struct_member(&modules, point, "x"), Some(Type::Int));
        assert_eq!(
            symbols.struct_member(&modules, point, "y"),
            Some(Type::Array(Box::new(Type::Char)))
        );
        assert_eq!(symbols.struct_member(&modules, point, "z"), None);
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
        assert_eq!(symbols.structs.len(), 1);
        let (&p, _) = symbols.structs.iter().next().unwrap();
        let get = symbols.struct_method(p, "get").expect("get");
        assert_eq!(symbols.symbol(get).name, "get");
        assert!(symbols.struct_method(p, "set").is_some());
        assert!(symbols.struct_method(p, "missing").is_none());
    }

    #[test]
    fn test_top_level_symbols_are_bare_and_distinct() {
        let mut program = load_program(
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
        let symbols = resolve_program(&mut program).expect("ok");
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
