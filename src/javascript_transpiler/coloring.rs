use std::collections::HashSet;

use crate::ir::{Block, Expression, ExpressionKind, Program, Statement};

/// JavaScript is a colored language ;)
/// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
///
/// A function is async if it (transitively) calls an async extern, or value
/// We start with host declared async externs and include every indirect function call
/// that can possibly be async.
pub(crate) fn resolve_async_fns(program: &Program, async_externs: HashSet<String>) -> HashSet<String> {
    let async_possible = !async_externs.is_empty();
    let mut async_fns = async_externs;

    let callees: Vec<(String, HashSet<String>, bool)> = program
        .functions
        .iter()
        .map(|f| {
            let mut names = HashSet::new();
            let mut indirect = false;
            collect_block(program, &f.body, &mut names, &mut indirect);
            (f.symbol.clone(), names, indirect)
        })
        .collect();

    if async_possible {
        for (name, _, indirect) in &callees {
            if *indirect {
                async_fns.insert(name.clone());
            }
        }
    }

    loop {
        let mut changed = false;
        for (name, called, _) in &callees {
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

fn collect_block(program: &Program, block: &Block, out: &mut HashSet<String>, indirect: &mut bool) {
    for stmt in block {
        collect_stmt(program, stmt, out, indirect);
    }
}

fn collect_stmt(program: &Program, stmt: &Statement, out: &mut HashSet<String>, indirect: &mut bool) {
    match stmt {
        Statement::Let(_, e) | Statement::Expression(e) => collect_expr(program, e, out, indirect),
        Statement::Return(e) => {
            if let Some(e) = e {
                collect_expr(program, e, out, indirect);
            }
        }
        Statement::Break | Statement::Continue => {}
        Statement::While { cond, body } => {
            collect_expr(program, cond, out, indirect);
            collect_block(program, body, out, indirect);
        }
        Statement::For {
            init,
            cond,
            step,
            body,
        } => {
            collect_block(program, init, out, indirect);
            collect_expr(program, cond, out, indirect);
            collect_expr(program, step, out, indirect);
            collect_block(program, body, out, indirect);
        }
        Statement::If {
            cond,
            then,
            otherwise,
        } => {
            collect_expr(program, cond, out, indirect);
            collect_block(program, then, out, indirect);
            if let Some(otherwise) = otherwise {
                collect_block(program, otherwise, out, indirect);
            }
        }
    }
}

fn collect_expr(program: &Program, expr: &Expression, out: &mut HashSet<String>, indirect: &mut bool) {
    match &expr.kind {
        ExpressionKind::Call { function, args } => {
            out.insert(program[*function].symbol.clone());
            for arg in args {
                collect_expr(program, arg, out, indirect);
            }
        }
        ExpressionKind::CallExtern { function, args } => {
            out.insert(program[*function].symbol.clone());
            for arg in args {
                collect_expr(program, arg, out, indirect);
            }
        }
        ExpressionKind::IndirectCall { callee, args } => {
            *indirect = true;
            collect_expr(program, callee, out, indirect);
            for arg in args {
                collect_expr(program, arg, out, indirect);
            }
        }
        ExpressionKind::Array(items) => {
            for item in items {
                collect_expr(program, item, out, indirect);
            }
        }
        ExpressionKind::StructLit { fields, .. } => {
            for field in fields {
                collect_expr(program, field, out, indirect);
            }
        }
        ExpressionKind::ArrayOp { receiver, args, .. } => {
            collect_expr(program, receiver, out, indirect);
            for arg in args {
                collect_expr(program, arg, out, indirect);
            }
        }
        ExpressionKind::Field { object, .. } => collect_expr(program, object, out, indirect),
        ExpressionKind::Index { array, index } => {
            collect_expr(program, array, out, indirect);
            collect_expr(program, index, out, indirect);
        }
        ExpressionKind::Assign { place, value } => {
            collect_place(program, place, out, indirect);
            collect_expr(program, value, out, indirect);
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::And(left, right)
        | ExpressionKind::Or(left, right) => {
            collect_expr(program, left, out, indirect);
            collect_expr(program, right, out, indirect);
        }
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { operand, .. }
        | ExpressionKind::Copy(operand)
        | ExpressionKind::Wrap(operand)
        | ExpressionKind::Unwrap(operand) => collect_expr(program, operand, out, indirect),
        ExpressionKind::ArrayNew { len } => collect_expr(program, len, out, indirect),
        ExpressionKind::Int(_)
        | ExpressionKind::Real(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Str(_)
        | ExpressionKind::None
        | ExpressionKind::Local(_)
        | ExpressionKind::FnRef(_)
        | ExpressionKind::DefaultStruct(_) => {}
    }
}

fn collect_place(
    program: &Program,
    place: &crate::ir::Place,
    out: &mut HashSet<String>,
    indirect: &mut bool,
) {
    use crate::ir::PlaceKind;
    match &place.kind {
        PlaceKind::Local(_) => {}
        PlaceKind::Field { object, .. } => collect_place(program, object, out, indirect),
        PlaceKind::Index { array, index } => {
            collect_place(program, array, out, indirect);
            collect_expr(program, index, out, indirect);
        }
    }
}
