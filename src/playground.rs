//! WASM entry points for the browser playground.

use std::path::Path;

use wasm_bindgen::prelude::*;

use crate::compile;
use crate::javascript_transpiler::JavascriptTranspiler;
use crate::parser::ASTVisitor;

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

    let module = &compiled.program.modules.first().unwrap().module;
    let method_calls = crate::javascript_transpiler::mangled_method_calls(
        &compiled.symbols,
        &compiled.method_calls,
    );
    let async_fns = crate::javascript_transpiler::resolve_async_fns(
        module,
        &method_calls,
        async_externs.into_iter().collect(),
    );

    let struct_members = crate::javascript_transpiler::struct_member_map(&compiled.symbols);
    let mut transpiler = JavascriptTranspiler::new(
        compiled.types,
        method_calls,
        compiled.array_method_calls,
        struct_members,
        async_fns,
    );

    transpiler.visit_module(module);

    transpiler.get_source().map(|s| s.to_string()).map_err(|e| {
        e.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}
