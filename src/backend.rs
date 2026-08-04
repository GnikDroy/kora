use std::collections::HashSet;

use crate::CompiledProgram;
use crate::javascript_transpiler;

#[cfg(feature = "codegen")]
use std::error::Error;
#[cfg(feature = "codegen")]
use std::fmt;
#[cfg(feature = "codegen")]
use std::path::Path;

#[cfg(feature = "codegen")]
use crate::codegen::{self, CodegenErr, LinkErr};

const NODE_DRIVER: &str = include_str!("../runtime/kora_node_runtime.js");

pub fn node_program(
    program: CompiledProgram,
    async_externs: HashSet<String>,
) -> Result<String, String> {
    let mut js = javascript_transpiler::transpile(program, async_externs)?;
    js.push_str(NODE_DRIVER);
    Ok(js)
}

#[cfg(feature = "codegen")]
#[derive(Debug)]
pub enum BackendErr {
    Codegen(CodegenErr),
    Link(LinkErr),
}

#[cfg(feature = "codegen")]
impl Error for BackendErr {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            BackendErr::Codegen(e) => Some(e),
            BackendErr::Link(e) => Some(e),
        }
    }
}

#[cfg(feature = "codegen")]
impl fmt::Display for BackendErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BackendErr::Codegen(e) => write!(f, "{e}"),
            BackendErr::Link(e) => write!(f, "{e}"),
        }
    }
}

#[cfg(feature = "codegen")]
pub fn native(program: &CompiledProgram, output: &Path, opt: &str) -> Result<(), BackendErr> {
    let context = inkwell::context::Context::create();
    let llvm = codegen::lower(&context, program).map_err(BackendErr::Codegen)?;
    codegen::link(&llvm, output, opt).map_err(BackendErr::Link)
}

#[cfg(feature = "codegen")]
pub fn llvm_ir(program: &CompiledProgram, opt: &str) -> Result<String, BackendErr> {
    let context = inkwell::context::Context::create();
    let llvm = codegen::lower(&context, program).map_err(BackendErr::Codegen)?;
    codegen::optimize_ir(&llvm, opt).map_err(BackendErr::Link)?;
    Ok(llvm.print_to_string().to_string())
}
