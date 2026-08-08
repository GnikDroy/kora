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

#[derive(Debug)]
pub struct StructDef {
    pub module: usize,
    pub index: usize, // index into the AST's module's structs list
    pub methods: HashMap<String, SymbolId>,
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    pub symbols: Vec<Symbol>,
    pub uses: HashMap<NodeId, SymbolId>,
    pub declarations: HashMap<NodeId, SymbolId>,
    pub structs: HashMap<NodeId, StructDef>,
}

impl SymbolTable {
    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0]
    }

    pub fn set_symbol_type(&mut self, id: SymbolId, ty: Type) {
        self.symbols[id.0].ty = Some(ty);
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

    pub fn struct_decl_of(&self, sr: &StructRef) -> Option<NodeId> {
        sr.target.filter(|id| self.structs.contains_key(id))
    }

    pub fn struct_members<'m>(
        &self,
        modules: &[&'m Module],
        decl: NodeId,
    ) -> &'m [Spanned<IdentifierTypePair>] {
        let def = &self.structs[&decl];
        &modules[def.module].structs[def.index].node.members
    }

    pub fn struct_member(&self, modules: &[&Module], decl: NodeId, member: &str) -> Option<Type> {
        self.struct_members(modules, decl)
            .iter()
            .find(|m| m.node.name == member)
            .map(|m| m.node.typename.clone())
    }

    pub fn struct_method(&self, decl: NodeId, method: &str) -> Option<SymbolId> {
        self.structs.get(&decl)?.methods.get(method).copied()
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
