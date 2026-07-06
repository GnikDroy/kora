use crate::parser::Span;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct CodegenErr {
    pub(crate) msg: &'static str,
    pub(crate) span: Span,
}
impl Error for CodegenErr {}

impl fmt::Display for CodegenErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "error: {} ({}:{})",
            self.msg, self.span.start.row, self.span.start.col
        )
    }
}
