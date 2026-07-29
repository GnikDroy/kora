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

#[derive(Debug)]
pub enum LinkErr {
    EmitObject(String),
    Io(std::io::Error),
    LinkFailed,
}

impl Error for LinkErr {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LinkErr::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for LinkErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LinkErr::EmitObject(e) => write!(f, "{e}"),
            LinkErr::Io(e) => write!(f, "{e}"),
            LinkErr::LinkFailed => write!(f, "linking failed"),
        }
    }
}
