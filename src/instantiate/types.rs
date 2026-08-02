use std::collections::HashMap;

use crate::parser::{NodeId, Span, Type};

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceOrigin {
    pub generic: String,
    pub args: Vec<Type>,
}

pub type InstanceOrigins = HashMap<NodeId, InstanceOrigin>;

#[derive(Default)]
pub(super) struct InstantiationStack(Vec<(String, Span)>);

impl InstantiationStack {
    pub(super) fn new() -> InstantiationStack {
        InstantiationStack::default()
    }

    pub(super) fn depth(&self) -> usize {
        self.0.len()
    }

    pub(super) fn push(&mut self, display: String, site: Span) {
        self.0.push((display, site));
    }

    pub(super) fn pop(&mut self) {
        self.0.pop();
    }

    pub(super) fn summary(&self) -> String {
        let names: Vec<&str> = self.0.iter().map(|(name, _)| name.as_str()).collect();
        if names.len() <= 6 {
            names.join(" -> ")
        } else {
            format!(
                "{} -> ... -> {}",
                names[..3].join(" -> "),
                names[names.len() - 3..].join(" -> ")
            )
        }
    }
}

pub(super) struct InstantiationSite {
    pub(super) args: Vec<Type>,
    pub(super) span: Span,
}

#[derive(Default)]
pub(super) struct InstanceRegistry(HashMap<NodeId, HashMap<Vec<Type>, NodeId>>);

impl InstanceRegistry {
    pub(super) fn get(&self, generic: NodeId, args: &[Type]) -> Option<NodeId> {
        self.0.get(&generic)?.get(args).copied()
    }

    pub(super) fn insert(&mut self, generic: NodeId, args: Vec<Type>, instance: NodeId) {
        self.0.entry(generic).or_default().insert(args, instance);
    }
}
