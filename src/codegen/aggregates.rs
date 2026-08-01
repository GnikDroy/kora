use inkwell::AddressSpace;
use inkwell::types::StructType;
use inkwell::values::{FunctionValue, PointerValue};

use super::{CodeGen, CodegenErr};
use crate::parser::*;

impl<'ctx> CodeGen<'ctx, '_> {
    pub(super) fn struct_type(
        &mut self,
        name: &str,
        span: &Span,
    ) -> Result<StructType<'ctx>, CodegenErr> {
        if let Some(ty) = self.struct_types.get(name) {
            return Ok(*ty);
        }
        let members = self.program.symbols.struct_members(name).unwrap().to_vec();
        let field_types = members
            .iter()
            .map(|(_, ty)| self.basic_type(ty, span))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = self.context.opaque_struct_type(name);
        ty.set_body(&field_types, false);
        self.struct_types.insert(name.to_string(), ty);
        Ok(ty)
    }

    pub(super) fn struct_member_index(&self, struct_name: &str, member: &str) -> u32 {
        self.program
            .symbols
            .struct_members(struct_name)
            .unwrap()
            .iter()
            .position(|(name, _)| name == member)
            .unwrap() as u32
    }

    pub(super) fn struct_member_pointer(
        &mut self,
        obj: &Spanned<Expression>,
        member: &str,
        span: &Span,
    ) -> Result<PointerValue<'ctx>, CodegenErr> {
        let Type::Struct(struct_name) = self.program.types[&obj.id].clone() else {
            unreachable!("type checker rejects member access on non-structs");
        };
        let struct_type = self.struct_type(&struct_name.node, span)?;
        let index = self.struct_member_index(&struct_name.node, member);
        let obj = self.lower_expression(obj)?.into_pointer_value();
        Ok(self
            .builder
            .build_struct_gep(struct_type, obj, index, member)
            .unwrap())
    }

    pub(super) fn struct_constructor(
        &mut self,
        name: &str,
        span: &Span,
    ) -> Result<FunctionValue<'ctx>, CodegenErr> {
        if let Some(function) = self.default_fns.get(name) {
            return Ok(*function);
        }
        let struct_type = self.struct_type(name, span)?;
        let ptr = self.context.ptr_type(AddressSpace::default());
        let function =
            self.module
                .add_function(&format!("default.{name}"), ptr.fn_type(&[], false), None);
        self.default_fns.insert(name.to_string(), function);

        let saved_block = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let object = self.gc_malloc(struct_type.size_of().unwrap(), "new");
        let members = self.program.symbols.struct_members(name).unwrap().to_vec();
        for (index, (member, member_type)) in members.iter().enumerate() {
            let value = match member_type {
                Type::Struct(inner) => {
                    let inner_default = self.struct_constructor(&inner.node, span)?;
                    let call = self.builder.build_call(inner_default, &[], "new").unwrap();
                    self.call_value(call)
                }
                Type::Array(elem) => self.array_new(elem, span)?,
                _ => continue, // GC_malloc zeros scalers
            };
            let field = self
                .builder
                .build_struct_gep(struct_type, object, index as u32, member)
                .unwrap();
            self.builder.build_store(field, value).unwrap();
        }
        self.builder.build_return(Some(&object)).unwrap();

        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        Ok(function)
    }
}
