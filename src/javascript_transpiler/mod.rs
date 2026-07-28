mod coloring;
mod emit;
mod error;
mod mangle;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use self::error::TranspilerErr;
use self::mangle::{mangle, mangle_prefix};
use crate::loader::LoadedProgram;
use crate::parser::*;
use crate::semantic_analyzer::{ArrayMethod, SymbolId, SymbolTable};

pub(crate) use coloring::resolve_async_fns;

pub fn emit_js(
    compiled: crate::CompiledProgram,
    async_externs: HashSet<String>,
) -> Result<String, String> {
    let entry = &compiled.program.modules.first().unwrap().module;
    let method_calls = mangled_method_calls(&compiled.symbols, &compiled.method_calls);
    let function_names = function_names(&compiled.symbols, &compiled.program);
    let async_fns = resolve_async_fns(entry, &method_calls, async_externs);
    let struct_members = struct_member_map(&compiled.symbols);

    let modules: Vec<&Module> = compiled.program.modules.iter().map(|m| &m.module).collect();
    let mut transpiler = JavascriptTranspiler::new(
        compiled.types,
        method_calls,
        compiled.array_method_calls,
        struct_members,
        function_names,
        async_fns,
    );
    transpiler.emit_program(&modules);

    transpiler.get_source().map(|s| s.to_string()).map_err(|e| {
        e.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(crate) fn struct_member_map(symbols: &SymbolTable) -> HashMap<String, Vec<(String, Type)>> {
    symbols
        .structs
        .iter()
        .map(|(name, def)| (name.clone(), def.members.clone()))
        .collect()
}

pub(crate) fn function_names(
    symbols: &SymbolTable,
    program: &LoadedProgram,
) -> HashMap<NodeId, String> {
    let entry = program.modules.first().map(|m| m.id);
    let mut by_symbol: HashMap<SymbolId, String> = HashMap::new();
    let mut by_node: HashMap<NodeId, String> = HashMap::new();

    for module in &program.modules {
        let prefix = if Some(module.id) == entry {
            String::new()
        } else {
            mangle_prefix(&program.sources[module.id.0 as usize].path)
        };
        for func in &module.module.functions {
            let name = mangle(&prefix, &func.node.name);
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
    /// Mangled name per function definition and call site
    function_names: HashMap<NodeId, String>,
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
        function_names: HashMap<NodeId, String>,
        async_fns: HashSet<String>,
    ) -> JavascriptTranspiler {
        JavascriptTranspiler {
            source: String::new(),
            errors: Vec::new(),
            types,
            method_calls,
            array_method_calls,
            struct_members,
            function_names,
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
