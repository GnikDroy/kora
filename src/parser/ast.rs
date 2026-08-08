use crate::lexer::{Keyword, Position, Symbol, Token};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Source range for an AST node
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Span {
    pub source: SourceId,
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub fn to(&self, other: &Span) -> Span {
        Span {
            source: self.source,
            start: self.start.clone(),
            end: other.end.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(pub u32);

impl SourceId {
    pub const ANON: SourceId = SourceId(u32::MAX);
}

impl Default for SourceId {
    fn default() -> Self {
        SourceId::ANON
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u64);

impl NodeId {
    pub fn new() -> NodeId {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        NodeId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// An AST node annotated with where it was written and a stable `NodeId`.
/// Equality, `Hash`, and `Debug` delegate to `node` - the `span` and `id` are
/// metadata, so two structurally-equal nodes compare/hash equal regardless.
#[derive(Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
    pub id: NodeId,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Spanned::with_id(node, span, NodeId::new())
    }

    pub fn with_id(node: T, span: Span, id: NodeId) -> Self {
        Spanned { node, span, id }
    }
}

impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.node.fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

impl<T: Eq> Eq for Spanned<T> {}

impl<T: Hash> Hash for Spanned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}

#[derive(Debug, Default)]
pub struct Module {
    pub imports: Vec<Spanned<Import>>,
    pub structs: Vec<Spanned<Struct>>,
    pub extern_functions: Vec<Spanned<ExternFunction>>,
    pub functions: Vec<Spanned<Function>>,
    pub impls: Vec<Spanned<Impl>>,
    pub generic_structs: Vec<Spanned<GenericStruct>>,
    pub generic_functions: Vec<Spanned<GenericFunction>>,
    pub generic_impls: Vec<Spanned<GenericImpl>>,
    pub globals: Vec<Spanned<Global>>,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub name: Spanned<String>,
    pub typename: Option<Type>,
    pub value: Spanned<Expression>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

/// Methods for a struct. Each function's first argument is the synthesized
/// `self` parameter, typed as the impl'd struct, so downstream passes treat
/// methods as ordinary functions.
#[derive(Debug, Clone)]
pub struct Impl {
    pub struct_ref: StructRef,
    pub functions: Vec<Spanned<Function>>,
}

/// A mention of a struct. `target` identifies the declaration it refers to;
/// user-written mentions carry `None` and resolve by name, while types the
/// instantiate pass creates carry the instance declaration's id.
#[derive(Debug, Clone)]
pub struct StructRef {
    pub name: Spanned<String>,
    pub target: Option<NodeId>,
}

impl StructRef {
    pub fn unresolved(name: Spanned<String>) -> StructRef {
        StructRef { name, target: None }
    }
}

impl PartialEq for StructRef {
    fn eq(&self, other: &Self) -> bool {
        match (self.target, other.target) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.name.node == other.name.node,
            _ => false,
        }
    }
}

impl Eq for StructRef {}

impl Hash for StructRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.target {
            Some(id) => {
                1u8.hash(state);
                id.hash(state);
            }
            None => {
                0u8.hash(state);
                self.name.node.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: String,
    pub members: Vec<Spanned<IdentifierTypePair>>,
}

#[derive(Debug, Clone)]
pub struct GenericStruct {
    pub name: String,
    pub type_params: Vec<Spanned<String>>,
    pub members: Vec<Spanned<IdentifierTypePair>>,
}

#[derive(Debug, Clone)]
pub struct GenericFunction {
    pub return_type: Option<Type>,
    pub name: String,
    pub type_params: Vec<Spanned<String>>,
    pub arguments: Vec<Spanned<IdentifierTypePair>>,
    pub statement: Spanned<Statement>,
}

#[derive(Debug, Clone)]
pub struct GenericImpl {
    pub struct_name: Spanned<String>,
    pub type_params: Vec<Spanned<String>>,
    pub functions: Vec<Spanned<Function>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternType {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,
    Char,
    CString,
    Opaque,
    CInt,
    CUInt,
    CLong,
    CULong,
    CSize,
    Optional(Box<ExternType>),
    Function {
        params: Vec<ExternType>,
        ret: Option<Box<ExternType>>,
    },
}

impl ExternType {
    pub fn projection(&self) -> Type {
        use ExternType::*;
        match self {
            Int8 | Int16 | Int32 | Int64 | UInt8 | UInt16 | UInt32 | UInt64 | CInt | CUInt
            | CLong | CULong | CSize => Type::Int,
            Float32 | Float64 => Type::Real,
            ExternType::Bool => Type::Bool,
            ExternType::Char => Type::Char,
            CString => Type::Array(Box::new(Type::Char)),
            ExternType::Opaque => Type::Opaque,
            Optional(inner) => Type::Optional(Box::new(inner.projection())),
            Function { params, ret } => Type::Function(
                ret.as_ref().map(|r| Box::new(r.projection())),
                params.iter().map(|p| p.projection()).collect(),
            ),
        }
    }

    pub fn has_identical_crepr(&self) -> bool {
        use ExternType::*;
        match self {
            CLong | CULong if cfg!(target_env = "msvc") => false,
            Int64 | UInt64 | CLong | CULong | CSize | Float64 | Char | Opaque => true,
            Optional(inner) => matches!(**inner, Opaque),
            Function { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternParameter {
    pub typename: ExternType,
    pub name: String,
}

#[derive(Debug)]
pub struct ExternFunction {
    pub return_type: Option<ExternType>,
    pub name: String,
    pub arguments: Vec<Spanned<ExternParameter>>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub return_type: Option<Type>,
    pub name: String,
    pub arguments: Vec<Spanned<IdentifierTypePair>>,
    pub statement: Spanned<Statement>,
}

impl Function {
    pub fn get_type(&self) -> Type {
        get_type(&self.return_type, &self.arguments)
    }
}

impl ExternFunction {
    pub fn get_type(&self) -> Type {
        let args = self
            .arguments
            .iter()
            .map(|arg| arg.node.typename.projection())
            .collect();
        let ret = self
            .return_type
            .as_ref()
            .map(|ty| Box::new(ty.projection()));
        Type::Function(ret, args)
    }
}

fn get_type(return_type: &Option<Type>, args: &[Spanned<IdentifierTypePair>]) -> Type {
    let args = args.iter().map(|pair| pair.node.typename.clone()).collect();
    Type::Function(return_type.clone().map(Box::new), args)
}

#[derive(Debug, Clone)]
pub struct IdentifierTypePair {
    pub typename: Type,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Opaque,
    Real,
    Bool,
    Char,
    Array(Box<Type>),
    Optional(Box<Type>),
    Struct(StructRef),
    Generic(StructRef, Vec<Type>),
    Function(Option<Box<Type>>, Vec<Type>),
}

#[derive(Debug, Clone)]
pub enum Statement {
    Empty,
    Simple(Spanned<Expression>),
    Return(Option<Spanned<Expression>>),
    Let(Spanned<String>, Option<Type>, Spanned<Expression>),
    While(Spanned<Expression>, Box<Spanned<Statement>>),
    For(
        Box<Spanned<Statement>>,
        Spanned<Expression>,
        Spanned<Expression>,
        Box<Spanned<Statement>>,
    ),
    Break,
    Continue,
    If(
        Spanned<Expression>,
        Box<Spanned<Statement>>,
        Option<Box<Spanned<Statement>>>,
    ),
    TypeIf(
        Type,
        Type,
        Box<Spanned<Statement>>,
        Option<Box<Spanned<Statement>>>,
    ),
    Compound(Vec<Spanned<Statement>>),
}

#[derive(Debug, Clone)]
pub enum Expression {
    IntegerLiteral(isize),
    CharLiteral(u8),
    StringLiteral(String),
    BoolLiteral(bool),
    RealLiteral(f64),
    NoneLiteral,
    Unwrap(Box<Spanned<Expression>>),
    Array(Vec<Spanned<Expression>>),
    Identifier(String),
    Binary(Box<Spanned<Expression>>, BinaryOp, Box<Spanned<Expression>>),
    Unary(UnaryOp, Box<Spanned<Expression>>),
    Call(Box<Spanned<Expression>>, Vec<Spanned<Expression>>),
    ArrayIndex(Box<Spanned<Expression>>, Box<Spanned<Expression>>),
    Cast(Box<Spanned<Expression>>, Type),
    Access(Box<Spanned<Expression>>, String),
    Construct(Type, Option<Box<Spanned<Expression>>>),
    StructLiteral(Type, Vec<(Spanned<String>, Spanned<Expression>)>),
    TypeApplication(Box<Spanned<Expression>>, Vec<Type>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equality,
    NotEquality,
    And,
    Or,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Cast,
}

impl BinaryOp {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    ArrayIndex,
    Access,
    Unwrap,
    TypeApplication,
}

impl TryFrom<Token> for InfixOperator {
    type Error = ();
    #[rustfmt::skip]
    fn try_from(token: Token) -> Result<Self, Self::Error> {
        use BinaryOp::*;
        use InfixOperator::*;
        match token {
            Token::Symbol(Symbol::Equal)              => Ok(Binary(Assign)),
            Token::Symbol(Symbol::Plus)               => Ok(Binary(Add)),
            Token::Symbol(Symbol::Minus)              => Ok(Binary(Subtract)),
            Token::Symbol(Symbol::Star)               => Ok(Binary(Multiply)),
            Token::Symbol(Symbol::Slash)              => Ok(Binary(Divide)),
            Token::Symbol(Symbol::Percent)            => Ok(Binary(Modulo)),
            Token::Symbol(Symbol::EqualEqual)         => Ok(Binary(Equality)),
            Token::Symbol(Symbol::ExclamEqual)        => Ok(Binary(NotEquality)),
            Token::Symbol(Symbol::AmpersandAmpersand) => Ok(Binary(And)),
            Token::Symbol(Symbol::PipePipe)           => Ok(Binary(Or)),
            Token::Symbol(Symbol::Ampersand)          => Ok(Binary(BitAnd)),
            Token::Symbol(Symbol::Pipe)               => Ok(Binary(BitOr)),
            Token::Symbol(Symbol::Caret)              => Ok(Binary(BitXor)),
            Token::Symbol(Symbol::LessLess)           => Ok(Binary(ShiftLeft)),
            Token::Symbol(Symbol::GreaterGreater)     => Ok(Binary(ShiftRight)),
            Token::Symbol(Symbol::Greater)            => Ok(Binary(Greater)),
            Token::Symbol(Symbol::GreaterEqual)       => Ok(Binary(GreaterEqual)),
            Token::Symbol(Symbol::Less)               => Ok(Binary(Less)),
            Token::Symbol(Symbol::LessEqual)          => Ok(Binary(LessEqual)),
            Token::Keyword(Keyword::As)               => Ok(Binary(Cast)),
            Token::Symbol(Symbol::LeftParen)          => Ok(FunctionCall),
            Token::Symbol(Symbol::LeftBracket)        => Ok(ArrayIndex),
            Token::Symbol(Symbol::Dot)                => Ok(Access),
            Token::Symbol(Symbol::Exclam)             => Ok(Unwrap),
            Token::Symbol(Symbol::DoubleColon)        => Ok(TypeApplication),
            _ => Err(()),
        }
    }
}

impl InfixOperator {
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
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::BitOr | BinaryOp::BitXor => 12,
                BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Modulo
                | BinaryOp::BitAnd
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight => 14,
                BinaryOp::Cast => 16,
            },
            InfixOperator::FunctionCall
            | InfixOperator::ArrayIndex
            | InfixOperator::Access
            | InfixOperator::Unwrap
            | InfixOperator::TypeApplication => 202,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            InfixOperator::Binary(op) => match op {
                BinaryOp::Cast
                | BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Modulo
                | BinaryOp::Equality
                | BinaryOp::NotEquality
                | BinaryOp::Greater
                | BinaryOp::Less
                | BinaryOp::GreaterEqual
                | BinaryOp::LessEqual
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight => true,
                BinaryOp::Assign => false,
            },
            InfixOperator::FunctionCall
            | InfixOperator::ArrayIndex
            | InfixOperator::Access
            | InfixOperator::Unwrap
            | InfixOperator::TypeApplication => true,
        }
    }

    pub fn get_binding_power(&self) -> u32 {
        self.get_binding_power_real() - !self.is_left_associative() as u32
    }
}
