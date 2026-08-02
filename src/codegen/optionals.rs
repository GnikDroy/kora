use inkwell::IntPredicate;
use inkwell::values::{BasicValue, BasicValueEnum, IntValue};

use super::CodeGen;
use crate::ir::{BinOp, Expression, ExpressionKind, Type, TypeId};

impl<'ctx, 'a> CodeGen<'ctx, 'a> {
    /// The inner of an optional type. The IR guarantees the caller holds one.
    fn optional_inner(&self, optional: TypeId) -> TypeId {
        let Type::Optional(inner) = self.program.types[optional] else {
            unreachable!("expected an optional type");
        };
        inner
    }

    pub(super) fn lower_optional_wrap(
        &mut self,
        value: BasicValueEnum<'ctx>,
        optional: TypeId,
    ) -> BasicValueEnum<'ctx> {
        let inner = self.optional_inner(optional);
        if self.is_reference(inner) {
            return value;
        }
        let ty = self.basic_type(optional).into_struct_type();
        let tag = self.context.i8_type().const_int(1, false);
        let some = ty.const_zero();
        let some = self.builder.build_insert_value(some, tag, 0, "some").unwrap();
        let some = self
            .builder
            .build_insert_value(some, value, 1, "some")
            .unwrap();
        some.as_basic_value_enum()
    }

    pub(super) fn lower_unwrap(&mut self, operand: &Expression) -> BasicValueEnum<'ctx> {
        let inner = self.optional_inner(operand.ty);
        let value = self.lower_expression(operand);
        let is_none = self.is_optional_none(value, inner);
        self.panic_if(is_none, "force-unwrapped a none value");
        if self.is_reference(inner) {
            return value;
        }
        self.builder
            .build_extract_value(value.into_struct_value(), 1, "unwrapped")
            .unwrap()
    }

    fn is_optional_none(&mut self, value: BasicValueEnum<'ctx>, inner: TypeId) -> IntValue<'ctx> {
        if self.is_reference(inner) {
            return self
                .builder
                .build_is_null(value.into_pointer_value(), "is_none")
                .unwrap();
        }
        let tag = self
            .builder
            .build_extract_value(value.into_struct_value(), 0, "tag")
            .unwrap()
            .into_int_value();
        self.builder
            .build_int_compare(
                IntPredicate::EQ,
                tag,
                self.context.i8_type().const_zero(),
                "is_none",
            )
            .unwrap()
    }

    pub(super) fn lower_optional_equality(
        &mut self,
        op: BinOp,
        left: &Expression,
        right: &Expression,
    ) -> BasicValueEnum<'ctx> {
        // Both operands were coerced to the optional type during IR lowering,
        // so either side's type gives the shared inner.
        let optional = if matches!(self.program.types[left.ty], Type::Optional(_)) {
            left.ty
        } else {
            right.ty
        };
        let inner = self.optional_inner(optional);

        let equal = if matches!(left.kind, ExpressionKind::None) {
            let value = self.lower_expression(right);
            self.is_optional_none(value, inner)
        } else if matches!(right.kind, ExpressionKind::None) {
            let value = self.lower_expression(left);
            self.is_optional_none(value, inner)
        } else {
            let a = self.lower_expression(left);
            let b = self.lower_expression(right);
            self.is_optional_values_equal(inner, a, b)
        };
        if op == BinOp::OptionalNe {
            return self.builder.build_not(equal, "ne").unwrap().into();
        }
        equal.into()
    }

    pub(super) fn is_optional_values_equal(
        &mut self,
        inner: TypeId,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
    ) -> IntValue<'ctx> {
        if !self.is_reference(inner) {
            let tag_a = self.is_optional_none(a, inner);
            let tag_b = self.is_optional_none(b, inner);
            let a = self
                .builder
                .build_extract_value(a.into_struct_value(), 1, "a")
                .unwrap();
            let b = self
                .builder
                .build_extract_value(b.into_struct_value(), 1, "b")
                .unwrap();
            let tags_equal = self
                .builder
                .build_int_compare(IntPredicate::EQ, tag_a, tag_b, "tags_eq")
                .unwrap();

            let values_equal = match self.program.types[inner] {
                Type::Int | Type::Char | Type::Bool => self
                    .builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        a.into_int_value(),
                        b.into_int_value(),
                        "values_eq",
                    )
                    .unwrap(),
                Type::Real => self
                    .builder
                    .build_float_compare(
                        inkwell::FloatPredicate::OEQ,
                        a.into_float_value(),
                        b.into_float_value(),
                        "values_eq",
                    )
                    .unwrap(),
                _ => unreachable!("type checker limits optional equality to comparable inners"),
            };
            return self
                .builder
                .build_and(tags_equal, values_equal, "eq")
                .unwrap();
        }

        let a = a.into_pointer_value();
        let b = b.into_pointer_value();
        if matches!(self.program.types[inner], Type::Opaque) {
            return self.pointers_equal(a, b);
        }
        let a_null = self.builder.build_is_null(a, "a_null").unwrap();
        let b_null = self.builder.build_is_null(b, "b_null").unwrap();
        let either_null = self.builder.build_or(a_null, b_null, "either_null").unwrap();
        let both_null = self.builder.build_and(a_null, b_null, "both_null").unwrap();

        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();
        let compare = self.context.append_basic_block(function, "opt_cmp");
        let merge = self.context.append_basic_block(function, "opt_merge");
        let entry_end = self.builder.get_insert_block().unwrap();
        self.builder
            .build_conditional_branch(either_null, merge, compare)
            .unwrap();

        self.builder.position_at_end(compare);
        let values_equal = match self.program.types[inner] {
            Type::Array(_) => {
                let eq = self.array_equality_fn(inner);
                let call = self
                    .builder
                    .build_call(eq, &[a.into(), b.into()], "values_eq")
                    .unwrap();
                self.call_value(call).into_int_value()
            }
            _ => unreachable!("type checker limits optional structural equality to arrays"),
        };
        let compare_end = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge).unwrap();

        self.builder.position_at_end(merge);
        let phi = self
            .builder
            .build_phi(self.context.bool_type(), "eq")
            .unwrap();
        phi.add_incoming(&[(&both_null, entry_end), (&values_equal, compare_end)]);
        phi.as_basic_value().into_int_value()
    }
}
