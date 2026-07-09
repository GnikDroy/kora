mod builtins;
mod expression;
mod statement;

use std::collections::HashMap;

use super::{errors::TypeErr, symbol_resolver::*};
use crate::parser::*;

pub use builtins::ArrayMethod;

pub struct TypeChecker<'a> {
    symbols: &'a SymbolTable,
    current_return_type: Option<Type>,
    errors: Vec<TypeErr>,
    pub types: HashMap<NodeId, Type>,
    pub method_calls: HashMap<NodeId, SymbolId>,
    pub array_method_calls: HashMap<NodeId, ArrayMethod>,
    inferred: HashMap<SymbolId, Type>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> TypeChecker<'a> {
        TypeChecker {
            symbols,
            current_return_type: None,
            errors: Vec::new(),
            types: HashMap::new(),
            method_calls: HashMap::new(),
            array_method_calls: HashMap::new(),
            inferred: HashMap::new(),
        }
    }

    pub fn check(&self) -> Result<(), &[TypeErr]> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(&self.errors)
        }
    }
}

#[cfg(test)]
mod tests;
