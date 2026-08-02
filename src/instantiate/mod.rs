mod collect;
mod concretize;
mod errors;

pub use errors::*;

use std::collections::{HashMap, HashSet};

use crate::loader::LoadedProgram;
use crate::mangle::{InstanceOrigins, unique_name};
use crate::parser::{NodeId, Span, Type};

use collect::{GenericFns, GenericStructs, collect};
use concretize::{scaffold_function, scaffold_struct};

const DEPTH_LIMIT: usize = 64;

pub(crate) type TypeSubstitutions = HashMap<String, Type>;
pub(crate) type Chain = Vec<(String, Span)>;

#[derive(Default)]
pub struct Instantiated {
    pub notes: Vec<GenericNote>,
    pub resolutions: HashMap<NodeId, NodeId>,
    pub fn_instances: HashSet<NodeId>,
    pub struct_instances: HashSet<NodeId>,
    pub origins: InstanceOrigins,
}

pub struct Instantiator<'p> {
    program: &'p mut LoadedProgram,

    imports: Vec<HashMap<String, usize>>,

    generic_structs: GenericStructs,
    generic_fns: GenericFns,

    struct_registry: HashMap<(String, Vec<Type>), NodeId>,
    fn_registry: HashMap<(usize, String, Vec<Type>), NodeId>,

    used_struct_names: HashSet<String>,
    used_fn_names: Vec<HashSet<String>>,

    resolutions: HashMap<NodeId, NodeId>,
    fn_instances: HashSet<NodeId>,
    struct_instances: HashSet<NodeId>,
    origins: InstanceOrigins,

    instance_displays: HashMap<NodeId, String>,
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
            resolutions: HashMap::new(),
            fn_instances: HashSet::new(),
            struct_instances: HashSet::new(),
            origins: InstanceOrigins::new(),
            instance_displays: HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn run(mut self) -> Result<Instantiated, Vec<InstantiateErr>> {
        let ctx = collect(self.program)?;
        self.generic_structs = ctx.generic_structs;
        self.generic_fns = ctx.generic_fns;
        self.used_struct_names = ctx.used_struct_names;
        self.used_fn_names = ctx.used_fn_names;

        self.concretize_program();

        if self.errors.is_empty() {
            Ok(Instantiated {
                notes: self.notes(),
                resolutions: self.resolutions,
                fn_instances: self.fn_instances,
                struct_instances: self.struct_instances,
                origins: self.origins,
            })
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
    ) -> Option<NodeId> {
        let key = (name.to_string(), args.to_vec());
        if let Some(&decl) = self.struct_registry.get(&key) {
            return Some(decl);
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

        // Register before concretizing the members so self-referential
        // structs hit the registry instead of recursing forever.
        let module = self.generic_structs[name].module;
        let mut decl = scaffold_struct(&self.generic_structs[name].decl);
        self.struct_registry.insert(key, decl.id);
        self.struct_instances.insert(decl.id);
        self.origins
            .insert(decl.id, (name.to_string(), args.to_vec()));
        self.instance_displays.insert(decl.id, display.clone());
        self.generic_structs
            .get_mut(name)
            .unwrap()
            .instances
            .push((display.clone(), use_span.clone()));

        chain.push((display, use_span.clone()));
        self.instance_struct(name, &mut decl, args, chain);
        let id = decl.id;
        self.program.modules[module].module.structs.push(decl);
        for (impl_module, imp) in self.instance_impls(name, id, args, chain) {
            self.program.modules[impl_module].module.impls.push(imp);
        }
        chain.pop();
        Some(id)
    }

    fn instantiate_function(
        &mut self,
        module: usize,
        name: &str,
        args: &[Type],
        use_span: &Span,
        chain: &mut Chain,
    ) -> Option<NodeId> {
        let key = (module, name.to_string(), args.to_vec());
        if let Some(&decl) = self.fn_registry.get(&key) {
            return Some(decl);
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

        // Register before concretizing so self-recursion hits registry
        let mut decl = scaffold_function(&self.generic_fns[&(module, name.to_string())].decl);
        self.fn_registry.insert(key, decl.id);
        self.fn_instances.insert(decl.id);
        self.origins
            .insert(decl.id, (name.to_string(), args.to_vec()));
        self.generic_fns
            .get_mut(&(module, name.to_string()))
            .unwrap()
            .instances
            .push((display.clone(), use_span.clone()));

        chain.push((display, use_span.clone()));
        self.instance_function(module, name, &mut decl, args, chain);
        chain.pop();
        let id = decl.id;
        self.program.modules[module].module.functions.push(decl);
        Some(id)
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
            Type::Struct(sr) => sr
                .target
                .and_then(|t| self.instance_displays.get(&t))
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
    ) -> Result<(LoadedProgram, Instantiated), Vec<InstantiateErr>> {
        let mut program = load(files);
        let instances = Instantiator::new(&mut program).run()?;
        assert!(fully_instantiated(&program));
        Ok((program, instances))
    }

    fn run_one(source: &'static str) -> (LoadedProgram, Instantiated) {
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
        let (program, out) =
            run_one("int main() { return id::<int>(3); } T id<T>(x: T) { return x; }");
        let names = fn_names(&program, 0);
        assert_eq!(names, vec!["main".to_string(), "id".to_string()]);
        let instance = function(&program, 0, "id");
        assert_eq!(instance.node.return_type, Some(Type::Int));
        assert_eq!(instance.node.arguments[0].node.typename, Type::Int);
        assert_eq!(
            out.origins[&instance.id],
            ("id".to_string(), vec![Type::Int])
        );
        assert!(out.fn_instances.contains(&instance.id));
        assert!(out.resolutions.values().any(|&decl| decl == instance.id));
        assert!(body_debug(&program, 0, "main").contains("Identifier(\"id\")"));
    }

    #[test]
    fn test_instantiates_generic_struct_with_impls() {
        let (program, out) = run_one(
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
        assert_eq!(names, vec!["pair".to_string()]);
        let decl = &program.modules[0].module.structs[0];
        assert_eq!(decl.node.members[0].node.typename, Type::Int);
        assert_eq!(
            decl.node.members[1].node.typename,
            Type::Array(Box::new(Type::Char))
        );
        assert_eq!(
            out.origins[&decl.id],
            (
                "pair".to_string(),
                vec![Type::Int, Type::Array(Box::new(Type::Char))]
            )
        );
        assert!(out.struct_instances.contains(&decl.id));
        let imp = &program.modules[0].module.impls[0];
        assert_eq!(imp.node.struct_ref.name.node, "pair");
        assert_eq!(imp.node.struct_ref.target, Some(decl.id));
        let method = &imp.node.functions[0];
        assert!(matches!(
            &method.node.arguments[0].node.typename,
            Type::Struct(sr) if sr.name.node == "pair" && sr.target == Some(decl.id)
        ));
        assert_eq!(method.node.return_type, Some(Type::Int));
    }

    #[test]
    fn test_dedupes_instances() {
        let (program, out) = run_one(
            "int main() { return id::<int>(1) + id::<int>(2); } T id<T>(x: T) { return x; }",
        );
        let count = fn_names(&program, 0).iter().filter(|n| *n == "id").count();
        assert_eq!(count, 1);
        let decl = function(&program, 0, "id").id;
        let mentions: Vec<_> = out.resolutions.values().filter(|&&d| d == decl).collect();
        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn test_generic_calling_generic() {
        let (program, out) = run_one(
            r#"
            T outer<T>(x: T) { return inner::<T>(x); }
            T inner<T>(x: T) { return x; }
            int main() { return outer::<int>(7); }
            "#,
        );
        let names = fn_names(&program, 0);
        assert!(names.contains(&"outer".to_string()), "{names:?}");
        assert!(names.contains(&"inner".to_string()), "{names:?}");
        let inner = function(&program, 0, "inner").id;
        assert!(out.resolutions.values().any(|&decl| decl == inner));
        assert_eq!(out.origins[&inner], ("inner".to_string(), vec![Type::Int]));
    }

    #[test]
    fn test_generic_struct_field_of_generic_struct() {
        let (program, out) = run_one(
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
        assert!(names.contains(&"box".to_string()), "{names:?}");
        assert!(names.contains(&"uses".to_string()), "{names:?}");
        let uses = program.modules[0]
            .module
            .structs
            .iter()
            .find(|s| s.node.name == "uses")
            .unwrap();
        let box_decl = program.modules[0]
            .module
            .structs
            .iter()
            .find(|s| s.node.name == "box")
            .unwrap();
        assert_eq!(out.origins[&box_decl.id], ("box".to_string(), vec![Type::Int]));
        assert!(matches!(
            &uses.node.members[0].node.typename,
            Type::Struct(sr) if sr.target == Some(box_decl.id)
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
        assert_eq!(struct_names(&program, 0), vec!["node".to_string()]);
        let node = &program.modules[0].module.structs[0];
        assert!(matches!(
            &node.node.members[1].node.typename,
            Type::Optional(inner) if matches!(&**inner, Type::Struct(sr) if sr.target == Some(node.id))
        ));
    }

    #[test]
    fn test_nested_instance_args_stay_distinct() {
        let (program, out) = run_one(
            r#"
            struct box<T> { v: T }
            int main() {
                let a = new box<box<int>>{ v: new box<int>{ v: 1 } };
                let b = new box<box<bool>>{ v: new box<bool>{ v: true } };
                return a.v.v;
            }
            "#,
        );
        let boxes: Vec<_> = program.modules[0]
            .module
            .structs
            .iter()
            .filter(|s| s.node.name == "box")
            .collect();
        assert_eq!(boxes.len(), 4);
        let origins: Vec<_> = boxes.iter().map(|s| &out.origins[&s.id]).collect();
        for (i, a) in origins.iter().enumerate() {
            for b in origins.iter().skip(i + 1) {
                assert_ne!(a, b, "nested instances must have distinct origins");
            }
        }
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
        let (program, out) = run_one(
            r#"
            T id<T>(x: T) { return x; }
            int main() {
                let a = id::<int>(1);
                let b = id::<bool>(true);
                return a;
            }
            "#,
        );
        let decl_of = |args: Vec<Type>| {
            let (&id, _) = out
                .origins
                .iter()
                .find(|(_, origin)| **origin == ("id".to_string(), args.clone()))
                .expect("origin");
            program.modules[0]
                .module
                .functions
                .iter()
                .find(|f| f.id == id)
                .expect("instance decl")
        };
        let first = decl_of(vec![Type::Int]);
        let second = decl_of(vec![Type::Bool]);
        assert_eq!(first.node.name, "id");
        assert_eq!(second.node.name, "id");
        assert_ne!(first.id, second.id);
        assert_ne!(first.node.statement.id, second.node.statement.id);
        assert_ne!(first.node.arguments[0].id, second.node.arguments[0].id);
    }

    #[test]
    fn test_qualified_turbofish_instantiates_in_defining_module() {
        let (program, out) = run(&[
            (
                "main.kora",
                r#"import "util.kora"; int main() { return util.make::<int>(5); }"#,
            ),
            ("util.kora", "T make<T>(v: T) { return v; }"),
        ])
        .expect("instantiate");
        assert_eq!(fn_names(&program, 0), vec!["main".to_string()]);
        assert_eq!(fn_names(&program, 1), vec!["make".to_string()]);
        let make = function(&program, 1, "make").id;
        assert!(out.resolutions.values().any(|&decl| decl == make));
    }

    #[test]
    fn test_two_importers_dedupe() {
        let (program, out) = run(&[
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
        assert_eq!(fn_names(&program, util), vec!["make".to_string()]);
        let make = function(&program, util, "make").id;
        let mentions: Vec<_> = out.resolutions.values().filter(|&&d| d == make).collect();
        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn test_generic_named_intrinsic_rejected() {
        expect_err(
            "T copy<T>(x: T) { return x; } int main() { return 0; }",
            "intrinsic",
        );
    }

    #[test]
    fn test_notes_record_instances() {
        let (_, out) = run_one(
            r#"
            struct pair<A, B> { first: A, second: B }
            impl pair<A, B> { A fst(self) { return self.first; } }
            int main() {
                let p = new pair<int, string>{ first: 1, second: "x" };
                return p.fst();
            }
            "#,
        );
        let displays: Vec<&str> = out
            .notes
            .iter()
            .flat_map(|n| n.instances.iter().map(|(d, _)| d.as_str()))
            .collect();
        assert!(displays.contains(&"pair<int, [char]>"), "{displays:?}");
    }
}
