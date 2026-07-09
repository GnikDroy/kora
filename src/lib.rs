#![allow(dead_code)]

mod lexer;
mod loader;
mod parser;
mod semantic_analyzer;

use std::collections::HashMap;
use std::path::Path;

use loader::{LoadedProgram, Loader};
use parser::{ASTVisitor, NodeId, Type};
use semantic_analyzer::{ArrayMethod, Resolver, ReturnChecker, SymbolId, SymbolTable, TypeChecker};

pub use lexer::LexerErr;
pub use loader::LoadErr;
pub use parser::ParseErr;
pub use semantic_analyzer::TypeErr;

pub struct CompiledProgram {
    pub(crate) program: LoadedProgram,
    pub(crate) symbols: SymbolTable,
    pub(crate) types: HashMap<NodeId, Type>,
    pub(crate) method_calls: HashMap<NodeId, SymbolId>,
    pub(crate) array_method_calls: HashMap<NodeId, ArrayMethod>,
}

#[derive(Debug)]
pub enum CompileErr {
    Load(LoadErr),
    Lex(LexerErr),
    Parse(ParseErr),
    Semantic(TypeErr),
}

/// Compiler for the frontend
pub fn compile<P>(entry: &str, provider: P) -> Result<CompiledProgram, Vec<CompileErr>>
where
    P: Fn(&Path) -> Option<String>,
{
    let program = Loader::new(provider).load(entry)?;

    let symbols = Resolver::new()
        .resolve_program(&program)
        .map_err(|errors| {
            errors
                .into_iter()
                .map(CompileErr::Semantic)
                .collect::<Vec<_>>()
        })?;

    let mut analyze_errors = Vec::new();

    let (types, method_calls, array_method_calls) = {
        let mut checker = TypeChecker::new(&symbols);
        for module in &program.modules {
            checker.visit_module(&module.module);
        }
        if let Err(errors) = checker.check() {
            analyze_errors.extend(errors.iter().cloned());
        }
        (
            checker.types,
            checker.method_calls,
            checker.array_method_calls,
        )
    };

    let mut return_checker = ReturnChecker::new();
    for module in &program.modules {
        return_checker.visit_module(&module.module);
    }
    if let Err(errors) = return_checker.check() {
        analyze_errors.extend(errors.iter().cloned());
    }

    if !analyze_errors.is_empty() {
        return Err(analyze_errors
            .into_iter()
            .map(CompileErr::Semantic)
            .collect());
    }

    Ok(CompiledProgram {
        program,
        symbols,
        types,
        method_calls,
        array_method_calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(files: Vec<(&'static str, &'static str)>) -> impl Fn(&Path) -> Option<String> {
        let map: HashMap<String, String> = files
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |p: &Path| p.to_str().and_then(|s| map.get(s)).cloned()
    }

    #[test]
    fn test_compiles_a_multi_module_program() {
        let result = compile(
            "main.kora",
            provider(vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.helper(); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ]),
        );
        if let Err(errors) = result {
            panic!("unexpected errors: {errors:?}");
        }
    }

    #[test]
    fn test_reports_analyze_errors_across_modules() {
        let Err(errors) = compile(
            "main.kora",
            provider(vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.helper(1); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ]),
        ) else {
            panic!("expected a type error");
        };
        assert!(matches!(errors.as_slice(), [CompileErr::Semantic(_)]));
    }

    #[test]
    fn test_reports_missing_return() {
        let Err(errors) = compile("main.kora", provider(vec![("main.kora", "int main() { }")]))
        else {
            panic!("expected a return error");
        };
        assert!(matches!(errors.as_slice(), [CompileErr::Semantic(_)]));
    }

    #[test]
    fn test_reports_load_error_for_a_missing_import() {
        let Err(errors) = compile(
            "main.kora",
            provider(vec![(
                "main.kora",
                r#"import "missing.kora"; int main() { return 0; }"#,
            )]),
        ) else {
            panic!("expected a load error");
        };
        assert!(matches!(errors.as_slice(), [CompileErr::Load(_)]));
    }

    #[test]
    fn test_reports_parse_error() {
        let Err(errors) = compile("main.kora", provider(vec![("main.kora", "int main() {")]))
        else {
            panic!("expected a parse error");
        };
        assert!(matches!(errors.as_slice(), [CompileErr::Parse(_)]));
    }
}
