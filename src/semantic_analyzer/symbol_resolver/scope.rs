use std::collections::HashMap;

use super::table::SymbolId;

pub(super) type Scope = HashMap<String, SymbolId>;

/// local name -> index of the imported source.
pub(super) type ImportMap = HashMap<String, usize>;

#[derive(Default)]
pub(super) struct Scopes {
    globals: Vec<Scope>,
    imports: Vec<ImportMap>,
    locals: Vec<Scope>,
    current: usize,
}

impl Scopes {
    pub(super) fn new(globals: Vec<Scope>, imports: Vec<ImportMap>) -> Self {
        Scopes {
            globals,
            imports,
            locals: Vec::new(),
            current: 0,
        }
    }

    pub(super) fn enter_source(&mut self, index: usize) {
        self.current = index;
        self.locals.clear();
    }

    pub(super) fn push(&mut self) {
        self.locals.push(Scope::new());
    }

    pub(super) fn pop(&mut self) {
        self.locals.pop();
    }

    /// Returns `true` on a same-scope redeclaration.
    pub(super) fn declare(&mut self, name: String, id: SymbolId) -> bool {
        match self.locals.last_mut() {
            Some(scope) => scope.insert(name, id).is_some(),
            None => false,
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.locals
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .or_else(|| self.globals[self.current].get(name).copied())
    }

    pub(super) fn is_module(&self, name: &str) -> bool {
        self.imports[self.current].contains_key(name)
    }

    pub(super) fn lookup_qualified(&self, module: &str, member: &str) -> Option<SymbolId> {
        let &target = self.imports[self.current].get(module)?;
        self.globals[target].get(member).copied()
    }
}
