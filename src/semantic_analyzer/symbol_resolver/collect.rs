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
            if let Some(return_type) = &func.node.return_type {
                check_typename(self.table, self.errors, return_type);
            }
            for arg in func.node.arguments.iter() {
                check_typename(self.table, self.errors, &arg.node.typename);
            }
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
