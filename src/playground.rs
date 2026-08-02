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

    crate::javascript_transpiler::transpile(compiled, async_externs.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reports_instantiation_note() {
        let err = transpile(
            r#"
                struct lt<T> {}
                impl lt<T> { bool less(self, a: T, b: T) { return a < b; } }
                int main() {
                    let c = new lt<string>;
                    if (c.less("a", "b")) { return 0; }
                    return 1;
                }
            "#,
            vec![],
        )
        .unwrap_err();
        assert!(err.contains("instantiated here"), "{err}");
    }

    #[test]
    fn test_reports_undefined_type_argument() {
        let err = transpile(
            r#"
                import "std/time";
                import "std/conv";
                struct P<T> { node: P<T>? }
                int main() {
                    let p = new P<T>{ node: none };
                    return 0;
                }
            "#,
            vec![],
        )
        .unwrap_err();
        assert!(err.contains("Undefined type"), "{err}");
    }

    #[test]
    fn test_reports_runaway_instantiation() {
        let err = transpile(
            r#"
                struct w<T> { inner: w<w<T>>? }
                int main() { let x = new w<int>{ inner: none }; return 0; }
            "#,
            vec![],
        )
        .unwrap_err();
        assert!(err.contains("depth limit"), "{err}");
    }
}
