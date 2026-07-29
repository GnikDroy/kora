#![allow(dead_code)]

mod lexer;
mod loader;
mod mangle;
mod parser;
mod semantic_analyzer;
mod stdlib;

#[cfg(feature = "codegen")]
pub mod codegen;
mod frontend;
mod javascript_transpiler;

mod playground;

pub use frontend::{CompileErr, CompiledProgram, compile};
pub use javascript_transpiler::transpile;
pub use lexer::LexerErr;
pub use loader::LoadErr;
pub use parser::ParseErr;
pub use semantic_analyzer::TypeErr;
