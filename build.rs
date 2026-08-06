fn main() {
    if std::env::var_os("CARGO_FEATURE_CODEGEN").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=runtime/libkora");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");

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
