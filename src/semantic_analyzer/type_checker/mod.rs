mod builtins;
mod expression;
mod statement;

use std::collections::HashMap;

use super::{errors::TypeErr, symbol_resolver::*};
use crate::parser::*;

pub use builtins::ArrayMethod;
#[allow(unused_imports)]
pub use builtins::is_reference;

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
mod test_support {
    use std::collections::HashMap;
    use std::path::Path;

    use super::TypeChecker;
    use crate::loader::{LoadedProgram, Loader};
    use crate::parser::ASTVisitor;
    use crate::semantic_analyzer::symbol_resolver::SymbolTable;
    use crate::semantic_analyzer::symbol_resolver::test_support::resolve_program;

    pub(crate) fn analyze(source: &str) -> (SymbolTable, LoadedProgram) {
        let map: HashMap<String, String> = [("main.kora".to_string(), source.to_string())].into();
        let provider = move |p: &Path| p.to_str().and_then(|s| map.get(s)).cloned();
        let mut program = Loader::new(&provider).load("main.kora").expect("load");
        let symbols = resolve_program(&mut program).expect("resolve");
        (symbols, program)
    }

    pub(crate) fn check_cases(cases: &[(&str, bool)]) {
        for (source, expect_ok) in cases {
            let (symbols, program) = analyze(source);
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&program.modules[0].module);
            assert_eq!(checker.check().is_ok(), *expect_ok, "source: {}", source);
        }
    }

    pub(crate) fn program_type_checks(mut program: LoadedProgram) -> bool {
        let symbols = resolve_program(&mut program).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        for module in &program.modules {
            checker.visit_module(&module.module);
        }
        checker.check().is_ok()
    }
}
