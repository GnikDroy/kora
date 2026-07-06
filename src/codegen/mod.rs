mod errors;

pub use errors::*;

use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module as LlvmModule;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue, ValueKind};
use inkwell::{FloatPredicate, IntPredicate};

use crate::frontend::CompiledProgram;
use crate::parser::*;
use crate::semantic_analyzer::{SymbolId, SymbolTable, is_intrinsic};

pub struct CodeGen<'ctx, 'a> {
    context: &'ctx Context,
    module: LlvmModule<'ctx>,
    builder: Builder<'ctx>,
    symbols: &'a SymbolTable,
    types: &'a HashMap<NodeId, Type>,
    variables: HashMap<SymbolId, PointerValue<'ctx>>,
    functions: HashMap<SymbolId, FunctionValue<'ctx>>,
    // (continue target, break target) per enclosing loop
    loops: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    current_function: Option<FunctionValue<'ctx>>,
}

pub fn compile<'ctx>(
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
        symbols: &program.symbols,
        types: &program.types,
        variables: HashMap::new(),
        functions: HashMap::new(),
        loops: Vec::new(),
        current_function: None,
    };
    codegen.compile_module(&program.program.modules[0].module)?;
    Ok(codegen.module)
}

impl<'ctx> CodeGen<'ctx, '_> {
    fn compile_module(&mut self, module: &Module) -> Result<(), CodegenErr> {
        if let Some(imp) = module.impls.first() {
            return Err(CodegenErr {
                msg: "codegen for methods is not implemented yet",
                span: imp.span.clone(),
            });
        }
        for func in module.extern_functions.iter() {
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
        for func in module.functions.iter() {
            self.compile_function(func)?;
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

        let llvm_name = if name == "main" { "k_main" } else { name };
        let function = self.module.add_function(llvm_name, function_type, None);
        let id = self
            .symbols
            .symbol_id_of_declaration(declaration_id)
            .unwrap();
        self.functions.insert(id, function);
        Ok(function)
    }

    fn compile_function(&mut self, func: &Spanned<Function>) -> Result<(), CodegenErr> {
        let id = self.symbols.symbol_id_of_declaration(func.id).unwrap();
        let function = self.functions[&id];
        self.current_function = Some(function);

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        for (i, pair) in func.node.arguments.iter().enumerate() {
            let ty = self.basic_type(&pair.node.typename, &pair.span)?;
            let alloca = self.entry_alloca(ty, &pair.node.name);
            let value = function.get_nth_param(i as u32).unwrap();
            self.builder.build_store(alloca, value).unwrap();
            let id = self.symbols.symbol_id_of_declaration(pair.id).unwrap();
            self.variables.insert(id, alloca);
        }

        self.compile_statement(&func.node.statement)?;

        // The return checker guarantees non-void functions return on every
        // real path; anything still open here is a void fall-through or an
        // unreachable continuation block.
        let block = self.builder.get_insert_block().unwrap();
        if block.get_terminator().is_none() {
            match func.node.return_type {
                None => self.builder.build_return(None).unwrap(),
                Some(_) => self.builder.build_unreachable().unwrap(),
            };
        }
        Ok(())
    }

    fn compile_statement(&mut self, stmt: &Spanned<Statement>) -> Result<(), CodegenErr> {
        match &stmt.node {
            Statement::Empty => Ok(()),
            Statement::Compound(stmts) => {
                for stmt in stmts.iter() {
                    self.compile_statement(stmt)?;
                }
                Ok(())
            }
            Statement::Simple(expr) => {
                self.compile_expression_or_void(expr)?;
                Ok(())
            }
            Statement::Let(name, typename, init) => {
                let value = self.compile_expression(init)?;
                let k_type = match typename {
                    Some(typename) => typename.clone(),
                    None => self.types[&init.id].clone(),
                };
                let ty = self.basic_type(&k_type, &name.span)?;
                let alloca = self.entry_alloca(ty, &name.node);
                self.builder.build_store(alloca, value).unwrap();
                let id = self.symbols.symbol_id_of_declaration(name.id).unwrap();
                self.variables.insert(id, alloca);
                Ok(())
            }
            Statement::Return(expr) => {
                match expr {
                    Some(expr) => {
                        let value = self.compile_expression(expr)?;
                        self.builder.build_return(Some(&value)).unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                }
                self.start_continuation_block();
                Ok(())
            }
            Statement::If(cond, then_stmt, else_stmt) => {
                self.compile_if(cond, then_stmt, else_stmt.as_deref())
            }
            Statement::While(cond, body) => self.compile_while(cond, body),
            Statement::For(init, cond, step, body) => self.compile_for(init, cond, step, body),
            Statement::Break => {
                let (_, break_target) = *self.loops.last().unwrap();
                self.builder
                    .build_unconditional_branch(break_target)
                    .unwrap();
                self.start_continuation_block();
                Ok(())
            }
            Statement::Continue => {
                let (continue_target, _) = *self.loops.last().unwrap();
                self.builder
                    .build_unconditional_branch(continue_target)
                    .unwrap();
                self.start_continuation_block();
                Ok(())
            }
        }
    }

    fn compile_if(
        &mut self,
        cond: &Spanned<Expression>,
        then_stmt: &Spanned<Statement>,
        else_stmt: Option<&Spanned<Statement>>,
    ) -> Result<(), CodegenErr> {
        let function = self.current_function.unwrap();
        let cond_value = self.compile_expression(cond)?.into_int_value();

        let then_block = self.context.append_basic_block(function, "then");
        let merge_block = self.context.append_basic_block(function, "merge");
        let else_block = match else_stmt {
            Some(_) => self.context.append_basic_block(function, "else"),
            None => merge_block,
        };

        self.builder
            .build_conditional_branch(cond_value, then_block, else_block)
            .unwrap();

        self.builder.position_at_end(then_block);
        self.compile_statement(then_stmt)?;
        self.branch_if_open(merge_block);

        if let Some(else_stmt) = else_stmt {
            self.builder.position_at_end(else_block);
            self.compile_statement(else_stmt)?;
            self.branch_if_open(merge_block);
        }

        self.builder.position_at_end(merge_block);
        Ok(())
    }

    fn compile_while(
        &mut self,
        cond: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) -> Result<(), CodegenErr> {
        let function = self.current_function.unwrap();
        let cond_block = self.context.append_basic_block(function, "while_cond");
        let body_block = self.context.append_basic_block(function, "while_body");
        let after_block = self.context.append_basic_block(function, "while_after");

        self.builder.build_unconditional_branch(cond_block).unwrap();
        self.builder.position_at_end(cond_block);
        let cond_value = self.compile_expression(cond)?.into_int_value();
        self.builder
            .build_conditional_branch(cond_value, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.loops.push((cond_block, after_block));
        self.compile_statement(body)?;
        self.loops.pop();
        self.branch_if_open(cond_block);

        self.builder.position_at_end(after_block);
        Ok(())
    }

    fn compile_for(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) -> Result<(), CodegenErr> {
        let function = self.current_function.unwrap();
        self.compile_statement(init)?;

        let cond_block = self.context.append_basic_block(function, "for_cond");
        let body_block = self.context.append_basic_block(function, "for_body");
        let step_block = self.context.append_basic_block(function, "for_step");
        let after_block = self.context.append_basic_block(function, "for_after");

        self.builder.build_unconditional_branch(cond_block).unwrap();
        self.builder.position_at_end(cond_block);
        let cond_value = self.compile_expression(cond)?.into_int_value();
        self.builder
            .build_conditional_branch(cond_value, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.loops.push((step_block, after_block));
        self.compile_statement(body)?;
        self.loops.pop();
        self.branch_if_open(step_block);

        self.builder.position_at_end(step_block);
        self.compile_expression_or_void(step)?;
        self.builder.build_unconditional_branch(cond_block).unwrap();

        self.builder.position_at_end(after_block);
        Ok(())
    }

    fn compile_expression(
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
                let id = self.symbols.symbol_id_of_use(expr.id).unwrap();
                let alloca = self.variables.get(&id).ok_or(CodegenErr {
                    msg: "functions cannot be used as values",
                    span: span.clone(),
                })?;
                let ty = self.basic_type(&self.types[&expr.id], span)?;
                Ok(self.builder.build_load(ty, *alloca, name).unwrap())
            }
            Expression::Binary(left, BinaryOp::Assign, right) => {
                let target = self.compile_lvalue(left)?;
                let value = self.compile_expression(right)?;
                self.builder.build_store(target, value).unwrap();
                Ok(value)
            }
            Expression::Binary(left, op @ (BinaryOp::And | BinaryOp::Or), right) => {
                self.compile_short_circuit(left, *op, right)
            }
            Expression::Binary(left, op, right) => self.compile_binary(left, *op, right, span),
            Expression::Unary(op, operand) => {
                let value = self.compile_expression(operand)?;
                let result: BasicValueEnum = match (op, &self.types[&operand.id]) {
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
            Expression::Cast(operand, target) => self.compile_cast(operand, target),
            Expression::Call(callee, args) => Ok(self
                .compile_call(callee, args, span)?
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
            Expression::Access(_, _)
            | Expression::Construct(_, _)
            | Expression::StructLiteral(_, _) => Err(CodegenErr {
                msg: "codegen for structs is not implemented yet",
                span: span.clone(),
            }),
        }
    }

    /// An expression in statement position, where a void call is legal.
    fn compile_expression_or_void(&mut self, expr: &Spanned<Expression>) -> Result<(), CodegenErr> {
        match &expr.node {
            Expression::Call(callee, args) => {
                self.compile_call(callee, args, &expr.span)?;
                Ok(())
            }
            _ => self.compile_expression(expr).map(|_| ()),
        }
    }

    fn compile_lvalue(
        &mut self,
        expr: &Spanned<Expression>,
    ) -> Result<PointerValue<'ctx>, CodegenErr> {
        match &expr.node {
            Expression::Identifier(_) => {
                let id = self.symbols.symbol_id_of_use(expr.id).unwrap();
                Ok(self.variables[&id])
            }
            Expression::ArrayIndex(_, _) => Err(CodegenErr {
                msg: "codegen for arrays is not implemented yet",
                span: expr.span.clone(),
            }),
            Expression::Access(_, _) => Err(CodegenErr {
                msg: "codegen for structs is not implemented yet",
                span: expr.span.clone(),
            }),
            _ => unreachable!("type checker rejects other assignment targets"),
        }
    }

    fn compile_binary(
        &mut self,
        left: &Spanned<Expression>,
        op: BinaryOp,
        right: &Spanned<Expression>,
        span: &Span,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        use BinaryOp::*;

        let operand_type = self.types[&left.id].clone();
        let lhs = self.compile_expression(left)?;
        let rhs = self.compile_expression(right)?;

        let value: BasicValueEnum = match operand_type {
            Type::Int | Type::Char | Type::Bool => {
                let l = lhs.into_int_value();
                let r = rhs.into_int_value();
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

    fn compile_short_circuit(
        &mut self,
        left: &Spanned<Expression>,
        op: BinaryOp,
        right: &Spanned<Expression>,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        let function = self.current_function.unwrap();
        let lhs = self.compile_expression(left)?.into_int_value();
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
        let rhs = self.compile_expression(right)?.into_int_value();
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

    fn compile_cast(
        &mut self,
        operand: &Spanned<Expression>,
        target: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodegenErr> {
        use Type::*;
        let from = self.types[&operand.id].clone();
        let value = self.compile_expression(operand)?;
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

    fn compile_call(
        &mut self,
        callee: &Spanned<Expression>,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodegenErr> {
        let Expression::Identifier(name) = &callee.node else {
            return Err(CodegenErr {
                msg: "codegen for method calls is not implemented yet",
                span: span.clone(),
            });
        };
        if is_intrinsic(name) {
            return Err(CodegenErr {
                msg: "codegen for the copy intrinsic is not implemented yet",
                span: span.clone(),
            });
        }

        let id = self.symbols.symbol_id_of_use(callee.id).unwrap();
        let function = self.functions[&id];

        let arg_values = args
            .iter()
            .map(|arg| self.compile_expression(arg).map(Into::into))
            .collect::<Result<Vec<_>, _>>()?;

        let call = self.builder.build_call(function, &arg_values, "").unwrap();
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Ok(Some(value)),
            ValueKind::Instruction(_) => Ok(None),
        }
    }

    fn basic_type(&self, ty: &Type, span: &Span) -> Result<BasicTypeEnum<'ctx>, CodegenErr> {
        match ty {
            Type::Int => Ok(self.context.i64_type().into()),
            Type::Real => Ok(self.context.f64_type().into()),
            Type::Bool => Ok(self.context.bool_type().into()),
            Type::Char => Ok(self.context.i8_type().into()),
            Type::Array(_) => Err(CodegenErr {
                msg: "codegen for arrays is not implemented yet",
                span: span.clone(),
            }),
            Type::Optional(_) => Err(CodegenErr {
                msg: "codegen for optionals is not implemented yet",
                span: span.clone(),
            }),
            Type::Struct(_) => Err(CodegenErr {
                msg: "codegen for structs is not implemented yet",
                span: span.clone(),
            }),
            Type::Function(_, _) => Err(CodegenErr {
                msg: "functions cannot be used as values",
                span: span.clone(),
            }),
        }
    }

    /// Allocas live in the entry block so LLVM's mem2reg can promote them.
    fn entry_alloca(&self, ty: BasicTypeEnum<'ctx>, name: &str) -> PointerValue<'ctx> {
        let entry = self
            .current_function
            .unwrap()
            .get_first_basic_block()
            .unwrap();
        let builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(instruction) => builder.position_before(&instruction),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(ty, name).unwrap()
    }

    /// After `return`/`break`/`continue`, later statements in the block are
    /// unreachable but must still compile somewhere.
    fn start_continuation_block(&mut self) {
        let function = self.current_function.unwrap();
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

#[cfg(test)]
mod tests {
    use super::compile;
    use inkwell::OptimizationLevel;
    use inkwell::context::Context;
    use std::path::Path;

    fn run_main(source: &str) -> i64 {
        let program = crate::compile("main.kora", |path: &Path| {
            (path == Path::new("main.kora")).then(|| source.to_string())
        })
        .expect("front-end");

        let context = Context::create();
        let llvm = compile(&context, &program).expect("codegen");
        llvm.verify()
            .unwrap_or_else(|e| panic!("invalid IR:\n{}\n{}", llvm.print_to_string(), e));

        let engine = llvm
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("jit");
        unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i64>("k_main")
                .expect("k_main")
                .call()
        }
    }

    #[test]
    fn test_arithmetic() {
        assert_eq!(run_main("int main() { return 2 + 3 * 4 - 10 / 2; }"), 9);
        assert_eq!(run_main("int main() { return 17 % 5; }"), 2);
        assert_eq!(run_main("int main() { return -(3 - 5); }"), 2);
    }

    #[test]
    fn test_real_arithmetic() {
        assert_eq!(
            run_main("int main() { return (1.5 * 4.0 + 1.0 / 2.0) as int; }"),
            6
        );
    }

    #[test]
    fn test_variables_and_assignment() {
        assert_eq!(
            run_main("int main() { let a: int = 3; a = a + 4; return a; }"),
            7
        );
        assert_eq!(
            run_main("int main() { let a: int = 0; let b: int = 0; a = b = 5; return a + b; }"),
            10
        );
    }

    #[test]
    fn test_shadowing() {
        let source = r#"
            int main() {
                let x: int = 1;
                if (true) {
                    let x: int = 100;
                    x = x + 1;
                }
                return x;
            }
        "#;
        assert_eq!(run_main(source), 1);
    }

    #[test]
    fn test_if_else() {
        let source = r#"
            int max(a: int, b: int) {
                if (a > b) { return a; } else { return b; }
            }
            int main() { return max(3, 11) + max(7, 2); }
        "#;
        assert_eq!(run_main(source), 18);
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
            int main() {
                let a: int = 0;
                let b: int = 1;
                let i: int = 0;
                while (i < 10) {
                    let next: int = a + b;
                    a = b;
                    b = next;
                    i = i + 1;
                }
                return a;
            }
        "#;
        assert_eq!(run_main(source), 55);
    }

    #[test]
    fn test_recursion() {
        let source = r#"
            int fib(n: int) {
                if (n < 2) { return n; }
                return fib(n - 1) + fib(n - 2);
            }
            int main() { return fib(10); }
        "#;
        assert_eq!(run_main(source), 55);
    }

    #[test]
    fn test_for_break_continue() {
        let source = r#"
            int main() {
                let sum = 0;
                for (let i = 0; i < 100; i = i + 1) {
                    if (i % 2 == 0) { continue; }
                    if (i > 9) { break; }
                    sum = sum + i;
                }
                return sum;
            }
        "#;
        assert_eq!(run_main(source), 25);
    }

    #[test]
    fn test_bitwise() {
        assert_eq!(run_main("int main() { return 12 & 10; }"), 8);
        assert_eq!(run_main("int main() { return 12 | 10; }"), 14);
        assert_eq!(run_main("int main() { return 12 ^ 10; }"), 6);
        assert_eq!(run_main("int main() { return 3 << 4; }"), 48);
        assert_eq!(run_main("int main() { return -16 >> 2; }"), -4);
    }

    #[test]
    fn test_casts() {
        assert_eq!(run_main("int main() { return 2.9 as int; }"), 2);
        assert_eq!(run_main("int main() { return 'a' as int; }"), 97);
        assert_eq!(
            run_main("int main() { return (('a' as int + 1) as char) as int; }"),
            98
        );
        assert_eq!(run_main("int main() { return (1 as real) as int; }"), 1);
    }

    #[test]
    fn test_bool_logic() {
        let source = r#"
            int main() {
                let t: bool = 1 < 2 && 3 != 4;
                let f: bool = t && false;
                if (t || f) {
                    if (!f) { return 1; }
                }
                return 0;
            }
        "#;
        assert_eq!(run_main(source), 1);
    }

    #[test]
    fn test_short_circuit_skips_rhs() {
        // The rhs recursion would overflow the stack if && didn't short-circuit.
        let source = r#"
            bool diverge() { return diverge(); }
            int main() {
                if (false && diverge()) { return 1; }
                if (true || diverge()) { return 2; }
                return 3;
            }
        "#;
        assert_eq!(run_main(source), 2);
    }

    #[test]
    fn test_char_comparisons() {
        let source = r#"
            int main() {
                if ('a' < 'b' && 'z' > 'y' && 'c' == 'c') { return 1; }
                return 0;
            }
        "#;
        assert_eq!(run_main(source), 1);
    }

    #[test]
    fn test_void_function_call() {
        let source = r#"
            void nop() { }
            void maybe_return(a: bool) {
                if (a) { return; }
            }
            int main() { nop(); maybe_return(true); return 7; }
        "#;
        assert_eq!(run_main(source), 7);
    }

    #[test]
    fn test_let_inference() {
        let source = r#"
            int main() {
                let a = 3;
                let b = a * 4;
                let r = 1.5 + 2.5;
                let c = 'a';
                let big = b > a && r == 4.0;
                if (big) { return b + (r as int) + (c as int); }
                return 0;
            }
        "#;
        assert_eq!(run_main(source), 12 + 4 + 97);
    }

    #[test]
    fn test_dead_code_after_return() {
        let source = r#"
            int main() {
                while (true) {
                    return 4;
                }
                return 5;
            }
        "#;
        assert_eq!(run_main(source), 4);
    }
}
