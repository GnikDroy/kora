#[derive(Debug)]
pub struct Module {
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct Function {
    pub ret_type: Typename,
    pub name: String,
    pub args: Vec<FunctionParameter>,
    pub statement: Statement,
}

#[derive(Debug)]
pub struct FunctionParameter {
    pub typename: Typename,
    pub name: String,
}

#[derive(Debug)]
pub enum Typename {
    Int,
    Real,
    Bool,
    Char,
    Array(Box<Typename>, i64),
    Struct(String),
}

#[derive(Debug)]
pub enum Statement {
    Empty,
    Return(Expression),
    CompoundStatement(Vec<Statement>),
}

#[derive(Debug)]
pub enum Term {
    NumericLiteral(i64),
    StringLiteral(String),
    Variable(String),
}

#[derive(Debug)]
pub enum Expression {
    ExpressionTerm(Term),
    BinaryExpression(Box<Expression>, BinaryOperator, Box<Expression>),
    UnaryExpression(UnaryOperator, Box<Expression>),
    CallExpression(Term, Vec<Expression>),
}

#[derive(Debug)]
pub struct FunctionArgument {
    pub name: String,
}

#[derive(Debug)]
pub enum BinaryOperator {
    Plus,
    Minus,
    Star,
    Slash,
}

impl BinaryOperator {
    pub fn get_binding_power(&self) -> u32 {
        match self {
            BinaryOperator::Plus | BinaryOperator::Minus => 10,
            BinaryOperator::Star | BinaryOperator::Slash => 10,
        }
    }
    pub fn is_left_associative(&self) -> bool {
        match self {
            BinaryOperator::Plus
            | BinaryOperator::Minus
            | BinaryOperator::Star
            | BinaryOperator::Slash => true,
        }
    }

    pub fn is_right_associative(&self) -> bool {
        return !self.is_left_associative();
    }
}

#[derive(Debug)]
pub enum UnaryOperator {
    Minus,
}
