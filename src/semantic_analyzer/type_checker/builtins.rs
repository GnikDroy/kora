use crate::parser::{BinaryOp, Type, UnaryOp};

/// Built-in methods on `[T]`, dispatched by receiver type like struct
/// methods but implemented by the backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayMethod {
    Len,
    Push,
    Pop,
    Insert,
    Remove,
    Slice,
    Extend,
}

pub fn array_method(elem: Type, name: &str) -> Option<(ArrayMethod, Vec<Type>, Option<Type>)> {
    let signature = match name {
        "len" => (ArrayMethod::Len, vec![], Some(Type::Int)),
        "push" => (ArrayMethod::Push, vec![elem], None),
        "pop" => (ArrayMethod::Pop, vec![], Some(elem)),
        "insert" => (ArrayMethod::Insert, vec![Type::Int, elem], None),
        "remove" => (ArrayMethod::Remove, vec![Type::Int], Some(elem)),
        "slice" => (
            ArrayMethod::Slice,
            vec![Type::Int, Type::Int],
            Some(Type::Array(Box::new(elem))),
        ),
        "extend" => (ArrayMethod::Extend, vec![Type::Array(Box::new(elem))], None),
        _ => return None,
    };
    Some(signature)
}

#[rustfmt::skip]
pub fn binary_result(left: &Type, op: &BinaryOp, right: &Type) -> Option<Type> {
    use BinaryOp::*;
    use Type::*;

    match (left, op, right) {
        (Int, Add, Int)            => Some(Int),
        (Int, Subtract, Int)       => Some(Int),
        (Int, Multiply, Int)       => Some(Int),
        (Int, Divide, Int)         => Some(Int),
        (Int, Modulo, Int)         => Some(Int),
        (Int, BitAnd, Int)         => Some(Int),
        (Int, BitOr, Int)          => Some(Int),
        (Int, BitXor, Int)         => Some(Int),
        (Int, ShiftLeft, Int)      => Some(Int),
        (Int, ShiftRight, Int)     => Some(Int),
        (Int, Equality, Int)       => Some(Bool),
        (Int, NotEquality, Int)    => Some(Bool),
        (Int, Greater, Int)        => Some(Bool),
        (Int, Less, Int)           => Some(Bool),
        (Int, GreaterEqual, Int)   => Some(Bool),
        (Int, LessEqual, Int)      => Some(Bool),

        (Real, Add, Real)          => Some(Real),
        (Real, Subtract, Real)     => Some(Real),
        (Real, Multiply, Real)     => Some(Real),
        (Real, Divide, Real)       => Some(Real),
        (Real, Equality, Real)     => Some(Bool),
        (Real, NotEquality, Real)  => Some(Bool),
        (Real, Greater, Real)      => Some(Bool),
        (Real, Less, Real)         => Some(Bool),
        (Real, GreaterEqual, Real) => Some(Bool),
        (Real, LessEqual, Real)    => Some(Bool),

        (Bool, Equality, Bool)     => Some(Bool),
        (Bool, NotEquality, Bool)  => Some(Bool),
        (Bool, And, Bool)          => Some(Bool),
        (Bool, Or, Bool)           => Some(Bool),

        (Char, Equality, Char)     => Some(Bool),
        (Char, NotEquality, Char)  => Some(Bool),
        (Char, Greater, Char)      => Some(Bool),
        (Char, Less, Char)         => Some(Bool),
        (Char, GreaterEqual, Char) => Some(Bool),
        (Char, LessEqual, Char)    => Some(Bool),

        (l @ Array(_), Equality | NotEquality, r) if l == r && is_comparable(l) => Some(Bool),
        (l @ Array(_), Add, r) if l == r => Some(l.clone()),
        _ => None,
    }
}

#[rustfmt::skip]
pub fn unary_result(op: &UnaryOp, operand: &Type) -> Option<Type> {
    use Type::*;
    use UnaryOp::*;

    match (op, operand) {
        (Negate, Int)  => Some(Int),
        (Negate, Real) => Some(Real),
        (Not, Bool)    => Some(Bool),
        _ => None,
    }
}

pub fn copy_result(arg: &Type) -> Option<Type> {
    match arg {
        Type::Array(_) | Type::Struct(_) => Some(arg.clone()),
        _ => None,
    }
}

#[rustfmt::skip]
pub fn is_cast_possible(from: &Type, to: &Type) -> bool {
    use Type::*;
    matches!((from, to),
        (Int, Real)
        | (Int, Char)
        | (Real, Int)
        | (Real, Char)
        | (Char, Int)
        | (Char, Real))
}

pub fn is_comparable(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Real | Type::Bool | Type::Char => true,
        Type::Array(inner) | Type::Optional(inner) => is_comparable(inner),
        Type::Struct(_) | Type::Function(_, _) => false,
    }
}

pub fn is_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::Real | Type::Bool | Type::Char)
}

pub fn is_optional(ty: &Type) -> bool {
    matches!(ty, Type::Optional(_))
}

/// The inner type of an optional, or the type itself when not optional.
pub fn strip_optional(ty: &Type) -> &Type {
    match ty {
        Type::Optional(inner) => inner,
        _ => ty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{NodeId, Span, Spanned};

    fn struct_ty(name: &str) -> Type {
        Type::Struct(Spanned::new(
            name.to_string(),
            Span::default(),
            NodeId::default(),
        ))
    }

    fn array(inner: Type) -> Type {
        Type::Array(Box::new(inner))
    }

    #[test]
    fn test_binary_result_on_arrays() {
        use BinaryOp::*;
        use Type::*;

        let ints = array(Int);
        assert_eq!(binary_result(&ints, &Equality, &array(Int)), Some(Bool));
        assert_eq!(binary_result(&ints, &NotEquality, &array(Int)), Some(Bool));
        assert_eq!(
            binary_result(&array(array(Int)), &Equality, &array(array(Int))),
            Some(Bool)
        );
        assert_eq!(binary_result(&ints, &Add, &array(Int)), Some(array(Int)));
        assert_eq!(binary_result(&ints, &Equality, &array(Real)), None);
        assert_eq!(binary_result(&ints, &Add, &array(Real)), None);
        assert_eq!(binary_result(&ints, &Less, &array(Int)), None);
        assert_eq!(
            binary_result(&array(struct_ty("P")), &Equality, &array(struct_ty("P"))),
            None
        );
    }

    #[test]
    fn test_is_comparable_recurses_through_arrays() {
        use Type::*;

        assert!(is_comparable(&array(array(Int))));
        assert!(is_comparable(&array(array(Char))));
        assert!(!is_comparable(&array(struct_ty("P"))));
        assert!(!is_comparable(&array(Function(None, vec![]))));
    }
}
