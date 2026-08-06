use inkwell::types::BasicType;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind};
use inkwell::{FloatPredicate, IntPredicate};

use super::CodeGen;
use crate::ir::{BinOp, CastKind, Expression, ExpressionKind, ExternId, Place, PlaceKind, Type, UnOp};
use crate::parser::ExternType;

impl<'ctx, 'a> CodeGen<'ctx, 'a> {
    pub(super) fn lower_expression(&mut self, expr: &Expression) -> BasicValueEnum<'ctx> {
        match &expr.kind {
            ExpressionKind::Int(v) => self.context.i64_type().const_int(*v as u64, true).into(),
            ExpressionKind::Real(v) => self.context.f64_type().const_float(*v).into(),
            ExpressionKind::Bool(v) => self.context.bool_type().const_int(*v as u64, false).into(),
            ExpressionKind::Char(v) => self.context.i8_type().const_int(*v as u64, false).into(),
            ExpressionKind::Str(s) => self.lower_string_literal(s),
            ExpressionKind::None => self.basic_type(expr.ty).const_zero(),
            ExpressionKind::Array(items) => self.lower_array_literal(expr.ty, items),
            ExpressionKind::Local(local) => {
                let ptr = self.frame().variables[local.index()];
                let ty = self.basic_type(expr.ty);
                self.builder.build_load(ty, ptr, "load").unwrap()
            }
            ExpressionKind::Field { object, index } => {
                let ptr = self.struct_field_ptr(object, *index);
                let ty = self.basic_type(expr.ty);
                self.builder.build_load(ty, ptr, "field").unwrap()
            }
            ExpressionKind::Index { array, index } => {
                let base = self.lower_expression(array).into_pointer_value();
                let idx = self.lower_expression(index).into_int_value();
                let ptr = self.array_element_ptr(base, array.ty, idx);
                let ty = self.basic_type(expr.ty);
                self.builder.build_load(ty, ptr, "elem").unwrap()
            }
            ExpressionKind::Assign { place, value } => {
                let ptr = self.lower_lvalue(place);
                let value = self.lower_expression(value);
                self.builder.build_store(ptr, value).unwrap();
                value
            }
            ExpressionKind::Binary { op, left, right } => self.lower_binary(*op, left, right),
            ExpressionKind::And(left, right) => self.lower_short_circuit(left, true, right),
            ExpressionKind::Or(left, right) => self.lower_short_circuit(left, false, right),
            ExpressionKind::Unary { op, operand } => {
                let value = self.lower_expression(operand);
                match op {
                    UnOp::IntNeg => self
                        .builder
                        .build_int_neg(value.into_int_value(), "neg")
                        .unwrap()
                        .into(),
                    UnOp::RealNeg => self
                        .builder
                        .build_float_neg(value.into_float_value(), "neg")
                        .unwrap()
                        .into(),
                    UnOp::BoolNot => self
                        .builder
                        .build_not(value.into_int_value(), "not")
                        .unwrap()
                        .into(),
                }
            }
            ExpressionKind::Cast { kind, operand } => self.lower_cast(*kind, operand),
            ExpressionKind::Call { function, args } => {
                let function = self.function_value(*function);
                self.call_function(function, args)
                    .expect("type checker rejects void calls in value position")
            }
            ExpressionKind::CallExtern { function, args } => self
                .call_extern(*function, args)
                .expect("type checker rejects void calls in value position"),
            ExpressionKind::ArrayOp { op, receiver, args } => self
                .lower_array_op(*op, receiver, args)
                .expect("type checker rejects void array ops in value position"),
            ExpressionKind::FnRef(function) => self
                .function_value(*function)
                .as_global_value()
                .as_pointer_value()
                .into(),
            ExpressionKind::IndirectCall { callee, args } => self
                .indirect_call(callee, expr.ty, args)
                .expect("type checker rejects void calls in value position"),
            ExpressionKind::Copy(inner) => self.lower_copy(inner),
            ExpressionKind::StructLit { struct_, fields } => {
                self.lower_struct_literal(*struct_, fields)
            }
            ExpressionKind::DefaultStruct(struct_) => {
                let default = self.struct_constructor(*struct_);
                let call = self.builder.build_call(default, &[], "new").unwrap();
                self.call_value(call)
            }
            ExpressionKind::ArrayNew { len } => self.lower_array_construct(expr.ty, len),
            ExpressionKind::Wrap(inner) => {
                let value = self.lower_expression(inner);
                self.lower_optional_wrap(value, expr.ty)
            }
            ExpressionKind::Unwrap(inner) => self.lower_unwrap(inner),
        }
    }

    /// An expression in statement position, where a void call is legal.
    pub(super) fn lower_expression_or_void(&mut self, expr: &Expression) {
        match &expr.kind {
            ExpressionKind::Call { function, args } => {
                let function = self.function_value(*function);
                self.call_function(function, args);
            }
            ExpressionKind::CallExtern { function, args } => {
                self.call_extern(*function, args);
            }
            ExpressionKind::ArrayOp { op, receiver, args } => {
                self.lower_array_op(*op, receiver, args);
            }
            ExpressionKind::IndirectCall { callee, args } => {
                self.indirect_call(callee, expr.ty, args);
            }
            _ => {
                self.lower_expression(expr);
            }
        }
    }

    fn indirect_call(
        &mut self,
        callee: &Expression,
        ret: crate::ir::TypeId,
        args: &[Expression],
    ) -> Option<BasicValueEnum<'ctx>> {
        let callee_ptr = self.lower_expression(callee).into_pointer_value();
        let param_types = args
            .iter()
            .map(|a| self.basic_type(a.ty).into())
            .collect::<Vec<_>>();
        let fn_type = match self.program.types[ret] {
            Type::Void => self.context.void_type().fn_type(&param_types, false),
            _ => self.basic_type(ret).fn_type(&param_types, false),
        };
        let mut arg_values = Vec::with_capacity(args.len());
        for arg in args {
            arg_values.push(self.lower_expression(arg).into());
        }
        let call = self
            .builder
            .build_indirect_call(fn_type, callee_ptr, &arg_values, "")
            .unwrap();
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Some(value),
            ValueKind::Instruction(_) => None,
        }
    }

    fn call_function(
        &mut self,
        function: FunctionValue<'ctx>,
        args: &[Expression],
    ) -> Option<BasicValueEnum<'ctx>> {
        let mut arg_values = Vec::with_capacity(args.len());
        for arg in args {
            arg_values.push(self.lower_expression(arg).into());
        }
        let call = self.builder.build_call(function, &arg_values, "").unwrap();
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Some(value),
            ValueKind::Instruction(_) => None,
        }
    }

    fn call_extern(&mut self, id: ExternId, args: &[Expression]) -> Option<BasicValueEnum<'ctx>> {
        let function = self.extern_value(id);
        let params = self.program[id].params.clone();
        let mut arg_values = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            let value = match params.get(i) {
                Some(ExternType::Function { params: sig, ret }) => {
                    self.lower_callback_argument(arg, sig, ret)
                }
                _ => self.lower_expression(arg),
            };
            arg_values.push(value.into());
        }
        let call = self.builder.build_call(function, &arg_values, "").unwrap();
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Some(value),
            ValueKind::Instruction(_) => None,
        }
    }

    fn lower_callback_argument(
        &mut self,
        arg: &Expression,
        sig: &[ExternType],
        ret: &Option<Box<ExternType>>,
    ) -> BasicValueEnum<'ctx> {
        let identical = sig.iter().all(ExternType::has_identical_crepr)
            && ret.as_ref().is_none_or(|t| t.has_identical_crepr());
        if identical {
            return self.lower_expression(arg);
        }
        let ExpressionKind::FnRef(target) = arg.kind else {
            unreachable!("type checker guarantees a marshalled C callback is a named function");
        };
        self.c_callback_thunk(target, sig, ret).into()
    }

    pub(super) fn lower_lvalue(&mut self, place: &Place) -> PointerValue<'ctx> {
        match &place.kind {
            PlaceKind::Local(local) => self.frame().variables[local.index()],
            PlaceKind::Field { object, index } => {
                let struct_type = self.struct_type_of(object.ty);
                let base = self.read_place(object).into_pointer_value();
                self.builder
                    .build_struct_gep(struct_type, base, *index, "field")
                    .unwrap()
            }
            PlaceKind::Index { array, index } => {
                let base = self.read_place(array).into_pointer_value();
                let idx = self.lower_expression(index).into_int_value();
                self.array_element_ptr(base, array.ty, idx)
            }
        }
    }

    fn read_place(&mut self, place: &Place) -> BasicValueEnum<'ctx> {
        let ptr = self.lower_lvalue(place);
        let ty = self.basic_type(place.ty);
        self.builder.build_load(ty, ptr, "place").unwrap()
    }

    fn lower_binary(
        &mut self,
        op: BinOp,
        left: &Expression,
        right: &Expression,
    ) -> BasicValueEnum<'ctx> {
        match op {
            BinOp::ArrayConcat => {
                let l = self.lower_expression(left).into_pointer_value();
                let r = self.lower_expression(right).into_pointer_value();
                self.array_concat(left.ty, l, r)
            }
            BinOp::ArrayEq | BinOp::ArrayNe => {
                let l = self.lower_expression(left).into_pointer_value();
                let r = self.lower_expression(right).into_pointer_value();
                self.array_equality(op, left.ty, l, r)
            }
            BinOp::OptionalEq | BinOp::OptionalNe => self.lower_optional_equality(op, left, right),
            _ => self.lower_scalar_binary(op, left, right),
        }
    }

    fn lower_scalar_binary(
        &mut self,
        op: BinOp,
        left: &Expression,
        right: &Expression,
    ) -> BasicValueEnum<'ctx> {
        use BinOp::*;
        let l = self.lower_expression(left);
        let r = self.lower_expression(right);

        match op {
            IntDiv | IntMod => {
                let l = l.into_int_value();
                let r = r.into_int_value();
                self.check_nonzero_divisor(r);
                let value = if op == IntDiv {
                    self.builder.build_int_signed_div(l, r, "div").unwrap()
                } else {
                    self.builder.build_int_signed_rem(l, r, "rem").unwrap()
                };
                value.into()
            }
            OpaqueEq | OpaqueNe => {
                let eq = self.pointers_equal(l.into_pointer_value(), r.into_pointer_value());
                if op == OpaqueNe {
                    self.builder.build_not(eq, "ne").unwrap().into()
                } else {
                    eq.into()
                }
            }
            _ => {
                let b = &self.builder;
                #[rustfmt::skip]
                let value: BasicValueEnum = match op {
                    IntAdd => b.build_int_add(l.into_int_value(), r.into_int_value(), "add").unwrap().into(),
                    IntSub => b.build_int_sub(l.into_int_value(), r.into_int_value(), "sub").unwrap().into(),
                    IntMul => b.build_int_mul(l.into_int_value(), r.into_int_value(), "mul").unwrap().into(),
                    IntBitAnd => b.build_and(l.into_int_value(), r.into_int_value(), "and").unwrap().into(),
                    IntBitOr  => b.build_or(l.into_int_value(), r.into_int_value(), "or").unwrap().into(),
                    IntBitXor => b.build_xor(l.into_int_value(), r.into_int_value(), "xor").unwrap().into(),
                    IntShl    => b.build_left_shift(l.into_int_value(), r.into_int_value(), "shl").unwrap().into(),
                    IntShr    => b.build_right_shift(l.into_int_value(), r.into_int_value(), true, "shr").unwrap().into(),
                    IntEq => b.build_int_compare(IntPredicate::EQ, l.into_int_value(), r.into_int_value(), "eq").unwrap().into(),
                    IntNe => b.build_int_compare(IntPredicate::NE, l.into_int_value(), r.into_int_value(), "ne").unwrap().into(),
                    IntLt => b.build_int_compare(IntPredicate::SLT, l.into_int_value(), r.into_int_value(), "lt").unwrap().into(),
                    IntLe => b.build_int_compare(IntPredicate::SLE, l.into_int_value(), r.into_int_value(), "le").unwrap().into(),
                    IntGt => b.build_int_compare(IntPredicate::SGT, l.into_int_value(), r.into_int_value(), "gt").unwrap().into(),
                    IntGe => b.build_int_compare(IntPredicate::SGE, l.into_int_value(), r.into_int_value(), "ge").unwrap().into(),
                    CharEq => b.build_int_compare(IntPredicate::EQ, l.into_int_value(), r.into_int_value(), "eq").unwrap().into(),
                    CharNe => b.build_int_compare(IntPredicate::NE, l.into_int_value(), r.into_int_value(), "ne").unwrap().into(),
                    CharLt => b.build_int_compare(IntPredicate::ULT, l.into_int_value(), r.into_int_value(), "lt").unwrap().into(),
                    CharLe => b.build_int_compare(IntPredicate::ULE, l.into_int_value(), r.into_int_value(), "le").unwrap().into(),
                    CharGt => b.build_int_compare(IntPredicate::UGT, l.into_int_value(), r.into_int_value(), "gt").unwrap().into(),
                    CharGe => b.build_int_compare(IntPredicate::UGE, l.into_int_value(), r.into_int_value(), "ge").unwrap().into(),
                    BoolEq => b.build_int_compare(IntPredicate::EQ, l.into_int_value(), r.into_int_value(), "eq").unwrap().into(),
                    BoolNe => b.build_int_compare(IntPredicate::NE, l.into_int_value(), r.into_int_value(), "ne").unwrap().into(),
                    RealAdd => b.build_float_add(l.into_float_value(), r.into_float_value(), "add").unwrap().into(),
                    RealSub => b.build_float_sub(l.into_float_value(), r.into_float_value(), "sub").unwrap().into(),
                    RealMul => b.build_float_mul(l.into_float_value(), r.into_float_value(), "mul").unwrap().into(),
                    RealDiv => b.build_float_div(l.into_float_value(), r.into_float_value(), "div").unwrap().into(),
                    RealEq => b.build_float_compare(FloatPredicate::OEQ, l.into_float_value(), r.into_float_value(), "eq").unwrap().into(),
                    RealNe => b.build_float_compare(FloatPredicate::UNE, l.into_float_value(), r.into_float_value(), "ne").unwrap().into(),
                    RealLt => b.build_float_compare(FloatPredicate::OLT, l.into_float_value(), r.into_float_value(), "lt").unwrap().into(),
                    RealLe => b.build_float_compare(FloatPredicate::OLE, l.into_float_value(), r.into_float_value(), "le").unwrap().into(),
                    RealGt => b.build_float_compare(FloatPredicate::OGT, l.into_float_value(), r.into_float_value(), "gt").unwrap().into(),
                    RealGe => b.build_float_compare(FloatPredicate::OGE, l.into_float_value(), r.into_float_value(), "ge").unwrap().into(),
                    _ => unreachable!("array, optional, div/mod, and opaque ops are handled elsewhere"),
                };
                value
            }
        }
    }

    fn lower_short_circuit(
        &mut self,
        left: &Expression,
        is_and: bool,
        right: &Expression,
    ) -> BasicValueEnum<'ctx> {
        let function = self.frame().function;
        let lhs = self.lower_expression(left).into_int_value();
        let lhs_block = self.builder.get_insert_block().unwrap();

        let rhs_block = self.context.append_basic_block(function, "sc_rhs");
        let merge_block = self.context.append_basic_block(function, "sc_merge");

        // false && _ == false; true || _ == true
        let short_value = if is_and {
            self.builder
                .build_conditional_branch(lhs, rhs_block, merge_block)
                .unwrap();
            self.context.bool_type().const_int(0, false)
        } else {
            self.builder
                .build_conditional_branch(lhs, merge_block, rhs_block)
                .unwrap();
            self.context.bool_type().const_int(1, false)
        };

        self.builder.position_at_end(rhs_block);
        let rhs = self.lower_expression(right).into_int_value();
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
        phi.as_basic_value()
    }

    fn lower_cast(&mut self, kind: CastKind, operand: &Expression) -> BasicValueEnum<'ctx> {
        let value = self.lower_expression(operand);
        let b = &self.builder;
        match kind {
            CastKind::IntToReal => b
                .build_signed_int_to_float(value.into_int_value(), self.context.f64_type(), "cast")
                .unwrap()
                .into(),
            CastKind::RealToInt => b
                .build_float_to_signed_int(
                    value.into_float_value(),
                    self.context.i64_type(),
                    "cast",
                )
                .unwrap()
                .into(),
            CastKind::IntToChar => b
                .build_int_truncate(value.into_int_value(), self.context.i8_type(), "cast")
                .unwrap()
                .into(),
            CastKind::CharToInt => b
                .build_int_z_extend(value.into_int_value(), self.context.i64_type(), "cast")
                .unwrap()
                .into(),
            CastKind::CharToReal => b
                .build_unsigned_int_to_float(
                    value.into_int_value(),
                    self.context.f64_type(),
                    "cast",
                )
                .unwrap()
                .into(),
            CastKind::RealToChar => b
                .build_float_to_unsigned_int(
                    value.into_float_value(),
                    self.context.i8_type(),
                    "cast",
                )
                .unwrap()
                .into(),
        }
    }

    /// `copy(x)` universal shallow copy of an aggregate.
    fn lower_copy(&mut self, arg: &Expression) -> BasicValueEnum<'ctx> {
        match self.program.types[arg.ty] {
            Type::Struct(_) => {
                let struct_type = self.struct_type_of(arg.ty);
                let size = struct_type.size_of().unwrap();
                let source = self.lower_expression(arg).into_pointer_value();
                let copy = self.gc_malloc(size, "copy");
                self.builder.build_memcpy(copy, 1, source, 1, size).unwrap();
                copy.into()
            }
            Type::Array(_) => self.lower_array_copy(arg),
            _ => unreachable!("type checker rejects copy of scalars"),
        }
    }

    fn check_nonzero_divisor(&mut self, divisor: IntValue<'ctx>) {
        let is_zero = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                divisor,
                divisor.get_type().const_zero(),
                "is_zero",
            )
            .unwrap();
        self.panic_if(is_zero, "division by zero");
    }
}
