//! WASM entry points for the browser playground.

use std::path::Path;

use wasm_bindgen::prelude::*;

use crate::compile;

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

    crate::javascript_transpiler::emit_js(compiled, async_externs.into_iter().collect())
}
