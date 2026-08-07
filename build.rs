fn main() {
    if std::env::var_os("CARGO_FEATURE_CODEGEN").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=runtime/libkora");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");

    let mut build = cc::Build::new();
    build
        .file("runtime/libkora/libkora.c")
        .file("runtime/libkora/bdwgc/extra/gc.c")
        .include("runtime/libkora/bdwgc/include")
        .define("NO_EXECUTE_PERMISSION", None)
        .define("GC_DISABLE_INCREMENTAL", None)
        .define("GC_THREADS", None)
        .define("GC_BUILTIN_ATOMIC", None)
        .opt_level(2)
        .flag_if_supported("-mmacosx-version-min=11.0")
        .warnings(false)
        .cargo_metadata(false);

    // Mbed TLS with threading enabled
    let mut tls_sources: Vec<_> = std::fs::read_dir("runtime/libkora/mbedtls/library")
        .expect("vendored mbedtls sources")
        .filter_map(|e| {
            let path = e.unwrap().path();
            (path.extension().and_then(|x| x.to_str()) == Some("c")).then_some(path)
        })
        .collect();
    tls_sources.sort();
    build
        .files(tls_sources)
        .include("runtime/libkora")
        .include("runtime/libkora/mbedtls/include")
        .include("runtime/libkora/mbedtls/library")
        .define("MBEDTLS_THREADING_C", None)
        .define(
            if windows { "MBEDTLS_THREADING_ALT" } else { "MBEDTLS_THREADING_PTHREAD" },
            None,
        );

    if msvc {
        build.compiler("clang-cl").flag_if_supported("/Gy");
    } else {
        build
            .flag_if_supported("-ffunction-sections")
            .flag_if_supported("-fdata-sections");
    }

    build.compile("kora");
    let out = std::env::var("OUT_DIR").unwrap();
    let runtime = if msvc { "kora.lib" } else { "libkora.a" };
    println!("cargo:rustc-env=KORA_RUNTIME={out}/{runtime}");
}
