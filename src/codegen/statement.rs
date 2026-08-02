use super::CodeGen;
use crate::ir::{Block, Expression, Statement};

impl<'ctx, 'a> CodeGen<'ctx, 'a> {
    pub(super) fn block(&mut self, block: &Block) {
        for stmt in block {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Expression(expr) => self.lower_expression_or_void(expr),
            Statement::Let(local, init) => {
                let value = self.lower_expression(init);
                let slot = self.frame().variables[local.index()];
                self.builder.build_store(slot, value).unwrap();
            }
            Statement::Return(expr) => {
                match expr {
                    Some(expr) => {
                        let value = self.lower_expression(expr);
                        self.builder.build_return(Some(&value)).unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                }
                self.start_continuation_block();
            }
            Statement::If {
                cond,
                then,
                otherwise,
            } => self.lower_if(cond, then, otherwise.as_ref()),
            Statement::While { cond, body } => self.lower_while(cond, body),
            Statement::For {
                init,
                cond,
                step,
                body,
            } => self.lower_for(init, cond, step, body),
            Statement::Break => {
                let (_, break_target) = *self.frame().loops.last().unwrap();
                self.builder
                    .build_unconditional_branch(break_target)
                    .unwrap();
                self.start_continuation_block();
            }
            Statement::Continue => {
                let (continue_target, _) = *self.frame().loops.last().unwrap();
                self.builder
                    .build_unconditional_branch(continue_target)
                    .unwrap();
                self.start_continuation_block();
            }
        }
    }

    fn lower_if(&mut self, cond: &Expression, then: &Block, otherwise: Option<&Block>) {
        let function = self.frame().function;
        let cond_value = self.lower_expression(cond).into_int_value();

        let then_block = self.context.append_basic_block(function, "then");
        let merge_block = self.context.append_basic_block(function, "merge");
        let else_block = match otherwise {
            Some(_) => self.context.append_basic_block(function, "else"),
            None => merge_block,
        };

        self.builder
            .build_conditional_branch(cond_value, then_block, else_block)
            .unwrap();

        self.builder.position_at_end(then_block);
        self.block(then);
        self.branch_if_open(merge_block);

        if let Some(otherwise) = otherwise {
            self.builder.position_at_end(else_block);
            self.block(otherwise);
            self.branch_if_open(merge_block);
        }

        self.builder.position_at_end(merge_block);
    }

    fn lower_while(&mut self, cond: &Expression, body: &Block) {
        let function = self.frame().function;
        let cond_block = self.context.append_basic_block(function, "while_cond");
        let body_block = self.context.append_basic_block(function, "while_body");
        let after_block = self.context.append_basic_block(function, "while_after");

        self.builder.build_unconditional_branch(cond_block).unwrap();
        self.builder.position_at_end(cond_block);
        let cond_value = self.lower_expression(cond).into_int_value();
        self.builder
            .build_conditional_branch(cond_value, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.frame_mut().loops.push((cond_block, after_block));
        self.block(body);
        self.frame_mut().loops.pop();
        self.branch_if_open(cond_block);

        self.builder.position_at_end(after_block);
    }

    fn lower_for(&mut self, init: &Block, cond: &Expression, step: &Expression, body: &Block) {
        let function = self.frame().function;
        self.block(init);

        let cond_block = self.context.append_basic_block(function, "for_cond");
        let body_block = self.context.append_basic_block(function, "for_body");
        let step_block = self.context.append_basic_block(function, "for_step");
        let after_block = self.context.append_basic_block(function, "for_after");

        self.builder.build_unconditional_branch(cond_block).unwrap();
        self.builder.position_at_end(cond_block);
        let cond_value = self.lower_expression(cond).into_int_value();
        self.builder
            .build_conditional_branch(cond_value, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.frame_mut().loops.push((step_block, after_block));
        self.block(body);
        self.frame_mut().loops.pop();
        self.branch_if_open(step_block);

        self.builder.position_at_end(step_block);
        self.lower_expression_or_void(step);
        self.builder.build_unconditional_branch(cond_block).unwrap();

        self.builder.position_at_end(after_block);
    }
}
