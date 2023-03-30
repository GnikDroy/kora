use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct TypeErr {
    pub msg: &'static str,
}
impl Error for TypeErr {}

impl fmt::Display for TypeErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "error: {}", self.msg)
    }
}
