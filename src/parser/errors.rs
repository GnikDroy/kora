use crate::lexer::TokenInfo;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct ParseErr {
    pub msg: &'static str,
    pub token: Option<TokenInfo>,
}
impl Error for ParseErr {}

impl fmt::Display for ParseErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.token {
            Some(tok) => write!(
                f,
                "error: {}\n{}:{}\n{:?}",
                self.msg, tok.col, tok.row, tok.token
            ),

            None => write!(f, "error: {}\nEOF", self.msg),
        }
    }
}
