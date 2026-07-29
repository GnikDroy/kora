use std::path::Path;
use std::process::Command;

use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

const RUNTIME_LIB: &[u8] = include_bytes!(env!("KORA_RUNTIME"));

pub fn link(llvm: &Module, output: &Path) -> Result<(), String> {
    llvm.verify()
        .map_err(|e| format!("internal error: invalid IR generated:\n{}", e))?;

    Target::initialize_native(&InitializationConfig::default())?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let machine = target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or("cannot create target machine")?;
    llvm.set_triple(&triple);

    let object_path = output.with_extension("o");
    machine
        .write_to_file(llvm, FileType::Object, &object_path)
        .map_err(|e| e.to_string())?;
    let result = cc(&object_path, output);
    std::fs::remove_file(&object_path).ok();
    result
}

fn cc(object: &Path, output: &Path) -> Result<(), String> {
    let runtime_path = output.with_extension("a");
    std::fs::write(&runtime_path, RUNTIME_LIB).map_err(|e| e.to_string())?;

    let status = Command::new("cc")
        .arg(object)
        .arg(&runtime_path)
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|e| format!("cannot run cc: {}", e));
    std::fs::remove_file(&runtime_path).ok();
    if !status?.success() {
        return Err("linking failed".to_string());
    }
    Ok(())
}
