use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};

use super::LinkErr;

fn machine_level(opt: &str) -> OptimizationLevel {
    match opt {
        "0" => OptimizationLevel::None,
        "1" => OptimizationLevel::Less,
        "3" => OptimizationLevel::Aggressive,
        _ => OptimizationLevel::Default, // 2, s, z
    }
}

pub(super) fn target_machine(opt: &str) -> Result<TargetMachine, LinkErr> {
    Target::initialize_native(&InitializationConfig::default()).map_err(LinkErr::EmitObject)?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| LinkErr::EmitObject(e.to_string()))?;
    target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            machine_level(opt),
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| LinkErr::EmitObject("cannot create target machine".to_string()))
}

pub(super) fn optimize(llvm: &Module, machine: &TargetMachine, opt: &str) -> Result<(), LinkErr> {
    llvm.set_triple(&machine.get_triple());
    llvm.set_data_layout(&machine.get_target_data().get_data_layout());
    if opt != "0" {
        let pipeline = format!("default<O{opt}>");
        llvm.run_passes(&pipeline, machine, PassBuilderOptions::create())
            .map_err(|e| LinkErr::EmitObject(e.to_string()))?;
    }
    Ok(())
}

pub fn optimize_ir(llvm: &Module, opt: &str) -> Result<(), LinkErr> {
    let machine = target_machine(opt)?;
    optimize(llvm, &machine, opt)
}
