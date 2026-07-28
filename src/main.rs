fn main() -> std::process::ExitCode {
    cli::run()
}

mod cli {
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;

    use clap::Parser;

    #[derive(Parser)]
    #[command(version)]
    struct Args {
        input: PathBuf,
        #[cfg(feature = "codegen")]
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[cfg(feature = "codegen")]
        #[arg(long)]
        emit_llvm: bool,
        #[arg(long)]
        emit_js: bool,
        #[arg(long = "async-extern", value_name = "NAME")]
        async_externs: Vec<String>,
    }

    fn compile(args: &Args) -> Result<(), String> {
        let entry = args
            .input
            .to_str()
            .ok_or_else(|| format!("input path is not valid UTF-8: {}", args.input.display()))?;
        let program = kora::compile(entry, |path: &Path| std::fs::read_to_string(path).ok())
            .map_err(|errors| {
                errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;

        if args.emit_js {
            let js = kora::emit_js(program, args.async_externs.iter().cloned().collect())?;
            print!("{}", js);
            return Ok(());
        }

        native(args, program)
    }

    #[cfg(feature = "codegen")]
    fn native(args: &Args, program: kora::CompiledProgram) -> Result<(), String> {
        use inkwell::OptimizationLevel;
        use inkwell::context::Context;
        use inkwell::targets::{
            CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
        };

        let output = args
            .output
            .clone()
            .unwrap_or_else(|| args.input.with_extension(""));

        let context = Context::create();
        let llvm = kora::codegen::compile(&context, &program).map_err(|e| e.to_string())?;

        if args.emit_llvm {
            print!("{}", llvm.print_to_string().to_string());
            return Ok(());
        }

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
            .write_to_file(&llvm, FileType::Object, &object_path)
            .map_err(|e| e.to_string())?;

        link(&object_path, &output)?;
        std::fs::remove_file(&object_path).ok();
        Ok(())
    }

    #[cfg(not(feature = "codegen"))]
    fn native(_args: &Args, _program: kora::CompiledProgram) -> Result<(), String> {
        Ok(())
    }

    #[cfg(feature = "codegen")]
    fn link(object: &Path, output: &Path) -> Result<(), String> {
        const RUNTIME_LIB: &[u8] = include_bytes!(env!("KORA_RUNTIME"));

        let runtime_path = std::env::temp_dir().join("libkora.a");
        std::fs::write(&runtime_path, RUNTIME_LIB).map_err(|e| e.to_string())?;

        let status = std::process::Command::new("cc")
            .arg(object)
            .arg(&runtime_path)
            .arg("-o")
            .arg(output)
            .status()
            .map_err(|e| format!("cannot run cc: {}", e))?;
        if !status.success() {
            return Err("linking failed".to_string());
        }
        Ok(())
    }

    pub fn run() -> ExitCode {
        let args = Args::parse();
        match compile(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprint!("{}", e);
                if !e.ends_with('\n') {
                    eprintln!();
                }
                ExitCode::FAILURE
            }
        }
    }
}
