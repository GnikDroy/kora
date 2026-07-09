use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::lexer::LexerErr;
use crate::loader::{LoadErr, LoadedProgram, Loader};
use crate::parser::{ASTVisitor, NodeId, ParseErr, Type};
use crate::semantic_analyzer::{
    ArrayMethod, Resolver, ReturnChecker, SymbolId, SymbolTable, TypeChecker, TypeErr,
};

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

/// Run the whole front-end over the program reachable from `entry`: build the
/// source graph, resolve names across modules, then type-check and
/// return-check every module.
pub fn compile<P>(entry: &str, provider: P) -> Result<CompiledProgram, Vec<CompileErr>>
where
    P: Fn(&Path) -> Option<String>,
{
    // prioritize std/ imports from embedded standard library
    let provider = |path: &Path| crate::stdlib::source(path).or_else(|| provider(path));
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
    fn test_std_conv() {
        let result = compile(
            "main.kora",
            provider(vec![(
                "main.kora",
                r#"import "std/conv";
                   int main() {
                       let s = conv.int_to_string(-42);
                       let n = conv.string_to_int("100");
                       if (n == none) { return 1; }
                       return s.len() + n!;
                   }"#,
            )]),
        );
        if let Err(errors) = result {
            panic!("unexpected errors: {errors:?}");
        }
    }

    #[test]
    fn test_std_math() {
        let result = compile(
            "main.kora",
            provider(vec![(
                "main.kora",
                r#"import "std/math";
                   int main() {
                       let a = math.sqrtf(2.0);
                       let b = math.sin(1.0) + math.cos(1.0) + math.tan(0.5);
                       let c = math.exp(1.0) * math.log(2.718281828) + math.log2(8.0);
                       let d = math.powf(2.0, 10.0);
                       let e = math.floorf(3.7) + math.ceilf(1.2) + math.roundf(2.5);
                       let f = math.absf(-1.0) + math.signf(-2.0) + math.minf(1.0, 2.0);
                       let g = math.atan(1.0) + math.atan2(1.0, 1.0);
                       if (a > 1.0 && b < 3.0 && c > 0.0 && d > 1000.0 && e > 0.0 && f < 5.0 && g > 0.0) {
                           return 1;
                       }
                       return 0;
                   }"#,
            )]),
        );
        if let Err(errors) = result {
            panic!("unexpected errors: {errors:?}");
        }
    }

    #[test]
    fn test_std_str() {
        let result = compile(
            "main.kora",
            provider(vec![(
                "main.kora",
                r#"import "std/str";
                   int main() {
                       let parts = str.split("a,b,c", ',');
                       let joined = str.join(parts, "-");
                       let i = str.index_of(joined, "b");
                       if (i == none) { return 1; }
                       if (!str.contains(joined, "a")) { return 2; }
                       if (!str.starts_with(joined, "a-")) { return 3; }
                       if (!str.ends_with(joined, "-c")) { return 4; }
                       let up = str.to_upper(str.trim("  hi  "));
                       return joined.len() + i! + up.len() + str.reverse("xy").len();
                   }"#,
            )]),
        );
        if let Err(errors) = result {
            panic!("unexpected errors: {errors:?}");
        }
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
