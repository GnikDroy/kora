mod collect;
mod concretize;
mod errors;
mod types;

pub use errors::*;
pub use types::{InstanceOrigin, InstanceOrigins};

use std::collections::{HashMap, HashSet};

use crate::loader::LoadedProgram;
use crate::parser::{NodeId, Span, Type};

use collect::{GenericFns, GenericStructs, collect};
use concretize::{scaffold_function, scaffold_struct};
use types::{InstanceRegistry, InstantiationSite, InstantiationStack};

const DEPTH_LIMIT: usize = 64;

#[derive(Default)]
pub struct Instantiated {
    pub regions: Vec<GenericRegion>,
    pub resolutions: HashMap<NodeId, NodeId>,
    pub fn_instances: HashSet<NodeId>,
    pub struct_instances: HashSet<NodeId>,
    pub origins: InstanceOrigins,
}

pub struct Instantiator<'p> {
    program: &'p mut LoadedProgram,

    imports: Vec<HashMap<String, usize>>,
    concrete_structs: HashMap<String, NodeId>,

    generic_structs: GenericStructs,
    generic_fns: GenericFns,

    instance_registry: InstanceRegistry,
    instantiation_sites: HashMap<NodeId, Vec<InstantiationSite>>,

    output: Instantiated,
    errors: Vec<InstantiateErr>,
}

impl<'p> Instantiator<'p> {
    pub fn new(program: &'p mut LoadedProgram) -> Instantiator<'p> {
        let imports = program.modules.iter().map(|m| m.imports.clone()).collect();

        Instantiator {
            program,
            imports,
            generic_structs: HashMap::new(),
            generic_fns: GenericFns::new(),
            concrete_structs: HashMap::new(),
            instance_registry: InstanceRegistry::default(),
            output: Instantiated::default(),
            instantiation_sites: HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn run(mut self) -> Result<Instantiated, Vec<InstantiateErr>> {
        let ctx = collect(self.program)?;
        self.generic_structs = ctx.generic_structs;
        self.generic_fns = ctx.generic_fns;
        self.concrete_structs = ctx.concrete_structs;

        self.concretize_program();

        if self.errors.is_empty() {
            self.output.regions = regions(
                &self.generic_structs,
                &self.generic_fns,
                &self.instantiation_sites,
                &self.output.origins,
            );
            Ok(self.output)
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

    fn instantiate_struct(
        &mut self,
        name: &str,
        args: &[Type],
        use_span: &Span,
        stack: &mut InstantiationStack,
    ) -> Option<NodeId> {
        let def = &self.generic_structs[name];
        let generic = def.decl.id;
        if let Some(decl) = self.instance_registry.get(generic, args) {
            return Some(decl);
        }

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
        let display = display_instance(name, args, &self.output.origins);
        if stack.depth() >= DEPTH_LIMIT {
            self.error(
                format!(
                    "instantiation depth limit ({DEPTH_LIMIT}) exceeded at `{display}`: {}",
                    stack.summary()
                ),
                use_span,
            );
            return None;
        }

        // Register before concretizing so self-recursion hits registry
        let mut decl = scaffold_struct(&self.generic_structs[name].decl);
        let module = decl.span.source.0 as usize;
        self.instance_registry
            .insert(generic, args.to_vec(), decl.id);
        self.output.struct_instances.insert(decl.id);
        self.output.origins.insert(
            decl.id,
            InstanceOrigin {
                generic: name.to_string(),
                args: args.to_vec(),
            },
        );
        self.instantiation_sites
            .entry(generic)
            .or_default()
            .push(InstantiationSite {
                args: args.to_vec(),
                span: use_span.clone(),
            });

        stack.push(display, use_span.clone());
        self.instance_struct(name, &mut decl, args, stack);
        let id = decl.id;
        self.program.modules[module].module.structs.push(decl);
        for (impl_module, imp) in self.instance_impls(name, id, args, stack) {
            self.program.modules[impl_module].module.impls.push(imp);
        }
        stack.pop();
        Some(id)
    }

    fn instantiate_function(
        &mut self,
        module: usize,
        name: &str,
        args: &[Type],
        use_span: &Span,
        stack: &mut InstantiationStack,
    ) -> Option<NodeId> {
        let def = &self.generic_fns[module][name];
        let generic = def.decl.id;
        if let Some(decl) = self.instance_registry.get(generic, args) {
            return Some(decl);
        }

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
        let display = display_instance(name, args, &self.output.origins);
        if stack.depth() >= DEPTH_LIMIT {
            self.error(
                format!(
                    "instantiation depth limit ({DEPTH_LIMIT}) exceeded at `{display}`: {}",
                    stack.summary()
                ),
                use_span,
            );
            return None;
        }

        // Register before concretizing so self-recursion hits registry
        let mut decl = scaffold_function(&self.generic_fns[module][name].decl);
        self.instance_registry
            .insert(generic, args.to_vec(), decl.id);
        self.output.fn_instances.insert(decl.id);
        self.output.origins.insert(
            decl.id,
            InstanceOrigin {
                generic: name.to_string(),
                args: args.to_vec(),
            },
        );
        self.instantiation_sites
            .entry(generic)
            .or_default()
            .push(InstantiationSite {
                args: args.to_vec(),
                span: use_span.clone(),
            });

        stack.push(display, use_span.clone());
        self.instance_function(module, name, &mut decl, args, stack);
        stack.pop();
        let id = decl.id;
        self.program.modules[module].module.functions.push(decl);
        Some(id)
    }
}

pub(crate) fn display_type(ty: &Type, origins: &InstanceOrigins) -> String {
    match ty {
        Type::Int => "int".to_string(),
        Type::Real => "real".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Char => "char".to_string(),
        Type::Opaque => "opaque".to_string(),
        Type::Array(inner) => format!("[{}]", display_type(inner, origins)),
        Type::Optional(inner) => format!("{}?", display_type(inner, origins)),
        Type::Struct(sr) => sr
            .target
            .and_then(|t| origins.get(&t))
            .map(|origin| display_instance(&origin.generic, &origin.args, origins))
            .unwrap_or_else(|| sr.name.node.clone()),
        Type::Generic(name, args) => display_instance(&name.node, args, origins),
        Type::Function(_, _) => "fn".to_string(),
    }
}

fn display_instance(name: &str, args: &[Type], origins: &InstanceOrigins) -> String {
    let args = args
        .iter()
        .map(|a| display_type(a, origins))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}<{args}>")
}

fn regions(
    structs: &GenericStructs,
    fns: &GenericFns,
    instantiation_sites: &HashMap<NodeId, Vec<InstantiationSite>>,
    origins: &InstanceOrigins,
) -> Vec<GenericRegion> {
    let render = |name: &str, generic: NodeId| -> Vec<(String, Span)> {
        instantiation_sites
            .get(&generic)
            .into_iter()
            .flatten()
            .map(|site| {
                (
                    display_instance(name, &site.args, origins),
                    site.span.clone(),
                )
            })
            .collect()
    };

    let mut regions = Vec::new();
    for (name, def) in structs.iter() {
        let instances = render(name, def.decl.id);
        let spans =
            std::iter::once(&def.decl.span).chain(def.impls.iter().map(|imp| &imp.span));
        for span in spans {
            regions.push(GenericRegion {
                span: span.clone(),
                instances: instances.clone(),
            });
        }
    }
    for (name, def) in fns.iter().flatten() {
        regions.push(GenericRegion {
            span: def.decl.span.clone(),
            instances: render(name, def.decl.id),
        });
    }
    regions
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
            InstanceOrigin {
                generic: "id".to_string(),
                args: vec![Type::Int]
            }
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
            InstanceOrigin {
                generic: "pair".to_string(),
                args: vec![Type::Int, Type::Array(Box::new(Type::Char))]
            }
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
        assert_eq!(
            out.origins[&inner],
            InstanceOrigin {
                generic: "inner".to_string(),
                args: vec![Type::Int]
            }
        );
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
        assert_eq!(
            out.origins[&box_decl.id],
            InstanceOrigin {
                generic: "box".to_string(),
                args: vec![Type::Int]
            }
        );
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
                .find(|(_, origin)| origin.generic == "id" && origin.args == args)
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
    fn test_concrete_mentions_get_targets() {
        let (program, _) = run_one(
            r#"
            struct Point { x: int }
            struct Holder { p: Point }
            impl Point { int get(self) { return self.x; } }
            Point make(q: Point) { return q; }
            int main() { return 0; }
            "#,
        );
        let module = &program.modules[0].module;
        let point = module.structs[0].id;
        assert!(matches!(
            &module.structs[1].node.members[0].node.typename,
            Type::Struct(sr) if sr.target == Some(point)
        ));
        assert_eq!(module.impls[0].node.struct_ref.target, Some(point));
        assert!(matches!(
            &module.impls[0].node.functions[0].node.arguments[0].node.typename,
            Type::Struct(sr) if sr.target == Some(point)
        ));
        let make = function(&program, 0, "make");
        assert!(matches!(
            &make.node.return_type,
            Some(Type::Struct(sr)) if sr.target == Some(point)
        ));
        assert!(matches!(
            &make.node.arguments[0].node.typename,
            Type::Struct(sr) if sr.target == Some(point)
        ));
    }

    #[test]
    fn test_generic_named_intrinsic_rejected() {
        expect_err(
            "T copy<T>(x: T) { return x; } int main() { return 0; }",
            "intrinsic",
        );
    }

    #[test]
    fn test_regions_record_instances() {
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
            .regions
            .iter()
            .flat_map(|r| r.instances.iter().map(|(d, _)| d.as_str()))
            .collect();
        assert!(displays.contains(&"pair<int, [char]>"), "{displays:?}");
    }
}
