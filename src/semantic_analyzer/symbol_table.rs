use crate::parser::*;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Scope {
    symbols: HashMap<String, Type>,
}

#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    struct_members: HashMap<(String, String), Type>,
}

impl SymbolTable {
    pub fn new() -> SymbolTable {
        SymbolTable {
            ..Default::default()
        }
    }

    pub fn reverse(&mut self) {
        self.scopes.reverse();
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Scope {
            ..Default::default()
        });
    }

    pub fn add_scope(&mut self, scope: Scope) {
        self.scopes.push(scope);
    }

    pub fn pop_scope(&mut self) -> Option<Scope> {
        self.scopes.pop()
    }

    pub fn add_symbol(&mut self, name: String, typename: Type) -> bool {
        self.scopes
            .last_mut()
            .map(|scope| scope.symbols.insert(name, typename))
            .is_some()
    }

    pub fn resolve(&self, name: &String) -> Option<Type> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.symbols.get(name))
            .cloned()
    }

    pub fn resolve_struct_member(&self, name: &String, member: &String) -> Option<Type> {
        self.struct_members
            .get(&(name.to_owned(), member.to_owned()))
            .cloned()
    }
}

impl ASTVisitor for SymbolTable {
    fn visit_let_statement(&mut self, pair: &Spanned<IdentifierTypePair>, expr: &Spanned<Expression>) {
        walk_let_statement(self, pair, expr);
        self.add_symbol(pair.node.name.clone(), pair.node.typename.clone());
    }

    fn visit_compound_statement(&mut self, stmts: &[Spanned<Statement>]) {
        self.push_scope();
        walk_compound_statement(self, stmts);
    }

    fn visit_extern_function(&mut self, func: &Spanned<ExternFunction>) {
        self.push_scope();
        for pair in func.node.arguments.iter() {
            self.add_symbol(pair.node.name.clone(), pair.node.typename.clone());
        }
        walk_extern_function(self, func);
    }

    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.push_scope();
        for pair in func.node.arguments.iter() {
            self.add_symbol(pair.node.name.clone(), pair.node.typename.clone());
        }
        walk_function(self, func);
    }

    fn visit_module(&mut self, module: &Module) {
        self.push_scope();
        for struct_ in module.structs.iter() {
            for member in struct_.node.members.iter() {
                self.struct_members.insert(
                    (struct_.node.name.clone(), member.node.name.clone()),
                    member.node.typename.clone(),
                );
            }
        }
        for func in module.extern_functions.iter() {
            self.add_symbol(func.node.name.clone(), func.node.get_type());
        }
        for func in module.functions.iter() {
            self.add_symbol(func.node.name.clone(), func.node.get_type());
        }
        walk_module(self, module);
    }
}
