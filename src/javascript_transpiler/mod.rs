mod coloring;
mod emit;
mod error;
mod mangle;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use self::error::TranspilerErr;
use self::mangle::mangle;
use crate::parser::*;
use crate::semantic_analyzer::{ArrayMethod, SymbolId, SymbolTable};

pub(crate) use coloring::resolve_async_fns;

pub(crate) fn struct_member_map(symbols: &SymbolTable) -> HashMap<String, Vec<(String, Type)>> {
    symbols
        .structs
        .iter()
        .map(|(name, def)| (name.clone(), def.members.clone()))
        .collect()
}

pub(crate) fn mangled_method_calls(
    symbols: &SymbolTable,
    method_calls: &HashMap<NodeId, SymbolId>,
) -> HashMap<NodeId, String> {
    method_calls
        .iter()
        .map(|(id, sym)| {
            let method = &symbols.symbol(*sym).name;
            let name = symbols
                .structs
                .iter()
                .find(|(_, def)| def.methods.values().any(|m| m == sym))
                .map(|(struct_name, _)| mangle(struct_name, method))
                .unwrap_or_else(|| method.clone());
            (*id, name)
        })
        .collect()
}

#[derive(Default, Debug)]
pub struct JavascriptTranspiler {
    source: String,
    errors: Vec<TranspilerErr>,
    types: HashMap<NodeId, Type>,
    method_calls: HashMap<NodeId, String>,
    array_method_calls: HashMap<NodeId, ArrayMethod>,
    struct_members: HashMap<String, Vec<(String, Type)>>,
    current_impl: Option<String>,
    /// What color is your function?
    /// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
    /// Necessary because javascript is a colored language.
    async_fns: HashSet<String>,
}

impl JavascriptTranspiler {
    pub fn new(
        types: HashMap<NodeId, Type>,
        method_calls: HashMap<NodeId, String>,
        array_method_calls: HashMap<NodeId, ArrayMethod>,
        struct_members: HashMap<String, Vec<(String, Type)>>,
        async_fns: HashSet<String>,
    ) -> JavascriptTranspiler {
        JavascriptTranspiler {
            source: String::new(),
            errors: Vec::new(),
            types,
            method_calls,
            array_method_calls,
            struct_members,
            current_impl: None,
            async_fns,
        }
    }

    pub fn get_source(&self) -> Result<&str, &[TranspilerErr]> {
        if self.errors.is_empty() {
            Ok(&self.source)
        } else {
            Err(&self.errors)
        }
    }
}
