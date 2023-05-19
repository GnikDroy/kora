#![feature(exclusive_range_pattern)]
#![feature(let_chains)]
#![feature(stmt_expr_attributes)]
#![feature(result_option_inspect)]
#![feature(iter_intersperse)]
#![allow(dead_code)]

mod lexer;
mod loader;
mod parser;
mod semantic_analyzer;
mod js_transpiler;

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::Path;

use wasm_bindgen::prelude::*;

use loader::{LoadedProgram, Loader};
use parser::{ASTVisitor, NodeId, Type};
use semantic_analyzer::{ArrayMethod, Resolver, ReturnChecker, SymbolId, SymbolTable, TypeChecker};

use js_transpiler::JsTranspiler;

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

impl Error for CompileErr {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Load(err) => Some(err),
            Self::Lex(err) => Some(err),
            Self::Parse(err) => Some(err),
            Self::Semantic(err) => Some(err),
        }
    }
}

impl fmt::Display for CompileErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileErr::Load(err) => write!(f, "{err}"),
            CompileErr::Lex(err) => write!(f, "{err}"),
            CompileErr::Parse(err) => write!(f, "{err}"),
            CompileErr::Semantic(err) => write!(f, "{err}"),
        }
    }
}

/// Compiler for the frontend
pub fn compile_frontend<P>(entry: &str, provider: P) -> Result<CompiledProgram, Vec<CompileErr>>
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
        let result = compile_frontend(
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
        let Err(errors) = compile_frontend(
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
        let Err(errors) = compile_frontend("main.kora", provider(vec![("main.kora", "int main() { }")]))
        else {
            panic!("expected a return error");
        };
        assert!(matches!(errors.as_slice(), [CompileErr::Semantic(_)]));
    }

    #[test]
    fn test_reports_load_error_for_a_missing_import() {
        let Err(errors) = compile_frontend(
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
        let Err(errors) = compile_frontend("main.kora", provider(vec![("main.kora", "int main() {")]))
        else {
            panic!("expected a parse error");
        };
        assert!(matches!(errors.as_slice(), [CompileErr::Parse(_)]));
    }
}

#[wasm_bindgen]
pub fn compile(source: &str) -> Result<String, String> {
    let mut out = String::from(
        r#"
        async function clear() {
            document.getElementById("stdout").innerText = "";
        }
        async function print(a) {
            document.getElementById("stdout").innerText += a;
        }
        async function input() {
            return document.getElementById("stdin").value;
        }
        "#,
    );
    let mut in_ = String::from(
        r#"
        extern nil clear();
        extern nil print(a: [char]);
        extern [char] input();
        "#,
    );
    in_.push_str(source);
    let source = &in_;
    let compiled = compile_frontend(source, || None);

    let mut transpiler = JsTranspiler::new();
    transpiler.visit_module(&module);
    let output = transpiler
        .get_source()
        .map(|s| s.to_string())
        .map_err(|e| {
            e.iter()
                .map(|x| x.to_string())
                .intersperse("\n".to_string())
                .collect::<String>()
        })?;

    out.push_str(output.as_str());
    Ok(out)
}
