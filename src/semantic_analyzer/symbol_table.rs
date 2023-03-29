use crate::parser::*;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Scope {
    symbols: HashMap<String, Type>,
}

#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
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
            .filter_map(|scope| scope.symbols.get(name))
            .next()
            .cloned()
    }
}

impl ASTVisitor for SymbolTable {
    fn visit_let_statement(&mut self, pair: &IdentifierTypePair, expr: &Expression) {
        walk_let_statement(self, pair, expr);
        self.add_symbol(pair.name.clone(), pair.typename.clone());
    }

    fn visit_compound_statement(&mut self, stmts: &Vec<Statement>) {
        self.push_scope();
        walk_compound_statement(self, stmts);
    }

    fn visit_function(&mut self, func: &Function) {
        self.push_scope();
        for pair in func.arguments.iter() {
            self.add_symbol(pair.name.clone(), pair.typename.clone());
        }
        walk_function(self, func);
    }

    fn visit_module(&mut self, module: &Module) {
        self.push_scope();
        for func in module.functions.iter() {
            self.add_symbol(func.name.clone(), func.get_type());
        }
        walk_module(self, module);
    }
}
