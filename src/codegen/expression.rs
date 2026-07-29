use inkwell::values::{BasicValueEnum, IntValue, PointerValue, ValueKind};
use inkwell::{FloatPredicate, IntPredicate};

use super::{CodeGen, CodegenErr};
use crate::parser::*;
use crate::semantic_analyzer::is_intrinsic;

impl<'ctx> CodeGen<'ctx, '_> {
    pub(super) fn lower_expression(
        &mut self,
        expr: &Spanned<Expression>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let span = &expr.span;
        match &expr.node {
            Expression::IntegerLiteral(v) => {
                Ok(self.context.i64_type().const_int(*v as u64, true).into())
            }
            Expression::RealLiteral(v) => Ok(self.context.f64_type().const_float(*v).into()),
            Expression::BoolLiteral(v) => {
                Ok(self.context.bool_type().const_int(*v as u64, false).into())
            }
            Expression::CharLiteral(v) => {
                Ok(self.context.i8_type().const_int(*v as u64, false).into())
            }
            Expression::Identifier(name) => {
                let id = self.program.symbols.symbol_id_of_use(expr.id).unwrap();
                let alloca = self.frame().variables.get(&id).ok_or(CodegenErr {
                    msg: "functions cannot be used as values",
                    span: span.clone(),
                })?;
                let ty = self.basic_type(&self.program.types[&expr.id], span)?;
                Ok(self.builder.build_load(ty, *alloca, name).unwrap())
            }
            Expression::Binary(left, BinaryOp::Assign, right) => {
                let target = self.lower_lvalue(left)?;
                let value = self.lower_expression(right)?;
                self.builder.build_store(target, value).unwrap();
                Ok(value)
            }
            Expression::Binary(left, op @ (BinaryOp::And | BinaryOp::Or), right) => {
                self.lower_short_circuit(left, *op, right)
            }
            Expression::Binary(left, op, right) => self.lower_binary(left, *op, right, span),
            Expression::Unary(op, operand) => {
                let value = self.lower_expression(operand)?;
                let result: BasicValueEnum = match (op, &self.program.types[&operand.id]) {
                    (UnaryOp::Negate, Type::Int) => self
                        .builder
                        .build_int_neg(value.into_int_value(), "neg")
                        .unwrap()
                        .into(),
                    (UnaryOp::Negate, Type::Real) => self
                        .builder
                        .build_float_neg(value.into_float_value(), "neg")
                        .unwrap()
                        .into(),
                    (UnaryOp::Not, Type::Bool) => self
                        .builder
                        .build_not(value.into_int_value(), "not")
                        .unwrap()
                        .into(),
                    _ => unreachable!("type checker rejects other unary operands"),
                };
                Ok(result)
            }
            Expression::Cast(operand, target) => self.lower_cast(operand, target),
            Expression::Call(callee, args) => Ok(self
                .lower_call(callee, args, span)?
                .expect("type checker rejects void calls in value position")),
            Expression::StringLiteral(_) => Err(CodegenErr {
                msg: "codegen for strings is not implemented yet",
                span: span.clone(),
            }),
            Expression::Array(_) | Expression::ArrayIndex(_, _) => Err(CodegenErr {
                msg: "codegen for arrays is not implemented yet",
                span: span.clone(),
            }),
            Expression::NoneLiteral | Expression::Unwrap(_) => Err(CodegenErr {
                msg: "codegen for optionals is not implemented yet",
                span: span.clone(),
            }),
            Expression::Access(obj, member) => {
                let ptr = self.struct_member_pointer(obj, member, span)?;
                let ty = self.basic_type(&self.program.types[&expr.id], span)?;
                Ok(self.builder.build_load(ty, ptr, member).unwrap())
            }
            Expression::StructLiteral(typename, fields) => {
                let Type::Struct(name) = typename else {
                    unreachable!("struct literals always carry a struct type");
                };
                let struct_type = self.struct_type(&name.node, span)?;
                let object = self.gc_malloc(struct_type.size_of().unwrap(), "new");
                for (field, value) in fields.iter() {
                    let value = self.lower_expression(value)?;
                    let index = self.struct_member_index(&name.node, &field.node);
                    let ptr = self
                        .builder
                        .build_struct_gep(struct_type, object, index, &field.node)
                        .unwrap();
                    self.builder.build_store(ptr, value).unwrap();
                }
                Ok(object.into())
            }
            Expression::Construct(typename, size) => match (typename, size) {
                (_, Some(_)) => Err(CodegenErr {
                    msg: "codegen for arrays is not implemented yet",
                    span: span.clone(),
                }),
                (Type::Struct(name), None) => {
                    let default = self.struct_constructor(&name.node, span)?;
                    let call = self.builder.build_call(default, &[], "new").unwrap();
                    match call.try_as_basic_value() {
                        ValueKind::Basic(value) => Ok(value),
                        ValueKind::Instruction(_) => unreachable!(),
                    }
                }
                _ => unreachable!("type checker rejects bare `new` on non-structs"),
            },
        }
    }

    /// An expression in statement position, where a void call is legal.
    pub(super) fn lower_expression_or_void(
        &mut self,
        expr: &Spanned<Expression>,
    ) -> Result<(), CodegenErr> {
        match &expr.node {
            Expression::Call(callee, args) => {
                self.lower_call(callee, args, &expr.span)?;
                Ok(())
            }
            _ => self.lower_expression(expr).map(|_| ()),
        }
    }

    fn lower_lvalue(
        &mut self,
        expr: &Spanned<Expression>,
    ) -> Result<PointerValue<'ctx>, CodegenErr> {
        match &expr.node {
            Expression::Identifier(_) => {
                let id = self.program.symbols.symbol_id_of_use(expr.id).unwrap();
                Ok(self.frame().variables[&id])
            }
            Expression::ArrayIndex(_, _) => Err(CodegenErr {
                msg: "codegen for arrays is not implemented yet",
                span: expr.span.clone(),
            }),
            Expression::Access(obj, member) => self.struct_member_pointer(obj, member, &expr.span),
            _ => unreachable!("type checker rejects other assignment targets"),
        }
    }

    fn lower_binary(
        &mut self,
        left: &Spanned<Expression>,
        op: BinaryOp,
        right: &Spanned<Expression>,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        use BinaryOp::*;

        let operand_type = self.program.types[&left.id].clone();
        let lhs = self.lower_expression(left)?;
        let rhs = self.lower_expression(right)?;

        let value: BasicValueEnum = match operand_type {
            Type::Int | Type::Char | Type::Bool => {
                let l = lhs.into_int_value();
                let r = rhs.into_int_value();
                if matches!(op, Divide | Modulo) {
                    self.check_nonzero_divisor(r);
                }
                // Char is unsigned; Int is signed. Bool only reaches ==/!=.
                let signed = operand_type == Type::Int;
                let b = &self.builder;
                #[rustfmt::skip]
                let result = match (op, signed) {
                    (Add, _)      => b.build_int_add(l, r, "add").unwrap(),
                    (Subtract, _) => b.build_int_sub(l, r, "sub").unwrap(),
                    (Multiply, _) => b.build_int_mul(l, r, "mul").unwrap(),
                    (Divide, _)   => b.build_int_signed_div(l, r, "div").unwrap(),
                    (Modulo, _)   => b.build_int_signed_rem(l, r, "rem").unwrap(),
                    (BitAnd, _)     => b.build_and(l, r, "and").unwrap(),
                    (BitOr, _)      => b.build_or(l, r, "or").unwrap(),
                    (BitXor, _)     => b.build_xor(l, r, "xor").unwrap(),
                    (ShiftLeft, _)  => b.build_left_shift(l, r, "shl").unwrap(),
                    (ShiftRight, _) => b.build_right_shift(l, r, true, "shr").unwrap(),
                    (Equality, _)     => b.build_int_compare(IntPredicate::EQ, l, r, "eq").unwrap(),
                    (NotEquality, _)  => b.build_int_compare(IntPredicate::NE, l, r, "ne").unwrap(),
                    (Greater, true)       => b.build_int_compare(IntPredicate::SGT, l, r, "gt").unwrap(),
                    (Greater, false)      => b.build_int_compare(IntPredicate::UGT, l, r, "gt").unwrap(),
                    (Less, true)          => b.build_int_compare(IntPredicate::SLT, l, r, "lt").unwrap(),
                    (Less, false)         => b.build_int_compare(IntPredicate::ULT, l, r, "lt").unwrap(),
                    (GreaterEqual, true)  => b.build_int_compare(IntPredicate::SGE, l, r, "ge").unwrap(),
                    (GreaterEqual, false) => b.build_int_compare(IntPredicate::UGE, l, r, "ge").unwrap(),
                    (LessEqual, true)     => b.build_int_compare(IntPredicate::SLE, l, r, "le").unwrap(),
                    (LessEqual, false)    => b.build_int_compare(IntPredicate::ULE, l, r, "le").unwrap(),
                    _ => unreachable!("type checker rejects other int operators"),
                };
                result.into()
            }
            Type::Real => {
                let l = lhs.into_float_value();
                let r = rhs.into_float_value();
                let b = &self.builder;
                #[rustfmt::skip]
                let result: BasicValueEnum = match op {
                    Add      => b.build_float_add(l, r, "add").unwrap().into(),
                    Subtract => b.build_float_sub(l, r, "sub").unwrap().into(),
                    Multiply => b.build_float_mul(l, r, "mul").unwrap().into(),
                    Divide   => b.build_float_div(l, r, "div").unwrap().into(),
                    Equality     => b.build_float_compare(FloatPredicate::OEQ, l, r, "eq").unwrap().into(),
                    NotEquality  => b.build_float_compare(FloatPredicate::ONE, l, r, "ne").unwrap().into(),
                    Greater      => b.build_float_compare(FloatPredicate::OGT, l, r, "gt").unwrap().into(),
                    Less         => b.build_float_compare(FloatPredicate::OLT, l, r, "lt").unwrap().into(),
                    GreaterEqual => b.build_float_compare(FloatPredicate::OGE, l, r, "ge").unwrap().into(),
                    LessEqual    => b.build_float_compare(FloatPredicate::OLE, l, r, "le").unwrap().into(),
                    _ => unreachable!("type checker rejects other real operators"),
                };
                result
            }
            _ => {
                return Err(CodegenErr {
                    msg: "codegen for operators on this type is not implemented yet",
                    span: span.clone(),
                });
            }
        };
        Ok(value)
    }

    fn lower_short_circuit(
        &mut self,
        left: &Spanned<Expression>,
        op: BinaryOp,
        right: &Spanned<Expression>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let function = self.frame().function;
        let lhs = self.lower_expression(left)?.into_int_value();
        let lhs_block = self.builder.get_insert_block().unwrap();

        let rhs_block = self.context.append_basic_block(function, "sc_rhs");
        let merge_block = self.context.append_basic_block(function, "sc_merge");

        let short_value = match op {
            // false && _ == false; true || _ == true
            BinaryOp::And => {
                self.builder
                    .build_conditional_branch(lhs, rhs_block, merge_block)
                    .unwrap();
                self.context.bool_type().const_int(0, false)
            }
            BinaryOp::Or => {
                self.builder
                    .build_conditional_branch(lhs, merge_block, rhs_block)
                    .unwrap();
                self.context.bool_type().const_int(1, false)
            }
            _ => unreachable!(),
        };

        self.builder.position_at_end(rhs_block);
        let rhs = self.lower_expression(right)?.into_int_value();
        let rhs_end_block = self.builder.get_insert_block().unwrap();
        self.builder
            .build_unconditional_branch(merge_block)
            .unwrap();

        self.builder.position_at_end(merge_block);
        let phi = self
            .builder
            .build_phi(self.context.bool_type(), "sc")
            .unwrap();
        phi.add_incoming(&[(&short_value, lhs_block), (&rhs, rhs_end_block)]);
        Ok(phi.as_basic_value())
    }

    fn lower_cast(
        &mut self,
        operand: &Spanned<Expression>,
        target: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        use Type::*;
        let from = self.program.types[&operand.id].clone();
        let value = self.lower_expression(operand)?;
        let b = &self.builder;

        let result: BasicValueEnum = match (from, target) {
            (Int, Real) => b
                .build_signed_int_to_float(value.into_int_value(), self.context.f64_type(), "cast")
                .unwrap()
                .into(),
            (Real, Int) => b
                .build_float_to_signed_int(
                    value.into_float_value(),
                    self.context.i64_type(),
                    "cast",
                )
                .unwrap()
                .into(),
            (Int, Char) => b
                .build_int_truncate(value.into_int_value(), self.context.i8_type(), "cast")
                .unwrap()
                .into(),
            (Char, Int) => b
                .build_int_z_extend(value.into_int_value(), self.context.i64_type(), "cast")
                .unwrap()
                .into(),
            (Char, Real) => b
                .build_unsigned_int_to_float(
                    value.into_int_value(),
                    self.context.f64_type(),
                    "cast",
                )
                .unwrap()
                .into(),
            (Real, Char) => b
                .build_float_to_unsigned_int(
                    value.into_float_value(),
                    self.context.i8_type(),
                    "cast",
                )
                .unwrap()
                .into(),
            _ => unreachable!("type checker rejects other casts"),
        };
        Ok(result)
    }

    fn lower_call(
        &mut self,
        callee: &Spanned<Expression>,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodegenErr> {
        let (function, receiver) = match &callee.node {
            Expression::Identifier(name) => {
                if is_intrinsic(name) {
                    return self.lower_copy(&args[0], span).map(Some);
                }
                let id = self.program.symbols.symbol_id_of_use(callee.id).unwrap();
                (self.functions[&id], None)
            }
            Expression::Access(obj, _) => {
                if self.program.array_method_calls.contains_key(&callee.id) {
                    return Err(CodegenErr {
                        msg: "codegen for arrays is not implemented yet",
                        span: span.clone(),
                    });
                }
                let method = self.program.method_calls[&callee.id];
                (self.functions[&method], Some(obj))
            }
            _ => unreachable!("type checker rejects other callees"),
        };

        // A method's receiver is its first argument.
        let mut arg_values = Vec::with_capacity(args.len() + 1);
        if let Some(obj) = receiver {
            arg_values.push(self.lower_expression(obj)?.into());
        }
        for arg in args.iter() {
            arg_values.push(self.lower_expression(arg)?.into());
        }

        let call = self.builder.build_call(function, &arg_values, "").unwrap();
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Ok(Some(value)),
            ValueKind::Instruction(_) => Ok(None),
        }
    }

    /// `copy(x)` — universal shallow copy of an aggregate.
    fn lower_copy(
        &mut self,
        arg: &Spanned<Expression>,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        match self.program.types[&arg.id].clone() {
            Type::Struct(name) => {
                let struct_type = self.struct_type(&name.node, span)?;
                let size = struct_type.size_of().unwrap();
                let source = self.lower_expression(arg)?.into_pointer_value();
                let copy = self.gc_malloc(size, "copy");
                self.builder.build_memcpy(copy, 1, source, 1, size).unwrap();
                Ok(copy.into())
            }
            Type::Array(_) => Err(CodegenErr {
                msg: "codegen for arrays is not implemented yet",
                span: span.clone(),
            }),
            _ => unreachable!("type checker rejects copy of scalars"),
        }
    }

    fn check_nonzero_divisor(&mut self, divisor: IntValue<'ctx>) {
        let function = self.frame().function;
        let is_zero = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                divisor,
                divisor.get_type().const_zero(),
                "is_zero",
            )
            .unwrap();
        let panic_block = self.context.append_basic_block(function, "div_by_zero");
        let cont_block = self.context.append_basic_block(function, "div_cont");
        self.builder
            .build_conditional_branch(is_zero, panic_block, cont_block)
            .unwrap();
        self.builder.position_at_end(panic_block);
        self.build_panic("division by zero");
        self.builder.position_at_end(cont_block);
    }
}
