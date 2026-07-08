use crate::parser::{SourceId, Span};

#[derive(Debug)]
pub struct LoadError {
    pub msg: String,
    pub source: SourceId,
    pub span: Option<Span>,
}
