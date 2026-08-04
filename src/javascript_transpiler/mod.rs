mod coloring;
mod emit;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

pub fn transpile(
    compiled: crate::CompiledProgram,
    async_externs: HashSet<String>,
) -> Result<String, String> {
    let program = crate::ir::lower(&compiled);
    let async_fns = coloring::resolve_async_fns(&program, async_externs);
    emit::emit(&program, async_fns)
}
