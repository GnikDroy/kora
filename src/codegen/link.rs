use std::path::Path;
use std::process::Command;

use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

use super::LinkErr;

const RUNTIME_LIB: &[u8] = include_bytes!(env!("KORA_RUNTIME"));

pub fn link(llvm: &Module, output: &Path) -> Result<(), LinkErr> {
    llvm.verify()
        .unwrap_or_else(|e| panic!("invalid IR generated:\n{e}"));

    let object_path = output.with_extension("o");
    emit_object_file(llvm, &object_path)?;
    let runtime_path = output.with_extension("a");
    let status = std::fs::write(&runtime_path, RUNTIME_LIB)
        .map_err(LinkErr::Io)
        .and_then(|_| {
            Command::new("cc")
                .arg(&object_path)
                .arg(&runtime_path)
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

fn emit_object_file(llvm: &Module, object: &Path) -> Result<(), LinkErr> {
    Target::initialize_native(&InitializationConfig::default()).map_err(LinkErr::EmitObject)?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| LinkErr::EmitObject(e.to_string()))?;
    let machine = target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| LinkErr::EmitObject("cannot create target machine".to_string()))?;
    llvm.set_triple(&triple);
    machine
        .write_to_file(llvm, FileType::Object, object)
        .map_err(|e| LinkErr::EmitObject(e.to_string()))
}
