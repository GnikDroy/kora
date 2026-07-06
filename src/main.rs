// Native LLVM codegen CLI. Compiled only with `--features codegen` (requires
// LLVM 22); the default build is the front-end + JS transpiler + wasm
// playground, whose entry point is the library, so `main` is an empty stub.
#[cfg(not(feature = "codegen"))]
fn main() {}

#[cfg(feature = "codegen")]
fn main() -> std::process::ExitCode {
    cli::run()
}

#[cfg(feature = "codegen")]
mod cli {
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode};

    use inkwell::OptimizationLevel;
    use inkwell::context::Context;
    use inkwell::targets::{
        CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
    };

    use kora::codegen;

    const RUNTIME: &str = include_str!("../runtime/libk_rt.c");

    struct Args {
        input: PathBuf,
        output: PathBuf,
        emit_llvm: bool,
    }

    fn parse_args() -> Result<Args, String> {
        let mut input = None;
        let mut output = None;
        let mut emit_llvm = false;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-o" => output = Some(PathBuf::from(args.next().ok_or("-o expects a path")?)),
                "--emit-llvm" => emit_llvm = true,
                _ if input.is_none() => input = Some(PathBuf::from(arg)),
                _ => return Err(format!("unexpected argument: {}", arg)),
            }
        }

        let input = input.ok_or("usage: kora <input.kora> [-o <output>] [--emit-llvm]")?;
        let output = output.unwrap_or_else(|| input.with_extension(""));
        Ok(Args {
            input,
            output,
            emit_llvm,
        })
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

        let context = Context::create();
        let llvm = codegen::compile(&context, &program).map_err(|e| e.to_string())?;

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

        let object_path = args.output.with_extension("o");
        machine
            .write_to_file(&llvm, FileType::Object, &object_path)
            .map_err(|e| e.to_string())?;

        link(&object_path, &args.output)?;
        std::fs::remove_file(&object_path).ok();
        Ok(())
    }

    fn link(object: &Path, output: &Path) -> Result<(), String> {
        let runtime_path = std::env::temp_dir().join("libk_rt.c");
        std::fs::write(&runtime_path, RUNTIME).map_err(|e| e.to_string())?;

        let status = Command::new("cc")
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
        let args = match parse_args() {
            Ok(args) => args,
            Err(e) => {
                eprintln!("{}", e);
                return ExitCode::FAILURE;
            }
        };
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
