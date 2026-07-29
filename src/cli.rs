use std::path::{Path, PathBuf};

use clap::Parser;

#[derive(Parser)]
#[command(version)]
pub struct Args {
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

pub fn run(args: &Args) -> Result<(), String> {
    let entry = args
        .input
        .to_str()
        .ok_or_else(|| format!("input path is not valid UTF-8: {}", args.input.display()))?;
    let program = kora::compile(entry, |path: &Path| std::fs::read_to_string(path).ok()).map_err(
        |errors| {
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        },
    )?;

    if args.emit_js {
        let js = kora::transpile(program, args.async_externs.iter().cloned().collect())?;
        print!("{}", js);
        return Ok(());
    }

    build(args, program)
}

#[cfg(feature = "codegen")]
fn build(args: &Args, program: kora::CompiledProgram) -> Result<(), String> {
    use inkwell::context::Context;

    let output = args
        .output
        .clone()
        .unwrap_or_else(|| args.input.with_extension(""));

    let context = Context::create();
    let llvm = kora::codegen::lower(&context, &program).map_err(|e| e.to_string())?;

    if args.emit_llvm {
        print!("{}", llvm.print_to_string().to_string());
        return Ok(());
    }

    kora::codegen::link(&llvm, &output).map_err(|e| e.to_string())
}

#[cfg(not(feature = "codegen"))]
fn build(_args: &Args, _program: kora::CompiledProgram) -> Result<(), String> {
    Ok(())
}
