use std::collections::{HashMap, HashSet};

use crate::mangle::mangle;
use crate::parser::*;

/// JavaScript is a colored language ;)
/// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
pub(crate) fn resolve_async_fns(
    modules: &[&Module],
    function_names: &HashMap<NodeId, String>,
    method_calls: &HashMap<NodeId, String>,
    async_externs: HashSet<String>,
) -> HashSet<String> {
    let mut async_fns = async_externs;

    let callees: Vec<(String, HashSet<String>)> = modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .map(|f| {
                    (
                        function_names
                            .get(&f.id)
                            .cloned()
                            .unwrap_or_else(|| f.node.name.clone()),
                        called_names(&f.node.statement, function_names, method_calls),
                    )
                })
                .chain(module.impls.iter().flat_map(|impl_| {
                    impl_.node.functions.iter().map(|f| {
                        (
                            mangle(&impl_.node.struct_name.node, &f.node.name),
                            called_names(&f.node.statement, function_names, method_calls),
                        )
                    })
                }))
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

/// Every function name called within a body, under the same names the
/// emitter uses: mangled for kora functions, bare for externs, and
/// struct$method for methods.
fn called_names(
    body: &Spanned<Statement>,
    function_names: &HashMap<NodeId, String>,
    method_calls: &HashMap<NodeId, String>,
) -> HashSet<String> {
    let mut collector = CallCollector {
        function_names,
        method_calls,
        names: HashSet::new(),
    };
    collector.visit_statement(body);
    collector.names
}

struct CallCollector<'a> {
    function_names: &'a HashMap<NodeId, String>,
    method_calls: &'a HashMap<NodeId, String>,
    names: HashSet<String>,
}

impl ASTVisitor for CallCollector<'_> {
    fn visit_call_expression(
        &mut self,
        callee: &Spanned<Expression>,
        args: &[Spanned<Expression>],
    ) {
        if let Some(name) = self.function_names.get(&callee.id) {
            self.names.insert(name.clone());
        } else if let Some(name) = self.method_calls.get(&callee.id) {
            self.names.insert(name.clone());
        } else if let Expression::Identifier(name) = &callee.node {
            self.names.insert(name.clone());
        }
        walk_call_expression(self, callee, args);
    }
}
