use crate::parser::Span;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct LoadErr {
    pub msg: String,
    pub span: Option<Span>,
}

impl Error for LoadErr {}

impl fmt::Display for LoadErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.span {
            Some(span) => write!(
                f,
                "error: {} ({}:{})",
                self.msg, span.start.row, span.start.col
            ),
            None => write!(f, "error: {}", self.msg),
        }
    }
}
