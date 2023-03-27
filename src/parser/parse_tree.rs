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
pub enum Expression {
    NumericLiteral(i64),
    StringLiteral(String),
    Variable(String),
    BinaryExpression(Box<Expression>, BinaryOperator, Box<Expression>),
    UnaryExpression(UnaryOperator, Box<Expression>),
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

#[derive(Debug)]
pub enum UnaryOperator {
    Minus,
}
