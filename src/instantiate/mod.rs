mod collect;
mod concretize;
mod errors;

pub use errors::*;

use std::collections::{HashMap, HashSet};

use crate::loader::LoadedProgram;
use crate::mangle::{encode_instance, unique_name};
use crate::parser::{Span, Type};

use collect::{GenericFns, GenericStructs, collect};

const DEPTH_LIMIT: usize = 64;

pub(crate) type TypeSubstitutions = HashMap<String, Type>;
pub(crate) type Chain = Vec<(String, Span)>;

/// Monomorphizes the program in place: clones every generic struct/function
/// per concrete type-argument tuple and concretizes all mentions to the
/// instance names. The generic originals stay in their modules' generic
/// buckets, which no later pass reads; downstream only sees concrete code.
pub struct Instantiator<'p> {
    program: &'p mut LoadedProgram,

    imports: Vec<HashMap<String, usize>>,

    generic_structs: GenericStructs,
    generic_fns: GenericFns,

    struct_registry: HashMap<(String, Vec<Type>), String>,
    fn_registry: HashMap<(usize, String, Vec<Type>), String>,

    used_struct_names: HashSet<String>,
    used_fn_names: Vec<HashSet<String>>,

    instance_displays: HashMap<String, String>,
    errors: Vec<InstantiateErr>,
}

impl<'p> Instantiator<'p> {
    pub fn new(program: &'p mut LoadedProgram) -> Instantiator<'p> {
        let indices: HashMap<_, _> = program
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();

        let imports = program
            .modules
            .iter()
            .map(|m| {
                m.imports
                    .iter()
                    .map(|imp| (imp.local_name.clone(), indices[&imp.target]))
                    .collect()
            })
            .collect();

        Instantiator {
            program,
            imports,
            generic_structs: HashMap::new(),
            generic_fns: HashMap::new(),
            struct_registry: HashMap::new(),
            fn_registry: HashMap::new(),
            used_struct_names: HashSet::new(),
            used_fn_names: Vec::new(),
            instance_displays: HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn run(mut self) -> Result<Vec<GenericNote>, Vec<InstantiateErr>> {
        let ctx = collect(self.program)?;
        self.generic_structs = ctx.generic_structs;
        self.generic_fns = ctx.generic_fns;
        self.used_struct_names = ctx.used_struct_names;
        self.used_fn_names = ctx.used_fn_names;

        self.concretize_program();

        if self.errors.is_empty() {
            Ok(self.notes())
        } else {
            Err(self.errors)
        }
    }

    fn error(&mut self, msg: String, span: &Span) {
        self.errors.push(InstantiateErr {
            msg,
            span: span.clone(),
        });
    }

    /// One note per generic source region (declaration and each impl), all
    /// carrying the region's instances so semantic errors inside any region
    /// can point back at the use sites that produced them.
    fn notes(&self) -> Vec<GenericNote> {
        let mut notes = Vec::new();
        for def in self.generic_structs.values() {
            let regions =
                std::iter::once(&def.decl.span).chain(def.impls.iter().map(|(_, imp)| &imp.span));
            for region in regions {
                notes.push(GenericNote {
                    region: region.clone(),
                    instances: def.instances.clone(),
                });
            }
        }
        for def in self.generic_fns.values() {
            notes.push(GenericNote {
                region: def.decl.span.clone(),
                instances: def.instances.clone(),
            });
        }
        notes
    }

    fn instantiate_struct(
        &mut self,
        name: &str,
        args: &[Type],
        use_span: &Span,
        chain: &mut Chain,
    ) -> Option<String> {
        let key = (name.to_string(), args.to_vec());
        if let Some(instance) = self.struct_registry.get(&key) {
            return Some(instance.clone());
        }

        let def = &self.generic_structs[name];
        let expected = def.decl.node.type_params.len();
        if expected != args.len() {
            self.error(
                format!(
                    "`{name}` expects {expected} type argument(s), found {}",
                    args.len()
                ),
                use_span,
            );
            return None;
        }
        let display = self.display_instance(name, args);
        if chain.len() >= DEPTH_LIMIT {
            self.error(
                format!(
                    "instantiation depth limit ({DEPTH_LIMIT}) exceeded at `{display}`: {}",
                    chain_summary(chain)
                ),
                use_span,
            );
            return None;
        }

        let module = self.generic_structs[name].module;
        let instance = self.unique_struct_name(encode_instance(name, args, &HashMap::new()));
        self.struct_registry.insert(key, instance.clone());
        self.instance_displays
            .insert(instance.clone(), display.clone());
        self.generic_structs
            .get_mut(name)
            .unwrap()
            .instances
            .push((display.clone(), use_span.clone()));

        chain.push((display, use_span.clone()));
        let decl = self.instance_struct(name, &instance, args, chain);
        self.program.modules[module].module.structs.push(decl);
        for (impl_module, imp) in self.instance_impls(name, &instance, args, chain) {
            self.program.modules[impl_module].module.impls.push(imp);
        }
        chain.pop();
        Some(instance)
    }

    fn instantiate_function(
        &mut self,
        module: usize,
        name: &str,
        args: &[Type],
        use_span: &Span,
        chain: &mut Chain,
    ) -> Option<String> {
        let key = (module, name.to_string(), args.to_vec());
        if let Some(instance) = self.fn_registry.get(&key) {
            return Some(instance.clone());
        }

        let def = &self.generic_fns[&(module, name.to_string())];
        let expected = def.decl.node.type_params.len();
        if expected != args.len() {
            self.error(
                format!(
                    "`{name}` expects {expected} type argument(s), found {}",
                    args.len()
                ),
                use_span,
            );
            return None;
        }
        let display = self.display_instance(name, args);
        if chain.len() >= DEPTH_LIMIT {
            self.error(
                format!(
                    "instantiation depth limit ({DEPTH_LIMIT}) exceeded at `{display}`: {}",
                    chain_summary(chain)
                ),
                use_span,
            );
            return None;
        }

        let instance = self.unique_fn_name(module, encode_instance(name, args, &HashMap::new()));
        self.fn_registry.insert(key, instance.clone());
        self.generic_fns
            .get_mut(&(module, name.to_string()))
            .unwrap()
            .instances
            .push((display.clone(), use_span.clone()));

        chain.push((display, use_span.clone()));
        let decl = self.instance_function(module, name, &instance, args, chain);
        self.program.modules[module].module.functions.push(decl);
        chain.pop();
        Some(instance)
    }

    fn display_type(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "int".to_string(),
            Type::Real => "real".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Char => "char".to_string(),
            Type::Opaque => "opaque".to_string(),
            Type::Array(inner) => format!("[{}]", self.display_type(inner)),
            Type::Optional(inner) => format!("{}?", self.display_type(inner)),
            Type::Struct(sr) => self
                .instance_displays
                .get(&sr.name.node)
                .cloned()
                .unwrap_or_else(|| sr.name.node.clone()),
            Type::Generic(name, args) => self.display_instance(&name.node, args),
            Type::Function(_, _) => "fn".to_string(),
        }
    }

    fn display_instance(&self, name: &str, args: &[Type]) -> String {
        let args = args
            .iter()
            .map(|a| self.display_type(a))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{name}<{args}>")
    }

    fn unique_struct_name(&mut self, base: String) -> String {
        let name = unique_name(base, |candidate| self.used_struct_names.contains(candidate));
        self.used_struct_names.insert(name.clone());
        name
    }

    fn unique_fn_name(&mut self, module: usize, base: String) -> String {
        let name = unique_name(base, |candidate| {
            self.used_fn_names[module].contains(candidate)
        });
        self.used_fn_names[module].insert(name.clone());
        name
    }
}

fn chain_summary(chain: &Chain) -> String {
    let names: Vec<&str> = chain.iter().map(|(name, _)| name.as_str()).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Loader;
    use crate::parser::Spanned;
    use std::path::Path;

    fn load(files: &[(&'static str, &'static str)]) -> LoadedProgram {
        let map: HashMap<String, String> = files
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let provider = move |p: &Path| p.to_str().and_then(|s| map.get(s)).cloned();
        Loader::new(provider).load(files[0].0).expect("load")
    }

    fn fully_instantiated(program: &LoadedProgram) -> bool {
        use crate::parser::{ASTVisitor, Expression};

        fn concrete(ty: &Type) -> bool {
            match ty {
                Type::Generic(_, _) => false,
                Type::Array(inner) | Type::Optional(inner) => concrete(inner),
                Type::Function(ret, args) => {
                    ret.as_deref().is_none_or(concrete) && args.iter().all(concrete)
                }
                _ => true,
            }
        }
        struct Sweep {
            clean: bool,
        }
        impl ASTVisitor for Sweep {
            fn visit_typename(&mut self, ty: &Type) {
                self.clean &= concrete(ty);
            }
            fn visit_type_application(&mut self, _: &Spanned<Expression>, _: &[Type]) {
                self.clean = false;
            }
        }
        let mut sweep = Sweep { clean: true };
        for module in &program.modules {
            sweep.visit_module(&module.module);
        }
        sweep.clean
    }

    fn run(
        files: &[(&'static str, &'static str)],
    ) -> Result<(LoadedProgram, Vec<GenericNote>), Vec<InstantiateErr>> {
        let mut program = load(files);
        let notes = Instantiator::new(&mut program).run()?;
        assert!(fully_instantiated(&program));
        Ok((program, notes))
    }

    fn run_one(source: &'static str) -> (LoadedProgram, Vec<GenericNote>) {
        run(&[("main.kora", source)]).expect("instantiate")
    }

    fn expect_err(source: &'static str, needle: &str) {
        let mut program = load(&[("main.kora", source)]);
        let Err(errors) = Instantiator::new(&mut program).run() else {
            panic!("expected an instantiation error containing `{needle}`");
        };
        assert!(errors.iter().any(|e| e.msg.contains(needle)), "{errors:?}");
    }

    fn fn_names(program: &LoadedProgram, module: usize) -> Vec<String> {
        program.modules[module]
            .module
            .functions
            .iter()
            .map(|f| f.node.name.clone())
            .collect()
    }

    fn struct_names(program: &LoadedProgram, module: usize) -> Vec<String> {
        program.modules[module]
            .module
            .structs
            .iter()
            .map(|s| s.node.name.clone())
            .collect()
    }

    fn function<'a>(
        program: &'a LoadedProgram,
        module: usize,
        name: &str,
    ) -> &'a Spanned<crate::parser::Function> {
        program.modules[module]
            .module
            .functions
            .iter()
            .find(|f| f.node.name == name)
            .unwrap_or_else(|| panic!("function `{name}` not found"))
    }

    fn body_debug(program: &LoadedProgram, module: usize, name: &str) -> String {
        format!("{:?}", function(program, module, name).node.statement)
    }

    #[test]
    fn test_instantiates_generic_function() {
        let (program, _) =
            run_one("int main() { return id::<int>(3); } T id<T>(x: T) { return x; }");
        let names = fn_names(&program, 0);
        assert!(names.contains(&"id$$int".to_string()), "{names:?}");
        assert!(!names.contains(&"id".to_string()), "{names:?}");
        assert!(body_debug(&program, 0, "main").contains("id$$int"));
        let instance = function(&program, 0, "id$$int");
        assert_eq!(instance.node.return_type, Some(Type::Int));
        assert_eq!(instance.node.arguments[0].node.typename, Type::Int);
    }

    #[test]
    fn test_instantiates_generic_struct_with_impls() {
        let (program, _) = run_one(
            r#"
            struct pair<A, B> { first: A, second: B }
            impl pair<A, B> { A fst(self) { return self.first; } }
            int main() {
                let p = new pair<int, string>{ first: 1, second: "x" };
                return p.fst();
            }
            "#,
        );
        let names = struct_names(&program, 0);
        assert_eq!(names, vec!["pair$$int$$arr_char".to_string()]);
        let decl = &program.modules[0].module.structs[0];
        assert_eq!(decl.node.members[0].node.typename, Type::Int);
        assert_eq!(
            decl.node.members[1].node.typename,
            Type::Array(Box::new(Type::Char))
        );
        let imp = &program.modules[0].module.impls[0];
        assert_eq!(imp.node.struct_ref.name.node, "pair$$int$$arr_char");
        let method = &imp.node.functions[0];
        assert!(matches!(
            &method.node.arguments[0].node.typename,
            Type::Struct(sr) if sr.name.node == "pair$$int$$arr_char"
        ));
        assert_eq!(method.node.return_type, Some(Type::Int));
    }

    #[test]
    fn test_dedupes_instances() {
        let (program, _) = run_one(
            "int main() { return id::<int>(1) + id::<int>(2); } T id<T>(x: T) { return x; }",
        );
        let count = fn_names(&program, 0)
            .iter()
            .filter(|n| *n == "id$$int")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_generic_calling_generic() {
        let (program, _) = run_one(
            r#"
            T outer<T>(x: T) { return inner::<T>(x); }
            T inner<T>(x: T) { return x; }
            int main() { return outer::<int>(7); }
            "#,
        );
        let names = fn_names(&program, 0);
        assert!(names.contains(&"outer$$int".to_string()), "{names:?}");
        assert!(names.contains(&"inner$$int".to_string()), "{names:?}");
        assert!(body_debug(&program, 0, "outer$$int").contains("inner$$int"));
    }

    #[test]
    fn test_generic_struct_field_of_generic_struct() {
        let (program, _) = run_one(
            r#"
            struct box<T> { v: T }
            struct uses<T> { b: box<T> }
            int main() {
                let u = new uses<int>{ b: new box<int>{ v: 1 } };
                return u.b.v;
            }
            "#,
        );
        let names = struct_names(&program, 0);
        assert!(names.contains(&"box$$int".to_string()), "{names:?}");
        assert!(names.contains(&"uses$$int".to_string()), "{names:?}");
        let uses = program.modules[0]
            .module
            .structs
            .iter()
            .find(|s| s.node.name == "uses$$int")
            .unwrap();
        assert!(matches!(
            &uses.node.members[0].node.typename,
            Type::Struct(sr) if sr.name.node == "box$$int"
        ));
    }

    #[test]
    fn test_self_referential_generic_struct() {
        let (program, _) = run_one(
            r#"
            struct node<T> { value: T, next: node<T>? }
            int main() {
                let n = new node<int>{ value: 1, next: none };
                return n.value;
            }
            "#,
        );
        assert_eq!(struct_names(&program, 0), vec!["node$$int".to_string()]);
        let node = &program.modules[0].module.structs[0];
        assert!(matches!(
            &node.node.members[1].node.typename,
            Type::Optional(inner) if matches!(&**inner, Type::Struct(sr) if sr.name.node == "node$$int")
        ));
    }

    #[test]
    fn test_depth_limit() {
        expect_err(
            r#"
            struct box<T> { v: T }
            void f<T>(x: T) { f::<box<T>>(x); }
            int main() { f::<int>(0); return 0; }
            "#,
            "depth limit",
        );
    }

    #[test]
    fn test_wrong_arity() {
        expect_err(
            r#"
            struct pair<A, B> { first: A, second: B }
            void f(p: pair<int>) {}
            int main() { return 0; }
            "#,
            "expects 2 type argument(s), found 1",
        );
    }

    #[test]
    fn test_turbofish_on_non_generic() {
        expect_err(
            "int id(x: int) { return x; } int main() { return id::<int>(3); }",
            "is not a generic function",
        );
    }

    #[test]
    fn test_bare_generic_struct_mention() {
        expect_err(
            "struct box<T> { v: T } void f(b: box) {} int main() { return 0; }",
            "requires type arguments",
        );
    }

    #[test]
    fn test_nested_optional_rejected() {
        expect_err(
            r#"
            struct box<T> { v: T? }
            int main() { let b = new box<int?>{ v: none }; return 0; }
            "#,
            "nested optional",
        );
    }

    #[test]
    fn test_param_shadows_struct() {
        expect_err(
            r#"
            struct T { x: int }
            struct box<T> { v: T }
            int main() { let b = new box<int>{ v: 1 }; return 0; }
            "#,
            "shadows struct",
        );
    }

    #[test]
    fn test_duplicate_params() {
        expect_err(
            "void f<T, T>(a: T) {} int main() { return 0; }",
            "duplicate type parameter",
        );
    }

    #[test]
    fn test_generic_main_rejected() {
        expect_err("int main<T>() { return 0; }", "main cannot be generic");
    }

    #[test]
    fn test_impl_arity_mismatch() {
        expect_err(
            r#"
            struct pair<A, B> { first: A, second: B }
            impl pair<A> { A fst(self) { return self.first; } }
            int main() { return 0; }
            "#,
            "must declare the struct's 2 type parameter(s)",
        );
    }

    #[test]
    fn test_impl_type_params_on_concrete_struct() {
        expect_err(
            r#"
            struct P { x: int }
            impl P<T> { int get(self) { return self.x; } }
            int main() { return 0; }
            "#,
            "is not a generic struct",
        );
    }

    #[test]
    fn test_unused_generic_is_unchecked() {
        let (program, _) = run_one(
            "T broken<T>(x: T) { return frobnicate(x, does_not_exist); } int main() { return 0; }",
        );
        assert_eq!(fn_names(&program, 0), vec!["main".to_string()]);
    }

    #[test]
    fn test_fresh_node_ids() {
        let (program, _) = run_one(
            r#"
            T id<T>(x: T) { return x; }
            int main() {
                let a = id::<int>(1);
                let b = id::<bool>(true);
                return a;
            }
            "#,
        );
        let first = function(&program, 0, "id$$int");
        let second = function(&program, 0, "id$$bool");
        assert_ne!(first.id, second.id);
        assert_ne!(first.node.statement.id, second.node.statement.id);
        assert_ne!(first.node.arguments[0].id, second.node.arguments[0].id);
    }

    #[test]
    fn test_qualified_turbofish_instantiates_in_defining_module() {
        let (program, _) = run(&[
            (
                "main.kora",
                r#"import "util.kora"; int main() { return util.make::<int>(5); }"#,
            ),
            ("util.kora", "T make<T>(v: T) { return v; }"),
        ])
        .expect("instantiate");
        assert_eq!(fn_names(&program, 0), vec!["main".to_string()]);
        assert_eq!(fn_names(&program, 1), vec!["make$$int".to_string()]);
        assert!(body_debug(&program, 0, "main").contains("make$$int"));
    }

    #[test]
    fn test_two_importers_dedupe() {
        let (program, _) = run(&[
            (
                "main.kora",
                r#"
                import "a.kora";
                import "util.kora";
                int main() { return util.make::<int>(1) + a.get(); }
                "#,
            ),
            (
                "a.kora",
                r#"import "util.kora"; int get() { return util.make::<int>(2); }"#,
            ),
            ("util.kora", "T make<T>(v: T) { return v; }"),
        ])
        .expect("instantiate");
        let util = program
            .modules
            .iter()
            .position(|m| program.sources[m.id.0 as usize].path.to_str() == Some("util.kora"))
            .unwrap();
        let count = fn_names(&program, util)
            .iter()
            .filter(|n| *n == "make$$int")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_notes_record_instances() {
        let (_, notes) = run_one(
            r#"
            struct pair<A, B> { first: A, second: B }
            impl pair<A, B> { A fst(self) { return self.first; } }
            int main() {
                let p = new pair<int, string>{ first: 1, second: "x" };
                return p.fst();
            }
            "#,
        );
        let displays: Vec<&str> = notes
            .iter()
            .flat_map(|n| n.instances.iter().map(|(d, _)| d.as_str()))
            .collect();
        assert!(displays.contains(&"pair<int, [char]>"), "{displays:?}");
    }
}
