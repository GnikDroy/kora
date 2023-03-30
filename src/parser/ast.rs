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

impl Function {
    pub fn get_type(&self) -> Type {
        let args = self
            .arguments
            .iter()
            .map(|IdentifierTypePair { name: _, typename }| typename.clone())
            .collect();
        Type::Function(Box::new(self.return_type.clone()), args)
    }
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
    Identifier(String),
    Binary(Box<Expression>, BinaryOp, Box<Expression>),
    Unary(UnaryOp, Box<Expression>),
    Call(Box<Expression>, Vec<Expression>),
}

#[derive(Debug)]
pub struct FunctionArgument {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl BinaryOp {}

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

pub enum InfixOperator {
    Binary(BinaryOp),
    FunctionCall,
}

impl InfixOperator {
    pub fn get(token: &Token) -> Option<InfixOperator> {
        use InfixOperator::*;
        match token {
            Token::Symbol(Symbol::Equal) => Some(Binary(BinaryOp::Assign)),
            Token::Symbol(Symbol::Plus) => Some(Binary(BinaryOp::Add)),
            Token::Symbol(Symbol::Minus) => Some(Binary(BinaryOp::Subtract)),
            Token::Symbol(Symbol::Star) => Some(Binary(BinaryOp::Multiply)),
            Token::Symbol(Symbol::Slash) => Some(Binary(BinaryOp::Divide)),
            Token::Symbol(Symbol::EqualEqual) => Some(Binary(BinaryOp::Equality)),
            Token::Symbol(Symbol::ExclamEqual) => Some(Binary(BinaryOp::NotEquality)),
            Token::Symbol(Symbol::Ampersand) => Some(Binary(BinaryOp::And)),
            Token::Symbol(Symbol::Pipe) => Some(Binary(BinaryOp::Or)),
            Token::Symbol(Symbol::Greater) => Some(Binary(BinaryOp::Greater)),
            Token::Symbol(Symbol::GreaterEqual) => Some(Binary(BinaryOp::GreaterEqual)),
            Token::Symbol(Symbol::Less) => Some(Binary(BinaryOp::Less)),
            Token::Symbol(Symbol::LessEqual) => Some(Binary(BinaryOp::LessEqual)),
            Token::Symbol(Symbol::LeftParen) => Some(FunctionCall),
            _ => None,
        }
    }

    pub fn get_binding_power_real(&self) -> u32 {
        match self {
            InfixOperator::Binary(op) => match op {
                BinaryOp::Assign => 2,
                BinaryOp::Or => 4,
                BinaryOp::And => 6,
                BinaryOp::Equality | BinaryOp::NotEquality => 8,
                BinaryOp::Greater
                | BinaryOp::Less
                | BinaryOp::GreaterEqual
                | BinaryOp::LessEqual => 10,
                BinaryOp::Add | BinaryOp::Subtract => 12,
                BinaryOp::Multiply | BinaryOp::Divide => 14,
            },
            InfixOperator::FunctionCall => 202,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            InfixOperator::Binary(op) => match op {
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
            },
            InfixOperator::FunctionCall => true,
        }
    }

    pub fn get_binding_power(&self) -> u32 {
        self.get_binding_power_real() - !self.is_left_associative() as u32
    }
}
