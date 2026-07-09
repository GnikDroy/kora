use crate::parser::Span;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone)]
pub struct TypeErr {
    pub msg: &'static str,
    pub span: Span,
}
impl Error for TypeErr {}

impl fmt::Display for TypeErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "error: {} ({}:{})",
            self.msg, self.span.start.row, self.span.start.col
        )
    }
}
