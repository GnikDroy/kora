use inkwell::AddressSpace;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use super::CodeGen;
use crate::ir::{Expression, StructId, Type, TypeId};

impl<'ctx, 'a> CodeGen<'ctx, 'a> {
    pub(super) fn struct_type(&mut self, struct_: StructId) -> StructType<'ctx> {
        if let Some(ty) = self.struct_types.get(&struct_) {
            return *ty;
        }
        let field_types = self.program[struct_]
            .fields
            .iter()
            .map(|f| self.basic_type(f.ty))
            .collect::<Vec<_>>();
        let ty = self
            .context
            .opaque_struct_type(&self.program[struct_].symbol);
        ty.set_body(&field_types, false);
        self.struct_types.insert(struct_, ty);
        ty
    }

    pub(super) fn struct_type_of(&mut self, ty: TypeId) -> StructType<'ctx> {
        let Type::Struct(struct_) = self.program.types[ty] else {
            unreachable!("expected a struct type");
        };
        self.struct_type(struct_)
    }

    pub(super) fn struct_field_ptr(
        &mut self,
        object: &Expression,
        index: u32,
    ) -> PointerValue<'ctx> {
        let struct_type = self.struct_type_of(object.ty);
        let object = self.lower_expression(object).into_pointer_value();
        self.builder
            .build_struct_gep(struct_type, object, index, "field")
            .unwrap()
    }

    pub(super) fn struct_constructor(&mut self, struct_: StructId) -> FunctionValue<'ctx> {
        if let Some(function) = self.default_fns.get(&struct_) {
            return *function;
        }
        let struct_type = self.struct_type(struct_);
        let ptr = self.context.ptr_type(AddressSpace::default());
        let name = format!("default.{}", self.program[struct_].symbol);
        let function = self
            .module
            .add_function(&name, ptr.fn_type(&[], false), None);
        self.default_fns.insert(struct_, function);

        let saved_block = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let object = self.gc_malloc(struct_type.size_of().unwrap(), "new");
        let field_types: Vec<TypeId> = self.program[struct_].fields.iter().map(|f| f.ty).collect();
        for (index, field_ty) in field_types.into_iter().enumerate() {
            let value: BasicValueEnum = match self.program.types[field_ty] {
                Type::Struct(inner) => {
                    let inner_default = self.struct_constructor(inner);
                    let call = self.builder.build_call(inner_default, &[], "new").unwrap();
                    self.call_value(call)
                }
                Type::Array(elem) => self.array_new(elem),
                _ => continue, // GC_malloc zeros scalars, optionals, and opaques
            };
            let field = self
                .builder
                .build_struct_gep(struct_type, object, index as u32, "field")
                .unwrap();
            self.builder.build_store(field, value).unwrap();
        }
        self.builder.build_return(Some(&object)).unwrap();

        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        function
    }

    pub(super) fn lower_struct_literal(
        &mut self,
        struct_: StructId,
        fields: &[Expression],
    ) -> BasicValueEnum<'ctx> {
        let struct_type = self.struct_type(struct_);
        let object = self.gc_malloc(struct_type.size_of().unwrap(), "new");
        for (index, value) in fields.iter().enumerate() {
            let value = self.lower_expression(value);
            let ptr = self
                .builder
                .build_struct_gep(struct_type, object, index as u32, "field")
                .unwrap();
            self.builder.build_store(ptr, value).unwrap();
        }
        object.into()
    }
}
