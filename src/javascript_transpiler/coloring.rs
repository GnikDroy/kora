use std::collections::HashSet;

use crate::ir::{Block, Expression, ExpressionKind, Program, Statement};

/// JavaScript is a colored language ;)
/// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
pub(crate) fn resolve_async_fns(program: &Program, async_externs: HashSet<String>) -> HashSet<String> {
    let mut async_fns = async_externs;

    let callees: Vec<(String, HashSet<String>)> = program
        .functions
        .iter()
        .map(|f| {
            let mut names = HashSet::new();
            collect_block(program, &f.body, &mut names);
            (f.symbol.clone(), names)
        })
        .collect();

    loop {
        let mut changed = false;
        for (name, called) in &callees {
            if async_fns.contains(name) {
                continue;
            }
            if called.iter().any(|c| async_fns.contains(c)) {
                async_fns.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    async_fns
}

fn collect_block(program: &Program, block: &Block, out: &mut HashSet<String>) {
    for stmt in block {
        collect_stmt(program, stmt, out);
    }
}

fn collect_stmt(program: &Program, stmt: &Statement, out: &mut HashSet<String>) {
    match stmt {
        Statement::Let(_, e) | Statement::Expression(e) => collect_expr(program, e, out),
        Statement::Return(e) => {
            if let Some(e) = e {
                collect_expr(program, e, out);
            }
        }
        Statement::Break | Statement::Continue => {}
        Statement::While { cond, body } => {
            collect_expr(program, cond, out);
            collect_block(program, body, out);
        }
        Statement::For {
            init,
            cond,
            step,
            body,
        } => {
            collect_block(program, init, out);
            collect_expr(program, cond, out);
            collect_expr(program, step, out);
            collect_block(program, body, out);
        }
        Statement::If {
            cond,
            then,
            otherwise,
        } => {
            collect_expr(program, cond, out);
            collect_block(program, then, out);
            if let Some(otherwise) = otherwise {
                collect_block(program, otherwise, out);
            }
        }
    }
}

fn collect_expr(program: &Program, expr: &Expression, out: &mut HashSet<String>) {
    match &expr.kind {
        ExpressionKind::Call { function, args } => {
            out.insert(program[*function].symbol.clone());
            for arg in args {
                collect_expr(program, arg, out);
            }
        }
        ExpressionKind::CallExtern { function, args } => {
            out.insert(program[*function].symbol.clone());
            for arg in args {
                collect_expr(program, arg, out);
            }
        }
        ExpressionKind::Array(items) => {
            for item in items {
                collect_expr(program, item, out);
            }
        }
        ExpressionKind::StructLit { fields, .. } => {
            for field in fields {
                collect_expr(program, field, out);
            }
        }
        ExpressionKind::ArrayOp { receiver, args, .. } => {
            collect_expr(program, receiver, out);
            for arg in args {
                collect_expr(program, arg, out);
            }
        }
        ExpressionKind::Field { object, .. } => collect_expr(program, object, out),
        ExpressionKind::Index { array, index } => {
            collect_expr(program, array, out);
            collect_expr(program, index, out);
        }
        ExpressionKind::Assign { place, value } => {
            collect_place(program, place, out);
            collect_expr(program, value, out);
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::And(left, right)
        | ExpressionKind::Or(left, right) => {
            collect_expr(program, left, out);
            collect_expr(program, right, out);
        }
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { operand, .. }
        | ExpressionKind::Copy(operand)
        | ExpressionKind::Wrap(operand)
        | ExpressionKind::Unwrap(operand) => collect_expr(program, operand, out),
        ExpressionKind::ArrayNew { len } => collect_expr(program, len, out),
        ExpressionKind::Int(_)
        | ExpressionKind::Real(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Str(_)
        | ExpressionKind::None
        | ExpressionKind::Local(_)
        | ExpressionKind::DefaultStruct(_) => {}
    }
}

fn collect_place(program: &Program, place: &crate::ir::Place, out: &mut HashSet<String>) {
    use crate::ir::PlaceKind;
    match &place.kind {
        PlaceKind::Local(_) => {}
        PlaceKind::Field { object, .. } => collect_place(program, object, out),
        PlaceKind::Index { array, index } => {
            collect_place(program, array, out);
            collect_expr(program, index, out);
        }
    }
}
