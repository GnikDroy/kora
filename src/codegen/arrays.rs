use inkwell::AddressSpace;
use inkwell::types::StructType;
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind,
};
use inkwell::{FloatPredicate, IntPredicate};

use super::{CodeGen, CodegenErr};
use crate::parser::*;
use crate::semantic_analyzer::ArrayMethod;

impl<'ctx> CodeGen<'ctx, '_> {
    /// { i64 len, i64 cap, ptr buf }
    pub(super) fn array_header_type(&mut self) -> StructType<'ctx> {
        if let Some(ty) = self.struct_types.get("k.array") {
            return *ty;
        }
        let ty = self.context.opaque_struct_type("k.array");
        ty.set_body(
            &[
                self.context.i64_type().into(),
                self.context.i64_type().into(),
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        );
        self.struct_types.insert("k.array".to_string(), ty);
        ty
    }

    /// Get-or-declare one of the __kora_array_* runtime helpers
    fn array_fn(&mut self, name: &str) -> FunctionValue<'ctx> {
        if let Some(function) = self.module.get_function(name) {
            return function;
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let void = self.context.void_type();
        let p = ptr.into();
        let i = self.context.i64_type().into();
        let ty = match name {
            "__kora_array_new" => ptr.fn_type(&[i, i, i], false),
            "__kora_array_lit" => ptr.fn_type(&[p, i, i], false),
            "__kora_array_push" => void.fn_type(&[p, p, i], false),
            "__kora_array_pop" => void.fn_type(&[p, p, i], false),
            "__kora_array_insert" => void.fn_type(&[p, i, p, i], false),
            "__kora_array_remove" => void.fn_type(&[p, i, p, i], false),
            "__kora_array_slice" => ptr.fn_type(&[p, i, i, i], false),
            "__kora_array_extend" => void.fn_type(&[p, p, i], false),
            "__kora_array_concat" => ptr.fn_type(&[p, p, i], false),
            "__kora_array_copy" => ptr.fn_type(&[p, i], false),
            _ => unreachable!("unknown array runtime helper"),
        };
        self.module.add_function(name, ty, None)
    }

    fn array_fn_call(
        &mut self,
        name: &str,
        args: &[BasicMetadataValueEnum<'ctx>],
    ) -> Option<BasicValueEnum<'ctx>> {
        let function = self.array_fn(name);
        let call = self.builder.build_call(function, args, "").unwrap();
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Some(value),
            ValueKind::Instruction(_) => None,
        }
    }

    fn array_element_type(&self, expr: &Spanned<Expression>) -> Type {
        let Type::Array(elem) = self.program.types[&expr.id].clone() else {
            unreachable!("expression is array-typed");
        };
        *elem
    }

    fn array_len(&mut self, array: PointerValue<'ctx>) -> IntValue<'ctx> {
        let header = self.array_header_type();
        let ptr = self
            .builder
            .build_struct_gep(header, array, 0, "len_ptr")
            .unwrap();
        self.builder
            .build_load(self.context.i64_type(), ptr, "len")
            .unwrap()
            .into_int_value()
    }

    fn array_buf(&mut self, array: PointerValue<'ctx>) -> PointerValue<'ctx> {
        let header = self.array_header_type();
        let ptr = self
            .builder
            .build_struct_gep(header, array, 2, "buf_ptr")
            .unwrap();
        self.builder
            .build_load(self.context.ptr_type(AddressSpace::default()), ptr, "buf")
            .unwrap()
            .into_pointer_value()
    }

    pub(super) fn array_new(
        &mut self,
        array_type: &Type,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let Type::Array(elem) = array_type else {
            unreachable!();
        };
        let (_, size) = self.type_layout(elem, span)?;
        let zero = self.context.i64_type().const_zero();
        Ok(self
            .array_fn_call("__kora_array_new", &[zero.into(), zero.into(), size.into()])
            .unwrap())
    }

    pub(super) fn array_element_pointer(
        &mut self,
        array: &Spanned<Expression>,
        index: &Spanned<Expression>,
        span: &Span,
    ) -> Result<PointerValue<'ctx>, CodegenErr> {
        let elem = self.array_element_type(array);
        let (elem_ty, _) = self.type_layout(&elem, span)?;
        let array = self.lower_expression(array)?.into_pointer_value();
        let index = self.lower_expression(index)?.into_int_value();
        let len = self.array_len(array);
        let oob = self
            .builder
            .build_int_compare(IntPredicate::UGE, index, len, "oob")
            .unwrap();
        self.panic_if(oob, "index out of bounds");
        let buf = self.array_buf(array);
        Ok(unsafe {
            self.builder
                .build_gep(elem_ty, buf, &[index], "elem")
                .unwrap()
        })
    }

    pub(super) fn lower_array_literal(
        &mut self,
        expr: &Spanned<Expression>,
        elems: &[Spanned<Expression>],
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let elem = self.array_element_type(expr);
        let (elem_ty, size) = self.type_layout(&elem, &expr.span)?;
        let len = self.context.i64_type().const_int(elems.len() as u64, false);
        let zero = self.context.i64_type().const_zero();
        let array = self
            .array_fn_call("__kora_array_new", &[len.into(), zero.into(), size.into()])
            .unwrap()
            .into_pointer_value();
        let buf = self.array_buf(array);
        for (i, elem_expr) in elems.iter().enumerate() {
            let value = self.lower_expression_expecting(elem_expr, &elem)?;
            let index = self.context.i64_type().const_int(i as u64, false);
            let slot = unsafe {
                self.builder
                    .build_gep(elem_ty, buf, &[index], "slot")
                    .unwrap()
            };
            self.builder.build_store(slot, value).unwrap();
        }
        Ok(array.into())
    }

    pub(super) fn lower_string_literal(&mut self, s: &str) -> BasicValueEnum<'ctx> {
        let global = self.builder.build_global_string_ptr(s, "str").unwrap();
        let len = self.context.i64_type().const_int(s.len() as u64, false);
        let one = self.context.i64_type().const_int(1, false);
        self.array_fn_call(
            "__kora_array_lit",
            &[global.as_pointer_value().into(), len.into(), one.into()],
        )
        .unwrap()
    }

    pub(super) fn lower_array_construct(
        &mut self,
        typename: &Type,
        size_expr: &Spanned<Expression>,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let (elem_ty, size) = self.type_layout(typename, span)?;
        let n = self.lower_expression(size_expr)?.into_int_value();
        let zero = self.context.i64_type().const_zero();
        let array = self
            .array_fn_call("__kora_array_new", &[n.into(), zero.into(), size.into()])
            .unwrap()
            .into_pointer_value();

        if let Type::Struct(name) = typename {
            let constructor = self.struct_constructor(&name.node, span)?;
            let buf = self.array_buf(array);
            let function = self.frame().function;
            let slot_index = self.entry_alloca(self.context.i64_type().into(), "i");
            self.builder.build_store(slot_index, zero).unwrap();
            let cond = self.context.append_basic_block(function, "fill_cond");
            let body = self.context.append_basic_block(function, "fill_body");
            let after = self.context.append_basic_block(function, "fill_after");
            self.builder.build_unconditional_branch(cond).unwrap();

            self.builder.position_at_end(cond);
            let i = self
                .builder
                .build_load(self.context.i64_type(), slot_index, "i")
                .unwrap()
                .into_int_value();
            let done = self
                .builder
                .build_int_compare(IntPredicate::SGE, i, n, "done")
                .unwrap();
            self.builder
                .build_conditional_branch(done, after, body)
                .unwrap();

            self.builder.position_at_end(body);
            let call = self.builder.build_call(constructor, &[], "new").unwrap();
            let ValueKind::Basic(value) = call.try_as_basic_value() else {
                unreachable!();
            };
            let slot = unsafe { self.builder.build_gep(elem_ty, buf, &[i], "slot").unwrap() };
            self.builder.build_store(slot, value).unwrap();
            let one = self.context.i64_type().const_int(1, false);
            let next = self.builder.build_int_add(i, one, "next").unwrap();
            self.builder.build_store(slot_index, next).unwrap();
            self.builder.build_unconditional_branch(cond).unwrap();

            self.builder.position_at_end(after);
        }
        Ok(array.into())
    }

    pub(super) fn lower_array_method(
        &mut self,
        method: ArrayMethod,
        obj: &Spanned<Expression>,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodegenErr> {
        let elem = self.array_element_type(obj);
        let (elem_ty, size) = self.type_layout(&elem, span)?;
        let array = self.lower_expression(obj)?.into_pointer_value();
        let value = match method {
            ArrayMethod::Len => Some(self.array_len(array).into()),
            ArrayMethod::Push => {
                let value = self.lower_expression_expecting(&args[0], &elem)?;
                let slot = self.spill(value);
                self.array_fn_call(
                    "__kora_array_push",
                    &[array.into(), slot.into(), size.into()],
                );
                None
            }
            ArrayMethod::Pop => {
                let out = self.entry_alloca(elem_ty, "popped");
                self.array_fn_call("__kora_array_pop", &[array.into(), out.into(), size.into()]);
                Some(self.builder.build_load(elem_ty, out, "popped").unwrap())
            }
            ArrayMethod::Insert => {
                let index = self.lower_expression(&args[0])?;
                let value = self.lower_expression_expecting(&args[1], &elem)?;
                let slot = self.spill(value);
                self.array_fn_call(
                    "__kora_array_insert",
                    &[array.into(), index.into(), slot.into(), size.into()],
                );
                None
            }
            ArrayMethod::Remove => {
                let index = self.lower_expression(&args[0])?;
                let out = self.entry_alloca(elem_ty, "removed");
                self.array_fn_call(
                    "__kora_array_remove",
                    &[array.into(), index.into(), out.into(), size.into()],
                );
                Some(self.builder.build_load(elem_ty, out, "removed").unwrap())
            }
            ArrayMethod::Slice => {
                let start = self.lower_expression(&args[0])?;
                let end = self.lower_expression(&args[1])?;
                self.array_fn_call(
                    "__kora_array_slice",
                    &[array.into(), start.into(), end.into(), size.into()],
                )
            }
            ArrayMethod::Extend => {
                let other = self.lower_expression(&args[0])?;
                self.array_fn_call(
                    "__kora_array_extend",
                    &[array.into(), other.into(), size.into()],
                );
                None
            }
        };
        Ok(value)
    }

    pub(super) fn lower_array_copy(
        &mut self,
        arg: &Spanned<Expression>,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let elem = self.array_element_type(arg);
        let (_, size) = self.type_layout(&elem, span)?;
        let array = self.lower_expression(arg)?;
        Ok(self
            .array_fn_call("__kora_array_copy", &[array.into(), size.into()])
            .unwrap())
    }

    pub(super) fn lower_array_binary(
        &mut self,
        array_type: &Type,
        op: BinaryOp,
        lhs: PointerValue<'ctx>,
        rhs: PointerValue<'ctx>,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        match op {
            BinaryOp::Add => {
                let Type::Array(elem) = array_type else {
                    unreachable!();
                };
                let (_, size) = self.type_layout(elem, span)?;
                Ok(self
                    .array_fn_call(
                        "__kora_array_concat",
                        &[lhs.into(), rhs.into(), size.into()],
                    )
                    .unwrap())
            }
            BinaryOp::Equality | BinaryOp::NotEquality => {
                let eq = self.array_equality_fn(array_type, span)?;
                let call = self
                    .builder
                    .build_call(eq, &[lhs.into(), rhs.into()], "eq")
                    .unwrap();
                let ValueKind::Basic(value) = call.try_as_basic_value() else {
                    unreachable!();
                };
                if op == BinaryOp::NotEquality {
                    let ne = self
                        .builder
                        .build_not(value.into_int_value(), "ne")
                        .unwrap();
                    return Ok(ne.into());
                }
                Ok(value)
            }
            _ => unreachable!("type checker rejects other array operators"),
        }
    }

    pub(super) fn array_equality_fn(
        &mut self,
        array_type: &Type,
        span: &Span,
    ) -> Result<FunctionValue<'ctx>, CodegenErr> {
        if let Some(function) = self.array_equality_fns.get(array_type) {
            return Ok(*function);
        }
        let Type::Array(elem) = array_type.clone() else {
            unreachable!();
        };
        let elem = *elem;
        let (elem_ty, _) = self.type_layout(&elem, span)?;
        let ptr = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            &format!("eq.{}", self.array_equality_fns.len()),
            self.context
                .bool_type()
                .fn_type(&[ptr.into(), ptr.into()], false),
            None,
        );
        self.array_equality_fns.insert(array_type.clone(), function);

        let saved_block = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(function, "entry");
        let loop_cond = self.context.append_basic_block(function, "loop_cond");
        let loop_body = self.context.append_basic_block(function, "loop_body");
        let loop_inc = self.context.append_basic_block(function, "loop_inc");
        let ret_true = self.context.append_basic_block(function, "ret_true");
        let ret_false = self.context.append_basic_block(function, "ret_false");

        self.builder.position_at_end(entry);
        let a = function.get_nth_param(0).unwrap().into_pointer_value();
        let b = function.get_nth_param(1).unwrap().into_pointer_value();
        let len_a = self.array_len(a);
        let len_b = self.array_len(b);
        let buf_a = self.array_buf(a);
        let buf_b = self.array_buf(b);
        let len_ne = self
            .builder
            .build_int_compare(IntPredicate::NE, len_a, len_b, "len_ne")
            .unwrap();
        self.builder
            .build_conditional_branch(len_ne, ret_false, loop_cond)
            .unwrap();

        self.builder.position_at_end(loop_cond);
        let i = self
            .builder
            .build_phi(self.context.i64_type(), "i")
            .unwrap();
        let iv = i.as_basic_value().into_int_value();
        let done = self
            .builder
            .build_int_compare(IntPredicate::SGE, iv, len_a, "done")
            .unwrap();
        self.builder
            .build_conditional_branch(done, ret_true, loop_body)
            .unwrap();

        self.builder.position_at_end(loop_body);
        let slot_a = unsafe { self.builder.build_gep(elem_ty, buf_a, &[iv], "pa").unwrap() };
        let slot_b = unsafe { self.builder.build_gep(elem_ty, buf_b, &[iv], "pb").unwrap() };
        let elem_a = self.builder.build_load(elem_ty, slot_a, "ea").unwrap();
        let elem_b = self.builder.build_load(elem_ty, slot_b, "eb").unwrap();
        let elem_eq = match &elem {
            Type::Int | Type::Char | Type::Bool => self
                .builder
                .build_int_compare(
                    IntPredicate::EQ,
                    elem_a.into_int_value(),
                    elem_b.into_int_value(),
                    "elem_eq",
                )
                .unwrap(),
            Type::Real => self
                .builder
                .build_float_compare(
                    FloatPredicate::OEQ,
                    elem_a.into_float_value(),
                    elem_b.into_float_value(),
                    "elem_eq",
                )
                .unwrap(),
            Type::Array(_) => {
                let inner = self.array_equality_fn(&elem, span)?;
                let call = self
                    .builder
                    .build_call(inner, &[elem_a.into(), elem_b.into()], "elem_eq")
                    .unwrap();
                let ValueKind::Basic(value) = call.try_as_basic_value() else {
                    unreachable!();
                };
                value.into_int_value()
            }
            Type::Opaque => {
                self.pointers_equal(elem_a.into_pointer_value(), elem_b.into_pointer_value())
            }
            Type::Optional(opt_inner) => {
                self.is_optional_values_equal(opt_inner, elem_a, elem_b, span)?
            }
            _ => unreachable!("type checker limits array equality to comparable elements"),
        };
        self.builder
            .build_conditional_branch(elem_eq, loop_inc, ret_false)
            .unwrap();

        self.builder.position_at_end(loop_inc);
        let one = self.context.i64_type().const_int(1, false);
        let next = self.builder.build_int_add(iv, one, "next").unwrap();
        self.builder.build_unconditional_branch(loop_cond).unwrap();

        let zero = self.context.i64_type().const_zero();
        i.add_incoming(&[(&zero, entry), (&next, loop_inc)]);

        self.builder.position_at_end(ret_true);
        let true_ = self.context.bool_type().const_int(1, false);
        self.builder.build_return(Some(&true_)).unwrap();
        self.builder.position_at_end(ret_false);
        let false_ = self.context.bool_type().const_zero();
        self.builder.build_return(Some(&false_)).unwrap();

        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        Ok(function)
    }
}
