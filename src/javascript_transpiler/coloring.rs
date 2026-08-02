use std::collections::{HashMap, HashSet};

use crate::parser::*;

/// JavaScript is a colored language ;)
/// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
pub(crate) fn resolve_async_fns(
    modules: &[&Module],
    function_call_names: &HashMap<NodeId, String>,
    method_calls: &HashMap<NodeId, String>,
    async_externs: HashSet<String>,
    emitted: &HashMap<NodeId, String>,
) -> HashSet<String> {
    let mut async_fns = async_externs;

    let callees: Vec<(String, HashSet<String>)> = modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .chain(module.impls.iter().flat_map(|i| i.node.functions.iter()))
                .map(|f| {
                    (
                        emitted[&f.id].clone(),
                        called_names(&f.node.statement, function_call_names, method_calls),
                    )
                })
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

fn called_names(
    body: &Spanned<Statement>,
    function_call_names: &HashMap<NodeId, String>,
    method_calls: &HashMap<NodeId, String>,
) -> HashSet<String> {
    let mut collector = CallCollector {
        function_call_names,
        method_calls,
        names: HashSet::new(),
    };
    collector.visit_statement(body);
    collector.names
}

struct CallCollector<'a> {
    function_call_names: &'a HashMap<NodeId, String>,
    method_calls: &'a HashMap<NodeId, String>,
    names: HashSet<String>,
}

impl ASTVisitor for CallCollector<'_> {
    fn visit_call_expression(
        &mut self,
        callee: &Spanned<Expression>,
        args: &[Spanned<Expression>],
    ) {
        if let Some(name) = self.function_call_names.get(&callee.id) {
            self.names.insert(name.clone());
        } else if let Some(name) = self.method_calls.get(&callee.id) {
            self.names.insert(name.clone());
        } else if let Expression::Identifier(name) = &callee.node {
            self.names.insert(name.clone());
        }
        walk_call_expression(self, callee, args);
    }
}
