use super::symbol_table::*;
use crate::parser::*;

pub struct TypeChecker {
    symbol_table: SymbolTable,
}

impl TypeChecker {
    pub fn new(symbol_table: SymbolTable) -> TypeChecker {
        TypeChecker { symbol_table }
    }
}

impl ASTVisitor for TypeChecker {}
