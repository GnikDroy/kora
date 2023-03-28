use crate::lexer::{SymbolKind, Tok};

#[derive(Debug)]
pub struct Module {
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct Function {
    pub ret_type: Typename,
    pub name: String,
    pub args: Vec<IdentifierTypePair>,
    pub statement: Statement,
}

#[derive(Debug)]
pub struct IdentifierTypePair {
    pub typename: Typename,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Typename {
    Nil,
    Int,
    Real,
    Bool,
    Char,
    Array(Box<Typename>, isize),
    Struct(String),
    Function(Box<Typename>, Vec<Typename>),
}

#[derive(Debug)]
pub enum Statement {
    Empty,
    Simple(Expression),
    Return(Expression),
    Let(IdentifierTypePair, Expression),
    While(Expression, Box<Statement>),
    If(Expression, Box<Statement>, Option<Box<Statement>>),
    CompoundStatement(Vec<Statement>),
}

#[derive(Debug)]
pub enum Expression {
    IntegerLiteral(isize),
    StringLiteral(String),
    BooleanLiteral(bool),
    RealLiteral(f64),
    Array(Vec<Expression>),
    Variable(String),
    BinaryExpression(Box<Expression>, BinaryOperator, Box<Expression>),
    UnaryExpression(UnaryOperator, Box<Expression>),
    CallExpression(Box<Expression>, Vec<Expression>),
}

#[derive(Debug)]
pub struct FunctionArgument {
    pub name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BinaryOperator {
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

impl BinaryOperator {
    pub fn get(token: &Tok) -> Option<BinaryOperator> {
        match token {
            Tok::Symbol(SymbolKind::Equal) => Some(BinaryOperator::Assign),
            Tok::Symbol(SymbolKind::Plus) => Some(BinaryOperator::Add),
            Tok::Symbol(SymbolKind::Minus) => Some(BinaryOperator::Subtract),
            Tok::Symbol(SymbolKind::Star) => Some(BinaryOperator::Multiply),
            Tok::Symbol(SymbolKind::Slash) => Some(BinaryOperator::Divide),
            Tok::Symbol(SymbolKind::EqualEqual) => Some(BinaryOperator::Equality),
            Tok::Symbol(SymbolKind::ExclamEqual) => Some(BinaryOperator::NotEquality),
            Tok::Symbol(SymbolKind::Ampersand) => Some(BinaryOperator::And),
            Tok::Symbol(SymbolKind::Pipe) => Some(BinaryOperator::Or),
            Tok::Symbol(SymbolKind::Greater) => Some(BinaryOperator::Greater),
            Tok::Symbol(SymbolKind::GreaterEqual) => Some(BinaryOperator::GreaterEqual),
            Tok::Symbol(SymbolKind::Less) => Some(BinaryOperator::Less),
            Tok::Symbol(SymbolKind::LessEqual) => Some(BinaryOperator::LessEqual),
            _ => None,
        }
    }

    pub fn get_binding_power(&self) -> u32 {
        match self {
            BinaryOperator::Assign => 2,
            BinaryOperator::Or => 4,
            BinaryOperator::And => 6,
            BinaryOperator::Equality | BinaryOperator::NotEquality => 8,
            BinaryOperator::Greater
            | BinaryOperator::Less
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LessEqual => 10,
            BinaryOperator::Add | BinaryOperator::Subtract => 12,
            BinaryOperator::Multiply | BinaryOperator::Divide => 14,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Equality
            | BinaryOperator::NotEquality
            | BinaryOperator::Greater
            | BinaryOperator::Less
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LessEqual
            | BinaryOperator::And
            | BinaryOperator::Or => true,
            BinaryOperator::Assign => false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum UnaryOperator {
    Not,
    Negate,
}

impl UnaryOperator {
    pub fn get_binding_power(&self) -> u32 {
        match self {
            UnaryOperator::Not | UnaryOperator::Negate => 102,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            UnaryOperator::Not | UnaryOperator::Negate => false,
        }
    }
}
