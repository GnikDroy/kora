//! WASM entry points for the browser playground.

use std::path::Path;

use wasm_bindgen::prelude::*;

use crate::compile;
use crate::javascript_transpiler::JavascriptTranspiler;
use crate::parser::Module;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOCATOR: lol_alloc::AssumeSingleThreaded<lol_alloc::FreeListAllocator> =
    unsafe { lol_alloc::AssumeSingleThreaded::new(lol_alloc::FreeListAllocator::new()) };

#[wasm_bindgen(start)]
pub fn on_start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn transpile(source: &str, async_externs: Vec<String>) -> Result<String, String> {
    // The caller supplies the source already carrying the extern prelude, and
    // the runtime provides the host implementations, so transpile just lowers.
    let entry = "main.kora";
    let compiled = compile(entry, |path: &Path| {
        (path == Path::new(entry)).then(|| source.to_string())
    })
    .map_err(|e| {
        e.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let entry = &compiled.program.modules.first().unwrap().module;
    let method_calls = crate::javascript_transpiler::mangled_method_calls(
        &compiled.symbols,
        &compiled.method_calls,
    );
    let function_names =
        crate::javascript_transpiler::function_names(&compiled.symbols, &compiled.program);
    let async_fns = crate::javascript_transpiler::resolve_async_fns(
        entry,
        &method_calls,
        async_externs.into_iter().collect(),
    );
    let struct_members = crate::javascript_transpiler::struct_member_map(&compiled.symbols);

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
