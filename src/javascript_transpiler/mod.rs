mod coloring;
mod emit;
mod error;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use self::error::TranspilerErr;
use crate::loader::LoadedProgram;
use crate::mangle::{mangle, mangle_method, mangle_prefix};
use crate::parser::*;
use crate::semantic_analyzer::{ArrayMethod, SymbolId, SymbolTable};

pub(crate) use coloring::resolve_async_fns;

pub fn transpile(
    compiled: crate::CompiledProgram,
    async_externs: HashSet<String>,
) -> Result<String, String> {
    let emitted = crate::mangle::emitted_names(&compiled.program, &compiled.origins);
    let method_calls = mangled_method_calls(&compiled.symbols, &compiled.method_calls, &emitted);
    let function_names = function_names(&compiled.symbols, &compiled.program, &emitted);
    let struct_members = struct_member_map(&compiled.symbols);
    let struct_ids = compiled.symbols.struct_names.clone();

    let modules: Vec<&Module> = compiled.program.modules.iter().map(|m| &m.module).collect();
    let async_fns = resolve_async_fns(
        &modules,
        &function_names,
        &method_calls,
        async_externs,
        &emitted,
    );
    let mut transpiler = JavascriptTranspiler {
        types: compiled.types,
        method_calls,
        array_method_calls: compiled.array_method_calls,
        struct_members,
        struct_ids,
        function_names,
        async_fns,
        emitted,
        ..JavascriptTranspiler::default()
    };
    transpiler.emit_program(&modules);

    transpiler.get_source().map(|s| s.to_string()).map_err(|e| {
        e.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(crate) fn struct_member_map(symbols: &SymbolTable) -> HashMap<NodeId, Vec<(String, Type)>> {
    symbols
        .structs
        .iter()
        .map(|(decl, def)| (*decl, def.members.clone()))
        .collect()
}

pub(crate) fn function_names(
    symbols: &SymbolTable,
    program: &LoadedProgram,
    emitted: &HashMap<NodeId, String>,
) -> HashMap<NodeId, String> {
    let entry = program.modules.first().map(|m| m.id);
    let root = program
        .sources
        .first()
        .and_then(|s| s.path.parent())
        .unwrap_or_else(|| std::path::Path::new(""));
    let mut by_symbol: HashMap<SymbolId, String> = HashMap::new();
    let mut by_node: HashMap<NodeId, String> = HashMap::new();

    for module in &program.modules {
        let prefix = if Some(module.id) == entry {
            String::new()
        } else {
            mangle_prefix(&program.sources[module.id.0 as usize].path, root)
        };
        for func in &module.module.extern_functions {
            if let Some(id) = symbols.symbol_id_of_declaration(func.id) {
                by_symbol.insert(id, func.node.name.clone());
            }
        }
        for func in &module.module.functions {
            let name = if Some(module.id) == entry && func.node.name == "main" {
                "__kora_main".to_string()
            } else {
                let base = emitted.get(&func.id).unwrap_or(&func.node.name);
                mangle(&prefix, base)
            };
            by_node.insert(func.id, name.clone());
            if let Some(id) = symbols.symbol_id_of_declaration(func.id) {
                by_symbol.insert(id, name);
            }
        }
    }

    for (use_id, symbol) in &symbols.uses {
        if let Some(name) = by_symbol.get(symbol) {
            by_node.insert(*use_id, name.clone());
        }
    }

    by_node
}

pub(crate) fn mangled_method_calls(
    symbols: &SymbolTable,
    method_calls: &HashMap<NodeId, SymbolId>,
    emitted: &HashMap<NodeId, String>,
) -> HashMap<NodeId, String> {
    method_calls
        .iter()
        .map(|(id, sym)| {
            let method = &symbols.symbol(*sym).name;
            let name = symbols
                .structs
                .iter()
                .find(|(_, def)| def.methods.values().any(|m| m == sym))
                .map(|(decl, def)| {
                    mangle_method(emitted.get(decl).unwrap_or(&def.name), method)
                })
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
    struct_members: HashMap<NodeId, Vec<(String, Type)>>,
    struct_ids: HashMap<String, NodeId>,
    /// Mangled name per function definition and call site
    function_names: HashMap<NodeId, String>,
    emitted: HashMap<NodeId, String>,
    current_impl: Option<String>,
    /// What color is your function?
    /// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
    /// Necessary because javascript is a colored language.
    async_fns: HashSet<String>,
}

impl JavascriptTranspiler {
    fn struct_decl(&self, sr: &StructRef) -> Option<NodeId> {
        sr.target
            .or_else(|| self.struct_ids.get(&sr.name.node).copied())
    }

    pub fn get_source(&self) -> Result<&str, &[TranspilerErr]> {
        if self.errors.is_empty() {
            Ok(&self.source)
        } else {
            Err(&self.errors)
        }
    }
}
