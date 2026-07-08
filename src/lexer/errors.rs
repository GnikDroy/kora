use std::error::Error;
use std::fmt;

use super::Position;

#[derive(Debug)]
pub struct LexerErr {
    pub msg: &'static str,
    pub position: Position,
    pub suggestion: String,
}

impl Error for LexerErr {}

impl fmt::Display for LexerErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "error: {} ({}:{})\nsuggestion: {}",
            self.msg, self.position.row, self.position.col, self.suggestion
        )
    }
}
