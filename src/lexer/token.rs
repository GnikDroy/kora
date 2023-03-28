#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordKind {
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

impl KeywordKind {
    pub fn map(s: &str) -> Option<KeywordKind> {
        match s {
            "ret" => Some(KeywordKind::Ret),
            "let" => Some(KeywordKind::Let),
            "if" => Some(KeywordKind::If),
            "else" => Some(KeywordKind::Else),
            "while" => Some(KeywordKind::While),
            "nil" => Some(KeywordKind::Nil),
            "int" => Some(KeywordKind::Int),
            "real" => Some(KeywordKind::Real),
            "char" => Some(KeywordKind::Char),
            "bool" => Some(KeywordKind::Bool),
            "true" => Some(KeywordKind::True),
            "false" => Some(KeywordKind::False),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
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
pub enum Tok {
    Keyword(KeywordKind),
    Symbol(SymbolKind),
    Identifier(String),
    StringLiteral(String),
    IntegerLiteral(isize),
    RealLiteral(f64),
    Whitespace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token: Tok,
    pub col: usize,
    pub row: usize,
}
