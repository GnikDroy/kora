use std::collections::HashMap;

use crate::parser::*;

/// `copy` is the only intrinsic
pub fn is_intrinsic(name: &str) -> bool {
    name == "copy"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(usize);

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Option<Type>,
}

#[derive(Debug, Default)]
pub struct StructDef {
    // Members are ordered for C compatibility
    pub members: Vec<(String, Type)>,
    pub methods: HashMap<String, SymbolId>,
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    pub symbols: Vec<Symbol>,
    pub uses: HashMap<NodeId, SymbolId>,
    pub declarations: HashMap<NodeId, SymbolId>,
    pub structs: HashMap<String, StructDef>,
}

impl SymbolTable {
    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0]
    }

    pub fn symbol_id_of_use(&self, use_id: NodeId) -> Option<SymbolId> {
        self.uses.get(&use_id).copied()
    }

    pub fn symbol_id_of_declaration(&self, declaration_id: NodeId) -> Option<SymbolId> {
        self.declarations.get(&declaration_id).copied()
    }

    /// `None` also covers types that are later inferred.
    pub fn type_of_use(&self, use_id: NodeId) -> Option<Type> {
        self.symbol(self.symbol_id_of_use(use_id)?).ty.clone()
    }

    pub fn struct_exists(&self, name: &str) -> bool {
        self.structs.contains_key(name)
    }

    pub fn struct_member(&self, name: &str, member: &str) -> Option<Type> {
        self.structs
            .get(name)?
            .members
            .iter()
            .find(|(field, _)| field == member)
            .map(|(_, ty)| ty.clone())
    }

    pub fn struct_member_count(&self, name: &str) -> Option<usize> {
        self.structs.get(name).map(|s| s.members.len())
    }

    pub fn struct_members(&self, name: &str) -> Option<&[(String, Type)]> {
        self.structs.get(name).map(|s| s.members.as_slice())
    }

    pub fn struct_method(&self, name: &str, method: &str) -> Option<SymbolId> {
        self.structs.get(name)?.methods.get(method).copied()
    }

    pub(super) fn add_symbol(
        &mut self,
        declaration_id: NodeId,
        name: String,
        ty: Option<Type>,
    ) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol { name, ty });
        self.declarations.insert(declaration_id, id);
        id
    }
}
