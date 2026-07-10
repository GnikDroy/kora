use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct TranspilerErr {
    pub msg: &'static str,
}

impl Error for TranspilerErr {}

impl fmt::Display for TranspilerErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error: {}", self.msg)
    }
}
