use crate::lexer::Token;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct ParseError {
    pub msg: String,
    pub token: Option<Token>,
}
impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}
