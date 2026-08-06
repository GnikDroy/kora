use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

use super::CodeGen;
use crate::ir::ExternDef;
use crate::parser::ExternType;

fn extern_abi_tag(ty: &ExternType) -> String {
    use ExternType::*;
    match ty {
        Int8 => "i8".into(),
        Int16 => "i16".into(),
        Int32 => "i32".into(),
        Int64 => "i64".into(),
        UInt8 => "u8".into(),
        UInt16 => "u16".into(),
        UInt32 => "u32".into(),
        UInt64 => "u64".into(),
        Float32 => "f32".into(),
        Float64 => "f64".into(),
        Bool => "b".into(),
        Char => "c".into(),
        CString => "s".into(),
        Opaque => "p".into(),
        CInt => "ci".into(),
        CUInt => "cu".into(),
        CLong => "cl".into(),
        CULong => "clu".into(),
        CSize => "cz".into(),
        Optional(inner) => format!("o{}", extern_abi_tag(inner)),
        Function { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| extern_abi_tag(t))
                .unwrap_or_else(|| "v".into());
            let ps: Vec<_> = params.iter().map(extern_abi_tag).collect();
            format!("F{}_{}", r, ps.join("."))
        }
    }
}

fn optional_is_reference(inner: &ExternType) -> bool {
    matches!(inner, ExternType::CString | ExternType::Opaque)
}

impl<'ctx, 'a> CodeGen<'ctx, 'a> {
    pub(super) fn declare_extern_function(&mut self, ext: &ExternDef) -> FunctionValue<'ctx> {
        let name = &ext.symbol;
        let param_types: Vec<_> = ext
            .params
            .iter()
            .map(|ty| self.extern_llvm_type(ty).into())
            .collect();
        let c_type = match &ext.ret {
            Some(ty) => self.extern_llvm_type(ty).fn_type(&param_types, false),
            None => self.context.void_type().fn_type(&param_types, false),
        };
        let c_fn = self
            .module
            .get_function(name)
            .unwrap_or_else(|| self.module.add_function(name, c_type, None));

        let identity = ext.params.iter().all(ExternType::has_identical_crepr)
            && ext.ret.as_ref().is_none_or(|t| t.has_identical_crepr());
        if identity {
            c_fn
        } else {
            self.extern_thunk(ext, c_fn)
        }
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
            CString | Opaque | Optional(_) | Function { .. } => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
        }
    }

    fn extern_kora_type(&self, ty: &ExternType) -> BasicTypeEnum<'ctx> {
        use ExternType::*;
        match ty {
            Int8 | Int16 | Int32 | Int64 | UInt8 | UInt16 | UInt32 | UInt64 | CInt | CUInt
            | CLong | CULong | CSize => self.context.i64_type().into(),
            Float32 | Float64 => self.context.f64_type().into(),
            Bool => self.context.bool_type().into(),
            Char => self.context.i8_type().into(),
            CString | Opaque | Function { .. } => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
            Optional(inner) => {
                if optional_is_reference(inner) {
                    return self
                        .context
                        .ptr_type(inkwell::AddressSpace::default())
                        .into();
                }
                let inner = self.extern_kora_type(inner);
                let tag = self.context.i8_type().into();
                self.context.struct_type(&[tag, inner], false).into()
            }
        }
    }

    fn extern_thunk(&mut self, ext: &ExternDef, c_fn: FunctionValue<'ctx>) -> FunctionValue<'ctx> {
        let thunk_name = format!("{}.thunk", ext.symbol);
        if let Some(thunk) = self.module.get_function(&thunk_name) {
            return thunk;
        }

        let param_types: Vec<_> = ext
            .params
            .iter()
            .map(|ty| self.extern_kora_type(ty).into())
            .collect();
        let thunk_type = match &ext.ret {
            Some(ty) => self.extern_kora_type(ty).fn_type(&param_types, false),
            None => self.context.void_type().fn_type(&param_types, false),
        };
        let thunk = self
            .module
            .add_function(&thunk_name, thunk_type, Some(Linkage::Internal));

        let saved = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(thunk, "entry");
        self.builder.position_at_end(entry);

        let args: Vec<BasicMetadataValueEnum> = ext
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let value = thunk.get_nth_param(i as u32).unwrap();
                self.marshal_argument(ty, value).into()
            })
            .collect();
        let call = self.builder.build_call(c_fn, &args, "c").unwrap();

        match &ext.ret {
            None => self.builder.build_return(None).unwrap(),
            Some(ty) => {
                let raw = self.call_value(call);
                let value = self.marshal_return(ty, raw);
                self.builder.build_return(Some(&value)).unwrap()
            }
        };

        if let Some(block) = saved {
            self.builder.position_at_end(block);
        }
        thunk
    }

    pub(super) fn c_callback_thunk(
        &mut self,
        target: crate::ir::FunctionId,
        params: &[ExternType],
        ret: &Option<Box<ExternType>>,
    ) -> PointerValue<'ctx> {
        let callee = self.function_value(target);
        let ret_tag = ret
            .as_ref()
            .map(|t| extern_abi_tag(t))
            .unwrap_or_else(|| "v".into());
        let sig_tag: Vec<_> = params.iter().map(extern_abi_tag).collect();
        let thunk_name = format!(
            "{}.cthunk.{}_{}",
            callee.get_name().to_str().unwrap(),
            ret_tag,
            sig_tag.join(".")
        );
        if let Some(existing) = self.module.get_function(&thunk_name) {
            return existing.as_global_value().as_pointer_value();
        }

        let c_params: Vec<_> = params
            .iter()
            .map(|t| self.extern_llvm_type(t).into())
            .collect();
        let c_type = match ret {
            Some(t) => self.extern_llvm_type(t).fn_type(&c_params, false),
            None => self.context.void_type().fn_type(&c_params, false),
        };
        let thunk = self
            .module
            .add_function(&thunk_name, c_type, Some(Linkage::Internal));

        let saved = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(thunk, "entry");
        self.builder.position_at_end(entry);

        let args: Vec<BasicMetadataValueEnum> = params
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let raw = thunk.get_nth_param(i as u32).unwrap();
                self.marshal_return(ty, raw).into()
            })
            .collect();
        let call = self.builder.build_call(callee, &args, "kora").unwrap();

        match ret {
            None => {
                self.builder.build_return(None).unwrap();
            }
            Some(ty) => {
                let raw = self.call_value(call);
                let value = self.marshal_argument(ty, raw);
                self.builder.build_return(Some(&value)).unwrap();
            }
        }

        if let Some(block) = saved {
            self.builder.position_at_end(block);
        }
        thunk.as_global_value().as_pointer_value()
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
        self.call_value(call).into_pointer_value()
    }
}
