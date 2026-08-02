mod lower;

#[allow(unused_imports)]
pub(crate) use lower::lower;

use std::collections::HashMap;
use std::ops::Index;

use crate::parser::{ExternType, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Int,
    Real,
    Bool,
    Char,
    Opaque,
    Array(TypeId),
    Optional(TypeId),
    Struct(StructId),
}

#[derive(Debug, Default)]
pub struct Types {
    tys: Vec<Type>,
    ids: HashMap<Type, TypeId>,
}

impl Types {
    pub const VOID: TypeId = TypeId(0);
    pub const INT: TypeId = TypeId(1);
    pub const REAL: TypeId = TypeId(2);
    pub const BOOL: TypeId = TypeId(3);
    pub const CHAR: TypeId = TypeId(4);
    pub const OPAQUE: TypeId = TypeId(5);

    pub fn new() -> Types {
        let mut types = Types::default();
        for ty in [
            Type::Void,
            Type::Int,
            Type::Real,
            Type::Bool,
            Type::Char,
            Type::Opaque,
        ] {
            types.intern(ty);
        }
        types
    }

    pub fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(&id) = self.ids.get(&ty) {
            return id;
        }
        let id = TypeId(self.tys.len() as u32);
        self.tys.push(ty);
        self.ids.insert(ty, id);
        id
    }
}

impl Index<TypeId> for Types {
    type Output = Type;

    fn index(&self, id: TypeId) -> &Type {
        &self.tys[id.0 as usize]
    }
}

#[derive(Debug)]
pub struct Program {
    pub types: Types,
    pub structs: Vec<StructDef>,
    pub externs: Vec<ExternDef>,
    pub functions: Vec<FunctionDef>,
    pub entry: Option<FunctionId>,
}

impl Index<StructId> for Program {
    type Output = StructDef;

    fn index(&self, id: StructId) -> &StructDef {
        &self.structs[id.0 as usize]
    }
}

impl Index<FunctionId> for Program {
    type Output = FunctionDef;

    fn index(&self, id: FunctionId) -> &FunctionDef {
        &self.functions[id.0 as usize]
    }
}

impl Index<ExternId> for Program {
    type Output = ExternDef;

    fn index(&self, id: ExternId) -> &ExternDef {
        &self.externs[id.0 as usize]
    }
}

#[derive(Debug)]
pub struct StructDef {
    /// The emitted base name: the LLVM type name, the default.{} suffix,
    /// and the method-mangling base.
    pub symbol: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeId,
}

#[derive(Debug)]
pub struct FunctionDef {
    /// The final emitted symbol, __kora_main and prefixes included.
    pub symbol: String,
    /// NOTE: Parameters are locals[0..params]
    pub params: usize,
    pub ret: TypeId,
    pub locals: Vec<Local>,
    pub body: Block,
}

#[derive(Debug)]
pub struct Local {
    pub name: String,
    pub ty: TypeId,
}

#[derive(Debug)]
pub struct ExternDef {
    /// Externs keep their raw source name as the symbol.
    pub symbol: String,
    pub params: Vec<ExternType>,
    pub ret: Option<ExternType>,
}

pub type Block = Vec<Statement>;

#[derive(Debug)]
pub enum Statement {
    Let(LocalId, Expression),
    Expression(Expression),
    Return(Option<Expression>),
    Break,
    Continue,
    While {
        cond: Expression,
        body: Block,
    },
    For {
        init: Block,
        cond: Expression,
        step: Expression,
        body: Block,
    },
    If {
        cond: Expression,
        then: Block,
        otherwise: Option<Block>,
    },
}

#[derive(Debug)]
pub struct Expression {
    pub ty: TypeId,
    pub span: Span,
    pub kind: ExpressionKind,
}

impl Expression {
    pub fn new(ty: TypeId, span: Span, kind: ExpressionKind) -> Expression {
        Expression { ty, span, kind }
    }
}

#[derive(Debug)]
pub enum ExpressionKind {
    Int(i64),
    Real(f64),
    Bool(bool),
    Char(u8),
    Str(String),
    Array(Vec<Expression>),
    None,

    Local(LocalId),
    Field {
        object: Box<Expression>,
        index: u32,
    },
    Index {
        array: Box<Expression>,
        index: Box<Expression>,
    },
    /// Place are the assignable subset of expressions; evaluates to the assigned value.
    Assign {
        place: Place,
        value: Box<Expression>,
    },

    Binary {
        op: BinOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// Short-circuit, so not a BinOp: the right operand may not evaluate.
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Unary {
        op: UnOp,
        operand: Box<Expression>,
    },
    Cast {
        kind: CastKind,
        operand: Box<Expression>,
    },
    Call {
        function: FunctionId,
        args: Vec<Expression>,
    },
    CallExtern {
        function: ExternId,
        args: Vec<Expression>,
    },
    ArrayOp {
        op: ArrayOp,
        receiver: Box<Expression>,
        args: Vec<Expression>,
    },
    Copy(Box<Expression>),
    /// Fields in declaration order.
    StructLit {
        struct_: StructId,
        fields: Vec<Expression>,
    },
    DefaultStruct(StructId),
    ArrayNew {
        len: Box<Expression>,
    },
    Wrap(Box<Expression>),
    Unwrap(Box<Expression>),
}

#[derive(Debug)]
pub struct Place {
    pub ty: TypeId,
    pub span: Span,
    pub kind: PlaceKind,
}

#[derive(Debug)]
pub enum PlaceKind {
    Local(LocalId),
    Field {
        object: Box<Place>,
        index: u32,
    },
    Index {
        array: Box<Place>,
        index: Box<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    IntAdd,
    IntSub,
    IntMul,
    IntDiv,
    IntMod,
    IntBitAnd,
    IntBitOr,
    IntBitXor,
    IntShl,
    IntShr,
    IntEq,
    IntNe,
    IntLt,
    IntLe,
    IntGt,
    IntGe,

    RealAdd,
    RealSub,
    RealMul,
    RealDiv,
    RealEq,
    RealNe,
    RealLt,
    RealLe,
    RealGt,
    RealGe,

    CharEq,
    CharNe,
    CharLt,
    CharLe,
    CharGt,
    CharGe,

    BoolEq,
    BoolNe,

    OpaqueEq,
    OpaqueNe,

    /// Structural, element-wise.
    ArrayEq,
    ArrayNe,
    ArrayConcat,

    /// Optionals are guaranteed to have same types, or both are none.
    OptionalEq,
    OptionalNe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    IntNeg,
    RealNeg,
    BoolNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    IntToReal,
    IntToChar,
    RealToInt,
    RealToChar,
    CharToInt,
    CharToReal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayOp {
    Len,
    Push,
    Pop,
    Insert,
    Remove,
    Slice,
    Extend,
}
