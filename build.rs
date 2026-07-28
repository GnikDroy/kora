fn main() {
    if std::env::var_os("CARGO_FEATURE_CODEGEN").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=runtime/libkora.c");
    println!("cargo:rerun-if-changed=runtime/bdwgc");
    cc::Build::new()
        .file("runtime/libkora.c")
        .file("runtime/bdwgc/extra/gc.c")
        .include("runtime/bdwgc/include")
        .define("NO_EXECUTE_PERMISSION", None)
        .define("GC_DISABLE_INCREMENTAL", None)
        .flag_if_supported("-mmacosx-version-min=11.0")
        .warnings(false)
        .cargo_metadata(false)
        .compile("kora");
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-env=KORA_RUNTIME={out}/libkora.a");
}
