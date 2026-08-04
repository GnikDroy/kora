mod aggregates;
mod arrays;
mod errors;
mod expression;
mod externs;
mod link;
mod optionals;
mod statement;

#[cfg(test)]
mod tests;

pub use errors::*;
pub use link::link;

use std::collections::HashMap;

use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module as LlvmModule;
use inkwell::types::{BasicType, BasicTypeEnum, StructType};
use inkwell::values::{
    BasicValueEnum, CallSiteValue, FunctionValue, IntValue, PointerValue, ValueKind,
};

use crate::frontend::CompiledProgram;
use crate::ir::{self, ExternId, FunctionDef, FunctionId, Program, StructId, Type, TypeId};

pub struct CodeGen<'ctx, 'a> {
    context: &'ctx Context,
    module: LlvmModule<'ctx>,
    builder: Builder<'ctx>,
    program: &'a Program,
    struct_types: HashMap<StructId, StructType<'ctx>>,
    array_type: Option<StructType<'ctx>>,
    array_equality_fns: HashMap<TypeId, FunctionValue<'ctx>>, // memoized `i1 @"eq.N"(ptr, ptr)` per array type
    default_fns: HashMap<StructId, FunctionValue<'ctx>>,
    frame: Option<Frame<'ctx>>,
}

struct Frame<'ctx> {
    function: FunctionValue<'ctx>,
    return_type: TypeId,
    variables: Vec<PointerValue<'ctx>>, // indexed by LocalId
    loops: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
}

pub fn lower<'ctx>(
    context: &'ctx Context,
    compiled: &CompiledProgram,
) -> Result<LlvmModule<'ctx>, CodegenErr> {
    let program = ir::lower(compiled);
    let mut codegen = CodeGen {
        context,
        module: context.create_module("kora"),
        builder: context.create_builder(),
        program: &program,
        struct_types: HashMap::new(),
        array_type: None,
        array_equality_fns: HashMap::new(),
        default_fns: HashMap::new(),
        frame: None,
    };
    codegen.run();
    Ok(codegen.module)
}

impl<'ctx, 'a> CodeGen<'ctx, 'a> {
    fn run(&mut self) {
        for ext in &self.program.externs {
            self.declare_extern_function(ext);
        }
        for func in &self.program.functions {
            self.declare_function(func);
        }
        for func in &self.program.functions {
            self.lower_function(func);
        }
    }

    fn frame(&self) -> &Frame<'ctx> {
        self.frame.as_ref().unwrap()
    }

    fn frame_mut(&mut self) -> &mut Frame<'ctx> {
        self.frame.as_mut().unwrap()
    }

    /// The LLVM module is the function symbol table; every function is declared
    /// under its final emitted symbol, so calls resolve by name.
    pub(super) fn function_value(&self, id: FunctionId) -> FunctionValue<'ctx> {
        self.module.get_function(&self.program[id].symbol).unwrap()
    }

    /// An extern's callable is its thunk when one was emitted, else the raw C
    /// function under its own name.
    pub(super) fn extern_value(&self, id: ExternId) -> FunctionValue<'ctx> {
        let symbol = &self.program[id].symbol;
        self.module
            .get_function(&format!("{symbol}.thunk"))
            .unwrap_or_else(|| self.module.get_function(symbol).unwrap())
    }

    fn declare_function(&mut self, func: &FunctionDef) {
        let param_types = func.locals[..func.params]
            .iter()
            .map(|local| self.basic_type(local.ty).into())
            .collect::<Vec<_>>();
        let function_type = match self.program.types[func.ret] {
            Type::Void => self.context.void_type().fn_type(&param_types, false),
            _ => self.basic_type(func.ret).fn_type(&param_types, false),
        };
        // A symbol is declared once; get-or-add stays defensive against dupes.
        if self.module.get_function(&func.symbol).is_none() {
            self.module.add_function(&func.symbol, function_type, None);
        }
    }

    fn lower_function(&mut self, func: &'a FunctionDef) {
        let function = self.module.get_function(&func.symbol).unwrap();
        self.frame = Some(Frame {
            function,
            return_type: func.ret,
            variables: Vec::new(),
            loops: Vec::new(),
        });

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let mut variables = Vec::with_capacity(func.locals.len());
        for local in &func.locals {
            let ty = self.basic_type(local.ty);
            variables.push(self.entry_alloca(ty, &local.name));
        }
        for (i, slot) in variables.iter().take(func.params).enumerate() {
            let value = function.get_nth_param(i as u32).unwrap();
            self.builder.build_store(*slot, value).unwrap();
        }
        self.frame_mut().variables = variables;

        self.block(&func.body);

        // The return checker guarantees non-void functions return on every branch.
        let block = self.builder.get_insert_block().unwrap();
        if block.get_terminator().is_none() {
            match self.program.types[func.ret] {
                Type::Void => self.builder.build_return(None).unwrap(),
                _ => self.builder.build_unreachable().unwrap(),
            };
        }
    }

    fn basic_type(&self, ty: TypeId) -> BasicTypeEnum<'ctx> {
        match self.program.types[ty] {
            Type::Int => self.context.i64_type().into(),
            Type::Real => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::Char => self.context.i8_type().into(),
            Type::Opaque | Type::Array(_) | Type::Struct(_) | Type::Fn => {
                self.context.ptr_type(AddressSpace::default()).into()
            }
            Type::Optional(inner) => {
                if self.is_reference(inner) {
                    return self.context.ptr_type(AddressSpace::default()).into();
                }
                let inner = self.basic_type(inner);
                let tag = self.context.i8_type().into();
                self.context.struct_type(&[tag, inner], false).into()
            }
            Type::Void => unreachable!("void is not a value type"),
        }
    }

    fn is_reference(&self, ty: TypeId) -> bool {
        matches!(
            self.program.types[ty],
            Type::Array(_) | Type::Struct(_) | Type::Opaque
        )
    }

    pub(super) fn pointers_equal(
        &self,
        a: PointerValue<'ctx>,
        b: PointerValue<'ctx>,
    ) -> IntValue<'ctx> {
        let ty = self.context.i64_type();
        let a = self.builder.build_ptr_to_int(a, ty, "a_addr").unwrap();
        let b = self.builder.build_ptr_to_int(b, ty, "b_addr").unwrap();
        self.builder
            .build_int_compare(IntPredicate::EQ, a, b, "ptr_eq")
            .unwrap()
    }

    fn gc_malloc(&mut self, size: IntValue<'ctx>, name: &str) -> PointerValue<'ctx> {
        let malloc = self.module.get_function("GC_malloc").unwrap_or_else(|| {
            let ptr = self.context.ptr_type(AddressSpace::default());
            let ty = ptr.fn_type(&[self.context.i64_type().into()], false);
            self.module.add_function("GC_malloc", ty, None)
        });
        let call = self
            .builder
            .build_call(malloc, &[size.into()], name)
            .unwrap();
        self.call_value(call).into_pointer_value()
    }

    fn call_value(&self, call: CallSiteValue<'ctx>) -> BasicValueEnum<'ctx> {
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => value,
            ValueKind::Instruction(_) => unreachable!("the callee returns a value"),
        }
    }

    fn panic_if(&mut self, failed: IntValue<'ctx>, message: &'static str) {
        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();
        let panic_block = self.context.append_basic_block(function, "panic");
        let cont_block = self.context.append_basic_block(function, "cont");
        self.builder
            .build_conditional_branch(failed, panic_block, cont_block)
            .unwrap();
        self.builder.position_at_end(panic_block);
        self.build_panic(message);
        self.builder.position_at_end(cont_block);
    }

    fn build_panic(&mut self, message: &str) {
        let panic_fn = self.module.get_function("__kora_panic").unwrap_or_else(|| {
            let ptr = self.context.ptr_type(AddressSpace::default());
            let ty = self.context.void_type().fn_type(&[ptr.into()], false);
            let function = self.module.add_function("__kora_panic", ty, None);
            let noreturn = Attribute::get_named_enum_kind_id("noreturn");
            function.add_attribute(
                AttributeLoc::Function,
                self.context.create_enum_attribute(noreturn, 0),
            );
            function
        });
        let message = self
            .builder
            .build_global_string_ptr(message, "panic_msg")
            .unwrap();
        self.builder
            .build_call(panic_fn, &[message.as_pointer_value().into()], "")
            .unwrap();
        self.builder.build_unreachable().unwrap();
    }

    pub(super) fn spill(&mut self, value: BasicValueEnum<'ctx>) -> PointerValue<'ctx> {
        let slot = self.entry_alloca(value.get_type(), "spill");
        self.builder.build_store(slot, value).unwrap();
        slot
    }

    pub(super) fn type_layout(&mut self, ty: TypeId) -> (BasicTypeEnum<'ctx>, IntValue<'ctx>) {
        let ty = self.basic_type(ty);
        (ty, ty.size_of().unwrap())
    }

    fn entry_alloca(&self, ty: BasicTypeEnum<'ctx>, name: &str) -> PointerValue<'ctx> {
        let entry = self.frame().function.get_first_basic_block().unwrap();
        let builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(instruction) => builder.position_before(&instruction),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(ty, name).unwrap()
    }

    /// After return/break/continue, statements are unreachable but must compile.
    fn start_continuation_block(&mut self) {
        let function = self.frame().function;
        let block = self.context.append_basic_block(function, "unreachable");
        self.builder.position_at_end(block);
    }

    fn branch_if_open(&self, target: BasicBlock<'ctx>) {
        let block = self.builder.get_insert_block().unwrap();
        if block.get_terminator().is_none() {
            self.builder.build_unconditional_branch(target).unwrap();
        }
    }
}
