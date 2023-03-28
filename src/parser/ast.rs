use crate::lexer::{Symbol, Token};

#[derive(Debug)]
pub struct Module {
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct Function {
    pub return_type: Type,
    pub name: String,
    pub arguments: Vec<IdentifierTypePair>,
    pub statement: Statement,
}

#[derive(Debug)]
pub struct IdentifierTypePair {
    pub typename: Type,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Nil,
    Int,
    Real,
    Bool,
    Char,
    Array(Box<Type>, isize),
    Struct(String),
    Function(Box<Type>, Vec<Type>),
}

#[derive(Debug)]
pub enum Statement {
    Empty,
    Simple(Expression),
    Return(Expression),
    Let(IdentifierTypePair, Expression),
    While(Expression, Box<Statement>),
    If(Expression, Box<Statement>, Option<Box<Statement>>),
    Compound(Vec<Statement>),
}

#[derive(Debug)]
pub enum Expression {
    IntegerLiteral(isize),
    StringLiteral(String),
    BoolLiteral(bool),
    RealLiteral(f64),
    Array(Vec<Expression>),
    Variable(String),
    Binary(Box<Expression>, BinaryOp, Box<Expression>),
    Unary(UnaryOp, Box<Expression>),
    Call(Box<Expression>, Vec<Expression>),
}

#[derive(Debug)]
pub struct FunctionArgument {
    pub name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Equality,
    NotEquality,
    And,
    Or,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
}

impl BinaryOp {
    pub fn get(token: &Token) -> Option<BinaryOp> {
        match token {
            Token::Symbol(Symbol::Equal) => Some(BinaryOp::Assign),
            Token::Symbol(Symbol::Plus) => Some(BinaryOp::Add),
            Token::Symbol(Symbol::Minus) => Some(BinaryOp::Subtract),
            Token::Symbol(Symbol::Star) => Some(BinaryOp::Multiply),
            Token::Symbol(Symbol::Slash) => Some(BinaryOp::Divide),
            Token::Symbol(Symbol::EqualEqual) => Some(BinaryOp::Equality),
            Token::Symbol(Symbol::ExclamEqual) => Some(BinaryOp::NotEquality),
            Token::Symbol(Symbol::Ampersand) => Some(BinaryOp::And),
            Token::Symbol(Symbol::Pipe) => Some(BinaryOp::Or),
            Token::Symbol(Symbol::Greater) => Some(BinaryOp::Greater),
            Token::Symbol(Symbol::GreaterEqual) => Some(BinaryOp::GreaterEqual),
            Token::Symbol(Symbol::Less) => Some(BinaryOp::Less),
            Token::Symbol(Symbol::LessEqual) => Some(BinaryOp::LessEqual),
            _ => None,
        }
    }

    pub fn get_binding_power(&self) -> u32 {
        self.get_binding_power_real() - if self.is_left_associative() { 0 } else { 1 }
    }

    pub fn get_binding_power_real(&self) -> u32 {
        match self {
            BinaryOp::Assign => 2,
            BinaryOp::Or => 4,
            BinaryOp::And => 6,
            BinaryOp::Equality | BinaryOp::NotEquality => 8,
            BinaryOp::Greater | BinaryOp::Less | BinaryOp::GreaterEqual | BinaryOp::LessEqual => 10,
            BinaryOp::Add | BinaryOp::Subtract => 12,
            BinaryOp::Multiply | BinaryOp::Divide => 14,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Equality
            | BinaryOp::NotEquality
            | BinaryOp::Greater
            | BinaryOp::Less
            | BinaryOp::GreaterEqual
            | BinaryOp::LessEqual
            | BinaryOp::And
            | BinaryOp::Or => true,
            BinaryOp::Assign => false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Negate,
}

impl UnaryOp {
    pub fn get_binding_power(&self) -> u32 {
        match self {
            UnaryOp::Not | UnaryOp::Negate => 102,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            UnaryOp::Not | UnaryOp::Negate => false,
        }
    }
}
