use std::path::{Path, PathBuf};

use clap::Parser;

#[derive(Parser)]
#[command(version)]
pub struct Args {
    input: PathBuf,
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

impl Args {
    fn emit_llvm(&self) -> bool {
        #[cfg(feature = "codegen")]
        return self.emit_llvm;
        #[cfg(not(feature = "codegen"))]
        false
    }
}

const NODE_DRIVER: &str = include_str!("../runtime/kora_node_runtime.js");

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

    if args.emit_js || args.emit_llvm() {
        if args.emit_js && args.emit_llvm() && args.output.is_some() {
            return Err(
                "-o is ambiguous with both --emit-js and --emit-llvm; drop it to use \
                 names derived from the input file"
                    .to_string(),
            );
        }
        emit_llvm(args, &program)?;
        if args.emit_js {
            let mut js = kora::transpile(program, args.async_externs.iter().cloned().collect())?;
            js.push_str(NODE_DRIVER);
            return write_artifact(args, "js", &js);
        }
        return Ok(());
    }

    build(args, program)
}

fn write_artifact(args: &Args, extension: &str, content: &str) -> Result<(), String> {
    let path = match &args.output {
        Some(path) if path == Path::new("-") => {
            print!("{}", content);
            return Ok(());
        }
        Some(path) => path.clone(),
        None => args.input.with_extension(extension),
    };
    std::fs::write(&path, content).map_err(|e| format!("cannot write {}: {}", path.display(), e))
}

#[cfg(feature = "codegen")]
fn emit_llvm(args: &Args, program: &kora::CompiledProgram) -> Result<(), String> {
    use inkwell::context::Context;

    if !args.emit_llvm {
        return Ok(());
    }
    let context = Context::create();
    let llvm = kora::codegen::lower(&context, program).map_err(|e| e.to_string())?;
    let ir = llvm.print_to_string().to_string();
    if args.emit_js {
        let path = args.input.with_extension("ll");
        return std::fs::write(&path, ir)
            .map_err(|e| format!("cannot write {}: {}", path.display(), e));
    }
    write_artifact(args, "ll", &ir)
}

#[cfg(not(feature = "codegen"))]
fn emit_llvm(_args: &Args, _program: &kora::CompiledProgram) -> Result<(), String> {
    Ok(())
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
    kora::codegen::link(&llvm, &output).map_err(|e| e.to_string())
}

#[cfg(not(feature = "codegen"))]
fn build(args: &Args, _program: kora::CompiledProgram) -> Result<(), String> {
    if args.output.is_some() {
        return Err("-o requires --emit-js in a build without the native backend".to_string());
    }
    Ok(())
}
