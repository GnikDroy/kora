use std::path::Path;
use std::process::Command;

use inkwell::module::Module;
use inkwell::targets::FileType;

use super::LinkErr;
use super::optimize::{optimize, target_machine};

const RUNTIME_LIB: &[u8] = include_bytes!(env!("KORA_RUNTIME"));

pub fn link(llvm: &Module, output: &Path, opt: &str) -> Result<(), LinkErr> {
    llvm.verify()
        .unwrap_or_else(|e| panic!("invalid IR generated:\n{e}"));

    let object_path = output.with_extension("o");
    emit_object_file(llvm, &object_path, opt)?;
    let runtime_path = output.with_extension("a");
    let status = std::fs::write(&runtime_path, RUNTIME_LIB)
        .map_err(LinkErr::Io)
        .and_then(|_| {
            let dead_strip = if cfg!(target_os = "macos") {
                "-Wl,-dead_strip"
            } else {
                "-Wl,--gc-sections"
            };
            Command::new("cc")
                .arg(&object_path)
                .arg(&runtime_path)
                .arg("-lm")
                .arg(dead_strip)
                .arg("-o")
                .arg(output)
                .status()
                .map_err(LinkErr::Io)
        });
    std::fs::remove_file(&object_path).ok();
    std::fs::remove_file(&runtime_path).ok();
    if !status?.success() {
        return Err(LinkErr::LinkFailed);
    }
    Ok(())
}

fn emit_object_file(llvm: &Module, object: &Path, opt: &str) -> Result<(), LinkErr> {
    let machine = target_machine(opt)?;
    optimize(llvm, &machine, opt)?;
    machine
        .write_to_file(llvm, FileType::Object, object)
        .map_err(|e| LinkErr::EmitObject(e.to_string()))
}
