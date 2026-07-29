mod aggregates;
mod arrays;
mod errors;
mod expression;
mod link;
mod optionals;
mod statement;

#[cfg(test)]
mod tests;

pub use errors::*;
pub use link::link;

use std::collections::HashMap;

use inkwell::AddressSpace;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module as LlvmModule;
use inkwell::types::{BasicType, BasicTypeEnum, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind};

use crate::frontend::CompiledProgram;
use crate::mangle::mangle;
use crate::parser::*;
use crate::semantic_analyzer::{SymbolId, is_reference};

pub struct CodeGen<'ctx, 'a> {
    context: &'ctx Context,
    module: LlvmModule<'ctx>,
    builder: Builder<'ctx>,
    program: &'a CompiledProgram,
    functions: HashMap<SymbolId, FunctionValue<'ctx>>,
    struct_types: HashMap<String, StructType<'ctx>>,
    array_equality_fns: HashMap<Type, FunctionValue<'ctx>>, // memoized `i1 @"eq.N"(ptr, ptr)` per array type
    default_fns: HashMap<String, FunctionValue<'ctx>>,
    frame: Option<Frame<'ctx>>,
}

struct Frame<'ctx> {
    function: FunctionValue<'ctx>,
    return_type: Option<Type>,
    variables: HashMap<SymbolId, PointerValue<'ctx>>,
    loops: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
}

pub fn lower<'ctx>(
    context: &'ctx Context,
    program: &CompiledProgram,
) -> Result<LlvmModule<'ctx>, CodegenErr> {
    if let Some(import) = program
        .program
        .modules
        .iter()
        .find_map(|m| m.imports.first())
    {
        return Err(CodegenErr {
            msg: "codegen for imports is not implemented yet",
            span: import.span.clone(),
        });
    }
    let mut codegen = CodeGen {
        context,
        module: context.create_module("kora"),
        builder: context.create_builder(),
        program,
        functions: HashMap::new(),
        struct_types: HashMap::new(),
        array_equality_fns: HashMap::new(),
        default_fns: HashMap::new(),
        frame: None,
    };
    codegen.lower_module(&program.program.modules[0].module)?;
    Ok(codegen.module)
}

impl<'ctx> CodeGen<'ctx, '_> {
    fn frame(&self) -> &Frame<'ctx> {
        self.frame.as_ref().unwrap()
    }

    fn frame_mut(&mut self) -> &mut Frame<'ctx> {
        self.frame.as_mut().unwrap()
    }

    fn lower_module(&mut self, module: &Module) -> Result<(), CodegenErr> {
        for func in module.extern_functions.iter() {
            // The C ABI of a by-value struct {i8, T} is compiler/platform dependent.
            // only reference optionals (pointers) should cross C.
            // C also doesn't have optionals. extern really shouldn't be using optionals for scalars.
            let scalar_optional =
                |ty: &Type| matches!(ty, Type::Optional(inner) if !is_reference(inner));
            let arguments = func.node.arguments.iter();
            if arguments.clone().any(|p| scalar_optional(&p.node.typename))
                || func.node.return_type.as_ref().is_some_and(scalar_optional)
            {
                return Err(CodegenErr {
                    msg: "scalar optionals cannot cross the extern boundary",
                    span: func.span.clone(),
                });
            }
            drop(arguments);
            self.declare_function(
                func.id,
                &func.node.name,
                &func.node.return_type,
                &func.node.arguments,
            )?;
        }
        for func in module.functions.iter() {
            if func.node.name == "main" {
                let signature_ok =
                    func.node.return_type == Some(Type::Int) && func.node.arguments.is_empty();
                if !signature_ok {
                    return Err(CodegenErr {
                        msg: "main must be declared as `int main()`",
                        span: func.span.clone(),
                    });
                }
            }
            self.declare_function(
                func.id,
                &func.node.name,
                &func.node.return_type,
                &func.node.arguments,
            )?;
        }
        for impl_ in module.impls.iter() {
            for func in impl_.node.functions.iter() {
                self.declare_function(
                    func.id,
                    &mangle(&impl_.node.struct_name.node, &func.node.name),
                    &func.node.return_type,
                    &func.node.arguments,
                )?;
            }
        }
        for func in module.functions.iter() {
            self.lower_function(func)?;
        }
        for impl_ in module.impls.iter() {
            for func in impl_.node.functions.iter() {
                self.lower_function(func)?;
            }
        }
        Ok(())
    }

    fn declare_function(
        &mut self,
        declaration_id: NodeId,
        name: &str,
        return_type: &Option<Type>,
        arguments: &[Spanned<IdentifierTypePair>],
    ) -> Result<FunctionValue<'ctx>, CodegenErr> {
        let param_types = arguments
            .iter()
            .map(|pair| {
                self.basic_type(&pair.node.typename, &pair.span)
                    .map(Into::into)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let function_type = match return_type {
            Some(ty) => {
                let span = arguments
                    .first()
                    .map(|pair| pair.span.clone())
                    .unwrap_or_default();
                self.basic_type(ty, &span)?.fn_type(&param_types, false)
            }
            None => self.context.void_type().fn_type(&param_types, false),
        };

        let llvm_name = if name == "main" { "__kora_main" } else { name };
        let function = self.module.add_function(llvm_name, function_type, None);
        let id = self
            .program
            .symbols
            .symbol_id_of_declaration(declaration_id)
            .unwrap();
        self.functions.insert(id, function);
        Ok(function)
    }

    fn lower_function(&mut self, func: &Spanned<Function>) -> Result<(), CodegenErr> {
        let id = self
            .program
            .symbols
            .symbol_id_of_declaration(func.id)
            .unwrap();
        let function = self.functions[&id];
        self.frame = Some(Frame {
            function,
            return_type: func.node.return_type.clone(),
            variables: HashMap::new(),
            loops: Vec::new(),
        });

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        for (i, pair) in func.node.arguments.iter().enumerate() {
            let ty = self.basic_type(&pair.node.typename, &pair.span)?;
            let alloca = self.entry_alloca(ty, &pair.node.name);
            let value = function.get_nth_param(i as u32).unwrap();
            self.builder.build_store(alloca, value).unwrap();
            let id = self
                .program
                .symbols
                .symbol_id_of_declaration(pair.id)
                .unwrap();
            self.frame_mut().variables.insert(id, alloca);
        }

        self.lower_statement(&func.node.statement)?;

        // The return checker guarantees non-void functions return on every branch
        let block = self.builder.get_insert_block().unwrap();
        if block.get_terminator().is_none() {
            match func.node.return_type {
                None => self.builder.build_return(None).unwrap(),
                Some(_) => self.builder.build_unreachable().unwrap(),
            };
        }
        Ok(())
    }

    fn basic_type(&self, ty: &Type, span: &Span) -> Result<BasicTypeEnum<'ctx>, CodegenErr> {
        match ty {
            Type::Int => Ok(self.context.i64_type().into()),
            Type::Real => Ok(self.context.f64_type().into()),
            Type::Bool => Ok(self.context.bool_type().into()),
            Type::Char => Ok(self.context.i8_type().into()),
            Type::Array(_) => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Type::Optional(inner) => {
                if is_reference(inner) {
                    return Ok(self.context.ptr_type(AddressSpace::default()).into());
                }
                let inner = self.basic_type(inner, span)?;
                let tag = self.context.i8_type().into();
                Ok(self.context.struct_type(&[tag, inner], false).into())
            }
            Type::Struct(_) => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Type::Function(_, _) => Err(CodegenErr {
                msg: "functions cannot be used as values",
                span: span.clone(),
            }),
        }
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
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => value.into_pointer_value(),
            ValueKind::Instruction(_) => unreachable!("GC_malloc returns a pointer"),
        }
    }

    fn panic_if(&mut self, failed: IntValue<'ctx>, message: &'static str) {
        let function = self.frame().function;
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
            self.module.add_function("__kora_panic", ty, None)
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

    pub(super) fn type_layout(
        &mut self,
        ty: &Type,
        span: &Span,
    ) -> Result<(BasicTypeEnum<'ctx>, IntValue<'ctx>), CodegenErr> {
        let ty = self.basic_type(ty, span)?;
        Ok((ty, ty.size_of().unwrap()))
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
