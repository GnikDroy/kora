use std::error::Error;
use std::fmt;

use crate::parser::Span;

#[derive(Debug, Clone)]
pub struct InstantiateErr {
    pub msg: String,
    pub span: Span,
}

impl Error for InstantiateErr {}

impl fmt::Display for InstantiateErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let prefix = if self.msg.starts_with("note:") {
            ""
        } else {
            "error: "
        };
        write!(
            f,
            "{prefix}{} ({}:{})",
            self.msg, self.span.start.row, self.span.start.col
        )
    }
}

#[derive(Debug)]
pub struct GenericRegion {
    pub span: Span,
    pub instances: Vec<(String, Span)>,
}
