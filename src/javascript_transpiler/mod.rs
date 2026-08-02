mod coloring;
mod emit;
mod error;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use self::error::TranspilerErr;
use crate::loader::LoadedProgram;
use crate::parser::*;
use crate::semantic_analyzer::{ArrayMethod, SymbolId, SymbolTable};

pub(crate) use coloring::resolve_async_fns;

pub fn transpile(
    compiled: crate::CompiledProgram,
    async_externs: HashSet<String>,
) -> Result<String, String> {
    let method_calls = method_call_names(&compiled.symbols, &compiled.method_calls, &compiled.emitted);
    let function_call_names =
        function_call_names(&compiled.symbols, &compiled.program, &compiled.emitted);
    let struct_members = struct_member_map(&compiled.symbols);

    let modules: Vec<&Module> = compiled.program.modules.iter().map(|m| &m.module).collect();
    let async_fns = resolve_async_fns(
        &modules,
        &function_call_names,
        &method_calls,
        async_externs,
        &compiled.emitted,
    );
    let mut transpiler = JavascriptTranspiler {
        types: compiled.types,
        method_calls,
        array_method_calls: compiled.array_method_calls,
        struct_members,
        function_call_names,
        async_fns,
        emitted: compiled.emitted,
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

pub(crate) fn function_call_names(
    symbols: &SymbolTable,
    program: &LoadedProgram,
    emitted: &HashMap<NodeId, String>,
) -> HashMap<NodeId, String> {
    let mut by_symbol: HashMap<SymbolId, String> = HashMap::new();
    for module in &program.modules {
        for func in module
            .module
            .extern_functions
            .iter()
            .map(|f| f.id)
            .chain(module.module.functions.iter().map(|f| f.id))
        {
            if let Some(id) = symbols.symbol_id_of_declaration(func) {
                by_symbol.insert(id, emitted[&func].clone());
            }
        }
    }

    symbols
        .uses
        .iter()
        .filter_map(|(use_id, symbol)| {
            by_symbol.get(symbol).map(|name| (*use_id, name.clone()))
        })
        .collect()
}

pub(crate) fn method_call_names(
    symbols: &SymbolTable,
    method_calls: &HashMap<NodeId, SymbolId>,
    emitted: &HashMap<NodeId, String>,
) -> HashMap<NodeId, String> {
    let decl_of: HashMap<SymbolId, NodeId> = symbols
        .declarations
        .iter()
        .map(|(decl, sym)| (*sym, *decl))
        .collect();
    method_calls
        .iter()
        .map(|(id, sym)| (*id, emitted[&decl_of[sym]].clone()))
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
    /// Emitted symbol per function use site
    function_call_names: HashMap<NodeId, String>,
    emitted: HashMap<NodeId, String>,
    /// What color is your function?
    /// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
    /// Necessary because javascript is a colored language.
    async_fns: HashSet<String>,
}

impl JavascriptTranspiler {
    pub fn get_source(&self) -> Result<&str, &[TranspilerErr]> {
        if self.errors.is_empty() {
            Ok(&self.source)
        } else {
            Err(&self.errors)
        }
    }
}
