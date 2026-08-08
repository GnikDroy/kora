use std::collections::HashMap;

use super::errors::TypeErr;
use super::symbol_resolver::{SymbolId, SymbolTable};
use super::type_checker::{binary_result, is_cast_possible, is_scalar, unary_result};
use crate::parser::*;

#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Real(f64),
    Bool(bool),
    Char(u8),
    Str(String),
}

impl ConstValue {
    pub fn get_type(&self) -> Type {
        match self {
            ConstValue::Int(_) => Type::Int,
            ConstValue::Real(_) => Type::Real,
            ConstValue::Bool(_) => Type::Bool,
            ConstValue::Char(_) => Type::Char,
            ConstValue::Str(_) => Type::Array(Box::new(Type::Char)),
        }
    }
}

pub fn evaluate_constants(
    symbols: &mut SymbolTable,
    modules: &[&Module],
) -> Result<HashMap<SymbolId, ConstValue>, Vec<TypeErr>> {
    let mut eval = ConstEvaluator {
        symbols,
        values: HashMap::new(),
        errors: Vec::new(),
    };
    for module in modules {
        eval.run_module(module);
    }
    if eval.errors.is_empty() {
        Ok(eval.values)
    } else {
        Err(eval.errors)
    }
}

struct ConstEvaluator<'a> {
    symbols: &'a mut SymbolTable,
    values: HashMap<SymbolId, ConstValue>,
    errors: Vec<TypeErr>,
}

impl ConstEvaluator<'_> {
    fn run_module(&mut self, module: &Module) {
        let mut consts: HashMap<String, SymbolId> = HashMap::new();
        for global in module.globals.iter() {
            let Some(id) = self.symbols.symbol_id_of_declaration(global.id) else {
                continue;
            };
            let value = self.fold(&consts, &global.node.value);
            match (&global.node.typename, &value) {
                (Some(annotation), Some(v)) => {
                    if *annotation != v.get_type() {
                        self.errors.push(TypeErr {
                            msg: "Types don't match",
                            span: global.node.value.span.clone(),
                        });
                    }
                }
                (None, Some(v)) => self.symbols.set_symbol_type(id, v.get_type()),
                _ => {}
            }
            if let Some(v) = value {
                self.values.insert(id, v);
            }
            consts.insert(global.node.name.node.clone(), id);
        }
    }

    fn fold(
        &mut self,
        consts: &HashMap<String, SymbolId>,
        expr: &Spanned<Expression>,
    ) -> Option<ConstValue> {
        use ConstValue::*;
        match &expr.node {
            Expression::IntegerLiteral(v) => Some(Int(*v as i64)),
            Expression::RealLiteral(v) => Some(Real(*v)),
            Expression::BoolLiteral(v) => Some(Bool(*v)),
            Expression::CharLiteral(v) => Some(Char(*v)),
            Expression::StringLiteral(v) => Some(Str(v.clone())),
            Expression::Identifier(name) => {
                let Some(&symbol) = consts.get(name) else {
                    self.errors.push(TypeErr {
                        msg: "A constant can only reference a constant declared above it in \
                              the same module",
                        span: expr.span.clone(),
                    });
                    return None;
                };
                self.symbols.uses.insert(expr.id, symbol);
                self.values.get(&symbol).cloned()
            }
            Expression::Unary(op, inner) => {
                let value = self.fold(consts, inner)?;
                if unary_result(op, &value.get_type()).is_none() {
                    self.errors.push(TypeErr {
                        msg: "Unary operator cannot be applied to the type",
                        span: expr.span.clone(),
                    });
                    return None;
                }
                Some(match (op, value) {
                    (UnaryOp::Negate, Int(v)) => Int(v.wrapping_neg()),
                    (UnaryOp::Negate, Real(v)) => Real(-v),
                    (UnaryOp::Not, Bool(v)) => Bool(!v),
                    _ => unreachable!("unary_result vetted the operand"),
                })
            }
            Expression::Binary(_, BinaryOp::Assign, _) => self.not_constant(expr),
            Expression::Binary(left, op, right) => {
                let l = self.fold(consts, left)?;
                let r = self.fold(consts, right)?;
                if binary_result(&l.get_type(), op, &r.get_type()).is_none() {
                    self.errors.push(TypeErr {
                        msg: "Binary operator cannot be applied to the types",
                        span: expr.span.clone(),
                    });
                    return None;
                }
                self.fold_binary(l, *op, r, &expr.span)
            }
            Expression::Cast(inner, target) => {
                let value = self.fold(consts, inner)?;
                if !is_scalar(target) || !is_cast_possible(&value.get_type(), target) {
                    self.errors.push(TypeErr {
                        msg: "The cast cannot be applied in a constant expression",
                        span: expr.span.clone(),
                    });
                    return None;
                }
                Some(match (value, target) {
                    (Int(v), Type::Real) => Real(v as f64),
                    (Int(v), Type::Char) => Char(v as u8),
                    (Real(v), Type::Int) => Int(v as i64),
                    (Real(v), Type::Char) => Char(v as u8),
                    (Char(v), Type::Int) => Int(v as i64),
                    (Char(v), Type::Real) => Real(v as f64),
                    (same, _) => same, // identity cast
                })
            }
            _ => self.not_constant(expr),
        }
    }

    fn not_constant(&mut self, expr: &Spanned<Expression>) -> Option<ConstValue> {
        self.errors.push(TypeErr {
            msg: "A module-level let must be initialized with a constant expression: \
                  literals, earlier constants, and operators over them",
            span: expr.span.clone(),
        });
        None
    }

    fn fold_binary(
        &mut self,
        l: ConstValue,
        op: BinaryOp,
        r: ConstValue,
        span: &Span,
    ) -> Option<ConstValue> {
        use BinaryOp::*;
        use ConstValue::*;
        if matches!((&l, op, &r), (Int(_), Divide | Modulo, Int(0))) {
            self.errors.push(TypeErr {
                msg: "Division by zero in a constant expression",
                span: span.clone(),
            });
            return None;
        }

        Some(match (l, op, r) {
            (Int(a), Add, Int(b)) => Int(a.wrapping_add(b)),
            (Int(a), Subtract, Int(b)) => Int(a.wrapping_sub(b)),
            (Int(a), Multiply, Int(b)) => Int(a.wrapping_mul(b)),
            (Int(a), Divide, Int(b)) => Int(a.wrapping_div(b)),
            (Int(a), Modulo, Int(b)) => Int(a.wrapping_rem(b)),
            (Int(a), BitAnd, Int(b)) => Int(a & b),
            (Int(a), BitOr, Int(b)) => Int(a | b),
            (Int(a), BitXor, Int(b)) => Int(a ^ b),
            (Int(a), ShiftLeft, Int(b)) => Int(a.wrapping_shl(b as u32)),
            (Int(a), ShiftRight, Int(b)) => Int(a.wrapping_shr(b as u32)),
            (Int(a), Equality, Int(b)) => Bool(a == b),
            (Int(a), NotEquality, Int(b)) => Bool(a != b),
            (Int(a), Greater, Int(b)) => Bool(a > b),
            (Int(a), Less, Int(b)) => Bool(a < b),
            (Int(a), GreaterEqual, Int(b)) => Bool(a >= b),
            (Int(a), LessEqual, Int(b)) => Bool(a <= b),

            (Real(a), Add, Real(b)) => Real(a + b),
            (Real(a), Subtract, Real(b)) => Real(a - b),
            (Real(a), Multiply, Real(b)) => Real(a * b),
            (Real(a), Divide, Real(b)) => Real(a / b),
            (Real(a), Equality, Real(b)) => Bool(a == b),
            (Real(a), NotEquality, Real(b)) => Bool(a != b),
            (Real(a), Greater, Real(b)) => Bool(a > b),
            (Real(a), Less, Real(b)) => Bool(a < b),
            (Real(a), GreaterEqual, Real(b)) => Bool(a >= b),
            (Real(a), LessEqual, Real(b)) => Bool(a <= b),

            (Bool(a), And, Bool(b)) => Bool(a && b),
            (Bool(a), Or, Bool(b)) => Bool(a || b),
            (Bool(a), Equality, Bool(b)) => Bool(a == b),
            (Bool(a), NotEquality, Bool(b)) => Bool(a != b),

            (Char(a), Equality, Char(b)) => Bool(a == b),
            (Char(a), NotEquality, Char(b)) => Bool(a != b),
            (Char(a), Greater, Char(b)) => Bool(a > b),
            (Char(a), Less, Char(b)) => Bool(a < b),
            (Char(a), GreaterEqual, Char(b)) => Bool(a >= b),
            (Char(a), LessEqual, Char(b)) => Bool(a <= b),

            (Str(a), Add, Str(b)) => Str(a + &b),
            (Str(a), Equality, Str(b)) => Bool(a == b),
            (Str(a), NotEquality, Str(b)) => Bool(a != b),

            _ => unreachable!("binary_result vetted the operands"),
        })
    }
}
