use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::instantiate::{GenericRegion, InstantiateErr};
use crate::lexer::{LexerErr, Position};
use crate::loader::{LoadErr, LoadedProgram, Loader};
use crate::parser::{ASTVisitor, ExternFunction, NodeId, ParseErr, Span, Type};
use crate::semantic_analyzer::{
    ArrayMethod, ConstValue, Resolver, ReturnChecker, SymbolId, SymbolTable, TypeChecker, TypeErr,
};

pub struct CompiledProgram {
    pub(crate) program: LoadedProgram,
    pub(crate) symbols: SymbolTable,
    pub(crate) consts: HashMap<SymbolId, ConstValue>,
    pub(crate) types: HashMap<NodeId, Type>,
    pub(crate) method_calls: HashMap<NodeId, SymbolId>,
    pub(crate) array_method_calls: HashMap<NodeId, ArrayMethod>,
    pub(crate) emitted: HashMap<NodeId, String>,
}

#[derive(Debug)]
pub enum CompileErr {
    Load(LoadErr),
    Lex(LexerErr),
    Parse(ParseErr),
    Instantiate(InstantiateErr),
    Semantic(TypeErr),
}

impl Error for CompileErr {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Load(err) => Some(err),
            Self::Lex(err) => Some(err),
            Self::Parse(err) => Some(err),
            Self::Instantiate(err) => Some(err),
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
            CompileErr::Instantiate(err) => write!(f, "{err}"),
            CompileErr::Semantic(err) => write!(f, "{err}"),
        }
    }
}

/// Attach an "instantiated here" note to every semantic error that lies inside a generic
fn annotate_generic_errors(errors: Vec<TypeErr>, regions: &[GenericRegion]) -> Vec<CompileErr> {
    fn le(a: &Position, b: &Position) -> bool {
        (a.row, a.col) <= (b.row, b.col)
    }
    fn within(inner: &Span, outer: &Span) -> bool {
        inner.source == outer.source && le(&outer.start, &inner.start) && le(&inner.end, &outer.end)
    }
    let mut out = Vec::new();
    for err in errors {
        let span = err.span.clone();
        out.push(CompileErr::Semantic(err));
        for region in regions {
            if within(&span, &region.span) {
                for (display, site) in &region.instances {
                    out.push(CompileErr::Instantiate(InstantiateErr {
                        msg: format!(
                            "note: inside a generic expanded as `{display}`, instantiated here"
                        ),
                        span: site.clone(),
                    }));
                }
            }
        }
    }
    out
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
    let mut program = Loader::new(provider).load(entry)?;

    let instances = crate::instantiate::Instantiator::new(&mut program)
        .run()
        .map_err(|errors| {
            errors
                .into_iter()
                .map(CompileErr::Instantiate)
                .collect::<Vec<_>>()
        })?;

    let mut symbols = Resolver::new()
        .resolve_program(&program, &instances)
        .map_err(|errors| annotate_generic_errors(errors, &instances.regions))?;

    let consts = {
        let modules: Vec<_> = program.modules.iter().map(|m| &m.module).collect();
        crate::semantic_analyzer::evaluate_constants(&mut symbols, &modules)
            .map_err(|errors| annotate_generic_errors(errors, &instances.regions))?
    };

    let mut analyze_errors = Vec::new();

    let (types, method_calls, array_method_calls) = {
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
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

    let mut extern_signatures: HashMap<&str, &ExternFunction> = HashMap::new();
    for module in &program.modules {
        for func in &module.module.extern_functions {
            let signature = |f: &ExternFunction| {
                (
                    f.return_type.clone(),
                    f.arguments
                        .iter()
                        .map(|a| a.node.typename.clone())
                        .collect::<Vec<_>>(),
                )
            };
            match extern_signatures.entry(func.node.name.as_str()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(&func.node);
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    if signature(entry.get()) != signature(&func.node) {
                        analyze_errors.push(TypeErr {
                            msg: "extern redeclared with a different signature",
                            span: func.span.clone(),
                        });
                    }
                }
            }
        }
    }

    // The entry point has a fixed signature
    if let Some(module) = program.modules.first()
        && let Some(main) = module
            .module
            .functions
            .iter()
            .find(|f| f.node.name == "main")
        && (main.node.return_type != Some(Type::Int) || !main.node.arguments.is_empty())
    {
        analyze_errors.push(TypeErr {
            msg: "main must be declared as `int main()`",
            span: main.span.clone(),
        });
    }

    if !analyze_errors.is_empty() {
        return Err(annotate_generic_errors(analyze_errors, &instances.regions));
    }

    let emitted = crate::mangle::emitted_symbols(&program, &instances.origins);
    Ok(CompiledProgram {
        program,
        symbols,
        consts,
        types,
        method_calls,
        array_method_calls,
        emitted,
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
    fn test_rejects_mismatched_extern_redeclaration() {
        let sources = provider(vec![
            (
                "main.kora",
                r#"import "a.kora"; extern int32 f(x: int32); int main() { return f(1); }"#,
            ),
            (
                "a.kora",
                "extern int64 f(x: int32);
int g() { return f(2); }",
            ),
        ]);
        let Err(errors) = compile("main.kora", sources) else {
            panic!("mismatched extern redeclaration must be rejected");
        };
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("extern redeclared")),
            "{errors:?}"
        );
    }

    #[test]
    fn test_accepts_matching_extern_redeclaration() {
        let sources = provider(vec![
            (
                "main.kora",
                r#"import "a.kora"; extern int32 abs(x: int32); int main() { return abs(1); }"#,
            ),
            (
                "a.kora",
                "extern int32 abs(x: int32);
int g() { return abs(2); }",
            ),
        ]);
        assert!(compile("main.kora", sources).is_ok());
    }

    #[test]
    fn test_rejects_wrong_main_signature() {
        for source in [
            "real main() { return 1.0; }",
            "int main(x: int) { return x; }",
        ] {
            let Err(errors) = compile("main.kora", provider(vec![("main.kora", source)])) else {
                panic!("expected a main-signature error for {source}");
            };
            assert!(matches!(errors.as_slice(), [CompileErr::Semantic(_)]));
        }
        // a `main` in an imported module is an ordinary function
        assert!(
            compile(
                "main.kora",
                provider(vec![
                    (
                        "main.kora",
                        r#"import "util.kora"; int main() { return util.main(1); }"#
                    ),
                    ("util.kora", "int main(x: int) { return x; }"),
                ]),
            )
            .is_ok()
        );
    }

    #[test]
    fn test_generic_program_compiles() {
        let result = compile(
            "main.kora",
            provider(vec![
                (
                    "main.kora",
                    r#"
                    import "util.kora";
                    struct pair<A, B> { first: A, second: B }
                    impl pair<A, B> {
                        A fst(self) { return self.first; }
                        B snd(self) { return self.second; }
                    }
                    int main() {
                        let p = new pair<int, string>{ first: 40, second: "ab" };
                        let boxed = util.wrap::<int>(p.fst());
                        return boxed.v + p.snd().len();
                    }
                    "#,
                ),
                (
                    "util.kora",
                    r#"
                    struct box<T> { v: T }
                    box<T> wrap<T>(v: T) { return new box<T>{ v: v }; }
                    "#,
                ),
            ]),
        );
        if let Err(errors) = result {
            panic!("unexpected errors: {errors:?}");
        }
    }

    #[test]
    fn test_frontend_keeps_source_names() {
        let compiled = compile(
            "main.kora",
            provider(vec![(
                "main.kora",
                r#"
                struct box<T> { v: T }
                impl box<T> { T get(self) { return self.v; } }
                T id<T>(x: T) { return x; }
                int main() {
                    let a = new box<int>{ v: 1 };
                    let b = new box<box<bool>>{ v: new box<bool>{ v: true } };
                    return id::<int>(a.get());
                }
                "#,
            )]),
        )
        .expect("compile");
        for module in &compiled.program.modules {
            for decl in &module.module.structs {
                assert!(!decl.node.name.contains("$$"), "{}", decl.node.name);
            }
            for decl in &module.module.functions {
                assert!(!decl.node.name.contains("$$"), "{}", decl.node.name);
            }
        }
        assert!(compiled.emitted.values().any(|s| s.contains("$$")));
    }

    #[test]
    fn test_generic_instance_error_mentions_instantiation() {
        let Err(errors) = compile(
            "main.kora",
            provider(vec![(
                "main.kora",
                "T bad<T>(x: T) { return x + true; } int main() { return bad::<int>(1); }",
            )]),
        ) else {
            panic!("expected a type error inside the instance");
        };
        assert!(
            errors.iter().any(|e| matches!(e, CompileErr::Semantic(_))),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("instantiated here")),
            "{errors:?}"
        );
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
