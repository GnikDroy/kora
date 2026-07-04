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
    Extern,
    As,
    Struct,
    New,
}

impl TryFrom<&str> for Keyword {
    type Error = ();
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        #[rustfmt::skip]
        match s {
            "ret"    => Ok(Keyword::Ret),
            "let"    => Ok(Keyword::Let),
            "if"     => Ok(Keyword::If),
            "else"   => Ok(Keyword::Else),
            "while"  => Ok(Keyword::While),
            "nil"    => Ok(Keyword::Nil),
            "int"    => Ok(Keyword::Int),
            "real"   => Ok(Keyword::Real),
            "char"   => Ok(Keyword::Char),
            "bool"   => Ok(Keyword::Bool),
            "true"   => Ok(Keyword::True),
            "false"  => Ok(Keyword::False),
            "extern" => Ok(Keyword::Extern),
            "as"     => Ok(Keyword::As),
            "struct" => Ok(Keyword::Struct),
            "new"    => Ok(Keyword::New),
            _ => Err(()),
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
    Percent,
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
    Dot,
}

impl TryFrom<char> for Symbol {
    type Error = ();
    fn try_from(c: char) -> Result<Self, Self::Error> {
        use Symbol::*;
        match c {
            '(' => Ok(LeftParen),
            ')' => Ok(RightParen),
            '{' => Ok(LeftBrace),
            '}' => Ok(RightBrace),
            '[' => Ok(LeftBracket),
            ']' => Ok(RightBracket),
            ';' => Ok(Semicolon),
            ':' => Ok(Colon),
            ',' => Ok(Comma),
            '+' => Ok(Plus),
            '-' => Ok(Minus),
            '*' => Ok(Star),
            '/' => Ok(Slash),
            '%' => Ok(Percent),
            '=' => Ok(Equal),
            '>' => Ok(Greater),
            '<' => Ok(Less),
            '!' => Ok(Exclam),
            '|' => Ok(Pipe),
            '&' => Ok(Ampersand),
            '.' => Ok(Dot),
            _ => Err(()),
        }
    }
}

impl TryFrom<&str> for Symbol {
    type Error = ();
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        use Symbol::*;
        if let Some(c) = s.chars().next()
            && s.len() == 1
        {
            Symbol::try_from(c)
        } else {
            match s {
                "==" => Ok(EqualEqual),
                ">=" => Ok(GreaterEqual),
                "<=" => Ok(LessEqual),
                "!=" => Ok(ExclamEqual),
                _ => Err(()),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(Keyword),
    Symbol(Symbol),
    Identifier(String),
    StringLiteral(String),
    CharLiteral(u8),
    IntegerLiteral(isize),
    RealLiteral(f64),
    Whitespace,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct LexerContext {
    pub col: isize,
    pub row: isize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo {
    pub token: Token,
    pub start: LexerContext,
    pub end: LexerContext,
}
