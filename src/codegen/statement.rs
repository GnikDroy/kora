use super::{CodeGen, CodegenErr};
use crate::parser::*;

impl<'ctx> CodeGen<'ctx, '_> {
    pub(super) fn lower_statement(&mut self, stmt: &Spanned<Statement>) -> Result<(), CodegenErr> {
        match &stmt.node {
            Statement::Empty => Ok(()),
            Statement::Compound(stmts) => {
                for stmt in stmts.iter() {
                    self.lower_statement(stmt)?;
                }
                Ok(())
            }
            Statement::Simple(expr) => {
                self.lower_expression_or_void(expr)?;
                Ok(())
            }
            Statement::Let(name, typename, init) => {
                let value = self.lower_expression(init)?;
                let k_type = match typename {
                    Some(typename) => typename.clone(),
                    None => self.program.types[&init.id].clone(),
                };
                let ty = self.basic_type(&k_type, &name.span)?;
                let alloca = self.entry_alloca(ty, &name.node);
                self.builder.build_store(alloca, value).unwrap();
                let id = self
                    .program
                    .symbols
                    .symbol_id_of_declaration(name.id)
                    .unwrap();
                self.variables.insert(id, alloca);
                Ok(())
            }
            Statement::Return(expr) => {
                match expr {
                    Some(expr) => {
                        let value = self.lower_expression(expr)?;
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
                self.lower_if(cond, then_stmt, else_stmt.as_deref())
            }
            Statement::While(cond, body) => self.lower_while(cond, body),
            Statement::For(init, cond, step, body) => self.lower_for(init, cond, step, body),
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

    fn lower_if(
        &mut self,
        cond: &Spanned<Expression>,
        then_stmt: &Spanned<Statement>,
        else_stmt: Option<&Spanned<Statement>>,
    ) -> Result<(), CodegenErr> {
        let function = self.current_function.unwrap();
        let cond_value = self.lower_expression(cond)?.into_int_value();

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
        self.lower_statement(then_stmt)?;
        self.branch_if_open(merge_block);

        if let Some(else_stmt) = else_stmt {
            self.builder.position_at_end(else_block);
            self.lower_statement(else_stmt)?;
            self.branch_if_open(merge_block);
        }

        self.builder.position_at_end(merge_block);
        Ok(())
    }

    fn lower_while(
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
        let cond_value = self.lower_expression(cond)?.into_int_value();
        self.builder
            .build_conditional_branch(cond_value, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.loops.push((cond_block, after_block));
        self.lower_statement(body)?;
        self.loops.pop();
        self.branch_if_open(cond_block);

        self.builder.position_at_end(after_block);
        Ok(())
    }

    fn lower_for(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) -> Result<(), CodegenErr> {
        let function = self.current_function.unwrap();
        self.lower_statement(init)?;

        let cond_block = self.context.append_basic_block(function, "for_cond");
        let body_block = self.context.append_basic_block(function, "for_body");
        let step_block = self.context.append_basic_block(function, "for_step");
        let after_block = self.context.append_basic_block(function, "for_after");

        self.builder.build_unconditional_branch(cond_block).unwrap();
        self.builder.position_at_end(cond_block);
        let cond_value = self.lower_expression(cond)?.into_int_value();
        self.builder
            .build_conditional_branch(cond_value, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.loops.push((step_block, after_block));
        self.lower_statement(body)?;
        self.loops.pop();
        self.branch_if_open(step_block);

        self.builder.position_at_end(step_block);
        self.lower_expression_or_void(step)?;
        self.builder.build_unconditional_branch(cond_block).unwrap();

        self.builder.position_at_end(after_block);
        Ok(())
    }
}
