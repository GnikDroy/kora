use std::error::Error;
use std::fmt;

use super::LexerContext;

#[derive(Debug)]
pub struct LexerErr {
    pub msg: &'static str,
    pub context: LexerContext,
    pub suggestion: String,
}

impl Error for LexerErr {}

impl fmt::Display for LexerErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", self.msg)
    }
}
