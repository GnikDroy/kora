#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Ret,
    Let,
    If,
    Else,
    While,
    Nil,
    Int,
    Real,
    Char,
    Bool,
    True,
    False,
}

impl Keyword {
    pub fn map(s: &str) -> Option<Keyword> {
        match s {
            "ret" => Some(Keyword::Ret),
            "let" => Some(Keyword::Let),
            "if" => Some(Keyword::If),
            "else" => Some(Keyword::Else),
            "while" => Some(Keyword::While),
            "nil" => Some(Keyword::Nil),
            "int" => Some(Keyword::Int),
            "real" => Some(Keyword::Real),
            "char" => Some(Keyword::Char),
            "bool" => Some(Keyword::Bool),
            "true" => Some(Keyword::True),
            "false" => Some(Keyword::False),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Semicolon,
    Colon,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Equal,
    EqualEqual,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Exclam,
    ExclamEqual,
    Pipe,
    Ampersand,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(Keyword),
    Symbol(Symbol),
    Identifier(String),
    StringLiteral(String),
    IntegerLiteral(isize),
    RealLiteral(f64),
    Whitespace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo {
    pub token: Token,
    pub col: usize,
    pub row: usize,
}
