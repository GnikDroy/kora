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
        use inkwell::context::Context;

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

        kora::codegen::build_binary(&llvm, &output)
    }

    #[cfg(not(feature = "codegen"))]
    fn native(_args: &Args, _program: kora::CompiledProgram) -> Result<(), String> {
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
