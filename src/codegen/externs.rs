use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

use super::CodeGen;
use crate::parser::*;

fn is_identity(ty: &ExternType) -> bool {
    use ExternType::*;
    match ty {
        Int64 | UInt64 | CLong | CULong | CSize | Float64 | Char | Opaque => true,
        Optional(inner) => matches!(**inner, Opaque),
        _ => false,
    }
}

impl<'ctx> CodeGen<'ctx, '_> {
    pub(super) fn declare_extern_function(&mut self, func: &Spanned<ExternFunction>) {
        let name = &func.node.name;
        let param_types: Vec<_> = func
            .node
            .arguments
            .iter()
            .map(|arg| self.extern_llvm_type(&arg.node.typename).into())
            .collect();
        let c_type = match &func.node.return_type {
            Some(ty) => self.extern_llvm_type(ty).fn_type(&param_types, false),
            None => self.context.void_type().fn_type(&param_types, false),
        };
        let c_fn = self
            .module
            .get_function(name)
            .unwrap_or_else(|| self.module.add_function(name, c_type, None));

        let symbol = self
            .program
            .symbols
            .symbol_id_of_declaration(func.id)
            .unwrap();
        let identity = func
            .node
            .arguments
            .iter()
            .all(|a| is_identity(&a.node.typename))
            && func.node.return_type.as_ref().is_none_or(is_identity);
        let function = if identity {
            c_fn
        } else {
            self.extern_thunk(&func.node, c_fn)
        };
        self.functions.insert(symbol, function);
    }

    fn extern_llvm_type(&self, ty: &ExternType) -> BasicTypeEnum<'ctx> {
        use ExternType::*;
        match ty {
            Int8 | UInt8 | Bool | Char => self.context.i8_type().into(),
            Int16 | UInt16 => self.context.i16_type().into(),
            Int32 | UInt32 | CInt | CUInt => self.context.i32_type().into(),
            Int64 | UInt64 | CLong | CULong | CSize => self.context.i64_type().into(),
            Float32 => self.context.f32_type().into(),
            Float64 => self.context.f64_type().into(),
            CString | Opaque | Optional(_) => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
        }
    }

    fn extern_thunk(
        &mut self,
        func: &ExternFunction,
        c_fn: FunctionValue<'ctx>,
    ) -> FunctionValue<'ctx> {
        let thunk_name = format!("{}.thunk", func.name);
        if let Some(thunk) = self.module.get_function(&thunk_name) {
            return thunk;
        }

        let span = Span::default();
        let param_types: Vec<_> = func
            .arguments
            .iter()
            .map(|arg| {
                let projected = arg.node.typename.projection();
                self.basic_type(&projected, &span).unwrap().into()
            })
            .collect();
        let thunk_type = match &func.return_type {
            Some(ty) => {
                let projected = ty.projection();
                self.basic_type(&projected, &span)
                    .unwrap()
                    .fn_type(&param_types, false)
            }
            None => self.context.void_type().fn_type(&param_types, false),
        };
        let thunk = self
            .module
            .add_function(&thunk_name, thunk_type, Some(Linkage::Internal));

        let saved = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(thunk, "entry");
        self.builder.position_at_end(entry);

        let args: Vec<BasicMetadataValueEnum> = func
            .arguments
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                let value = thunk.get_nth_param(i as u32).unwrap();
                self.marshal_argument(&arg.node.typename, value).into()
            })
            .collect();
        let call = self.builder.build_call(c_fn, &args, "c").unwrap();

        match &func.return_type {
            None => self.builder.build_return(None).unwrap(),
            Some(ty) => {
                let raw = match call.try_as_basic_value() {
                    super::ValueKind::Basic(value) => value,
                    _ => unreachable!("non-void extern returns a value"),
                };
                let value = self.marshal_return(ty, raw);
                self.builder.build_return(Some(&value)).unwrap()
            }
        };

        if let Some(block) = saved {
            self.builder.position_at_end(block);
        }
        thunk
    }

    fn marshal_argument(
        &mut self,
        ty: &ExternType,
        value: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        use ExternType::*;
        match ty {
            Int8 | Int16 | Int32 | UInt8 | UInt16 | UInt32 | CInt | CUInt => {
                let target = self.extern_llvm_type(ty).into_int_type();
                self.builder
                    .build_int_truncate(value.into_int_value(), target, "trunc")
                    .unwrap()
                    .into()
            }
            Bool => self
                .builder
                .build_int_z_extend(value.into_int_value(), self.context.i8_type(), "b")
                .unwrap()
                .into(),
            Float32 => self
                .builder
                .build_float_trunc(value.into_float_value(), self.context.f32_type(), "f")
                .unwrap()
                .into(),
            CString => self.array_buffer(value.into_pointer_value()).into(),
            Optional(inner) if matches!(**inner, CString) => {
                let array = value.into_pointer_value();
                let function = self
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_parent()
                    .unwrap();
                let load = self.context.append_basic_block(function, "cstr_load");
                let merge = self.context.append_basic_block(function, "cstr_merge");
                let is_null = self.builder.build_is_null(array, "is_null").unwrap();
                let entry_end = self.builder.get_insert_block().unwrap();
                self.builder
                    .build_conditional_branch(is_null, merge, load)
                    .unwrap();

                self.builder.position_at_end(load);
                let buf = self.array_buffer(array);
                let load_end = self.builder.get_insert_block().unwrap();
                self.builder.build_unconditional_branch(merge).unwrap();

                self.builder.position_at_end(merge);
                let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
                let phi = self.builder.build_phi(ptr, "cstr").unwrap();
                let null = ptr.const_null();
                phi.add_incoming(&[(&null, entry_end), (&buf, load_end)]);
                phi.as_basic_value()
            }
            _ => value,
        }
    }

    fn marshal_return(
        &mut self,
        ty: &ExternType,
        raw: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        use ExternType::*;
        let i64_type = self.context.i64_type();
        match ty {
            Int8 | Int16 | Int32 | CInt => self
                .builder
                .build_int_s_extend(raw.into_int_value(), i64_type, "sext")
                .unwrap()
                .into(),
            UInt8 | UInt16 | UInt32 | CUInt => self
                .builder
                .build_int_z_extend(raw.into_int_value(), i64_type, "zext")
                .unwrap()
                .into(),
            Bool => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::NE,
                    raw.into_int_value(),
                    self.context.i8_type().const_zero(),
                    "b",
                )
                .unwrap()
                .into(),
            Float32 => self
                .builder
                .build_float_ext(raw.into_float_value(), self.context.f64_type(), "f")
                .unwrap()
                .into(),
            CString => {
                let is_null = self
                    .builder
                    .build_is_null(raw.into_pointer_value(), "is_null")
                    .unwrap();
                self.panic_if(is_null, "extern returned a null cstring");
                self.array_from_cstring(raw.into_pointer_value()).into()
            }
            Optional(inner) if matches!(**inner, CString) => {
                self.array_from_cstring(raw.into_pointer_value()).into()
            }
            _ => raw,
        }
    }

    fn array_buffer(&mut self, array: PointerValue<'ctx>) -> PointerValue<'ctx> {
        let header = self.array_header_type();
        let slot = self
            .builder
            .build_struct_gep(header, array, 2, "buf_slot")
            .unwrap();
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        self.builder
            .build_load(ptr, slot, "buf")
            .unwrap()
            .into_pointer_value()
    }

    fn array_from_cstring(&mut self, raw: PointerValue<'ctx>) -> PointerValue<'ctx> {
        let ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let helper = self
            .module
            .get_function("__kora_array_from_cstring")
            .unwrap_or_else(|| {
                let ty = ptr.fn_type(&[ptr.into()], false);
                self.module
                    .add_function("__kora_array_from_cstring", ty, None)
            });
        let call = self
            .builder
            .build_call(helper, &[raw.into()], "kstr")
            .unwrap();
        let super::ValueKind::Basic(value) = call.try_as_basic_value() else {
            unreachable!();
        };
        value.into_pointer_value()
    }
}
