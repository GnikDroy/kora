use std::collections::{HashMap, HashSet};

use super::InstantiateErr;
use crate::loader::LoadedProgram;
use crate::parser::{GenericFunction, GenericImpl, GenericStruct, Span, Spanned};

pub(super) struct GenericStructDef {
    pub(super) module: usize,
    pub(super) decl: Spanned<GenericStruct>,
    pub(super) impls: Vec<(usize, Spanned<GenericImpl>)>,
    pub(super) instances: Vec<(String, Span)>,
}

pub(super) struct GenericFnDef {
    pub(super) decl: Spanned<GenericFunction>,
    pub(super) instances: Vec<(String, Span)>,
}

pub(super) type GenericStructs = HashMap<String, GenericStructDef>;
pub(super) type GenericFns = HashMap<(usize, String), GenericFnDef>;

pub(super) struct InstantiateCtx {
    pub(super) generic_structs: GenericStructs,
    pub(super) generic_fns: GenericFns,
    pub(super) used_struct_names: HashSet<String>,
    pub(super) used_fn_names: Vec<HashSet<String>>,
}

pub(super) fn collect(program: &LoadedProgram) -> Result<InstantiateCtx, Vec<InstantiateErr>> {
    let mut errors = Vec::new();
    let structs = collect_generic_structs(program, &mut errors);
    let fns = collect_generic_functions(program, &mut errors);
    let structs = collect_generic_impls(program, structs, &mut errors);

    if !errors.is_empty() {
        return Err(errors);
    }

    let concrete_struct_names = program
        .modules
        .iter()
        .flat_map(|m| m.module.structs.iter())
        .map(|s| s.node.name.clone())
        .collect();

    let concrete_fn_names = program
        .modules
        .iter()
        .map(|m| {
            m.module
                .functions
                .iter()
                .map(|f| f.node.name.clone())
                .collect()
        })
        .collect();

    let ctx = InstantiateCtx {
        generic_structs: structs,
        generic_fns: fns,
        used_struct_names: concrete_struct_names,
        used_fn_names: concrete_fn_names,
    };

    let errors = validate(program, &ctx);

    if errors.is_empty() {
        Ok(ctx)
    } else {
        Err(errors)
    }
}

fn collect_generic_structs(
    program: &LoadedProgram,
    errors: &mut Vec<InstantiateErr>,
) -> GenericStructs {
    let mut structs = GenericStructs::new();
    for (m, module) in program.modules.iter().enumerate() {
        for decl in module.module.generic_structs.iter() {
            let name = decl.node.name.clone();
            if structs.contains_key(&name) {
                errors.push(InstantiateErr {
                    msg: format!("struct `{name}` is declared multiple times"),
                    span: decl.span.clone(),
                });
                continue;
            }
            structs.insert(
                name,
                GenericStructDef {
                    module: m,
                    decl: decl.clone(),
                    impls: Vec::new(),
                    instances: Vec::new(),
                },
            );
        }
    }
    structs
}

fn collect_generic_functions(
    program: &LoadedProgram,
    errors: &mut Vec<InstantiateErr>,
) -> GenericFns {
    let mut fns = GenericFns::new();
    for (m, module) in program.modules.iter().enumerate() {
        for decl in module.module.generic_functions.iter() {
            let name = decl.node.name.clone();
            if fns.contains_key(&(m, name.clone())) {
                errors.push(InstantiateErr {
                    msg: format!("function `{name}` is declared multiple times"),
                    span: decl.span.clone(),
                });
                continue;
            }
            fns.insert(
                (m, name),
                GenericFnDef {
                    decl: decl.clone(),
                    instances: Vec::new(),
                },
            );
        }
    }
    fns
}

fn collect_generic_impls(
    program: &LoadedProgram,
    mut structs: GenericStructs,
    errors: &mut Vec<InstantiateErr>,
) -> GenericStructs {
    for (m, module) in program.modules.iter().enumerate() {
        for imp in module.module.generic_impls.iter() {
            let struct_name = &imp.node.struct_name.node;
            let Some(def) = structs.get_mut(struct_name) else {
                errors.push(InstantiateErr {
                    msg: format!(
                        "impl for `{struct_name}` declares type parameters but `{struct_name}` is not a generic struct"
                    ),
                    span: imp.node.struct_name.span.clone(),
                });
                continue;
            };
            def.impls.push((m, imp.clone()));
        }
    }
    structs
}

fn validate(program: &LoadedProgram, ctx: &InstantiateCtx) -> Vec<InstantiateErr> {
    let mut errors = Vec::new();
    let error = |errors: &mut Vec<InstantiateErr>, msg: String, span: &Span| {
        errors.push(InstantiateErr {
            msg,
            span: span.clone(),
        });
    };

    let check_params = |errors: &mut Vec<InstantiateErr>, params: &[Spanned<String>]| {
        let mut seen = HashSet::new();
        for param in params {
            if !seen.insert(param.node.clone()) {
                error(
                    errors,
                    format!("duplicate type parameter `{}`", param.node),
                    &param.span,
                );
            }
            if ctx.used_struct_names.contains(&param.node)
                || ctx.generic_structs.contains_key(&param.node)
            {
                error(
                    errors,
                    format!(
                        "type parameter `{}` shadows struct `{}`",
                        param.node, param.node
                    ),
                    &param.span,
                );
            }
        }
    };

    for def in ctx.generic_structs.values() {
        let name = &def.decl.node.name;
        if ctx.used_struct_names.contains(name) {
            error(
                &mut errors,
                format!("struct `{name}` is declared multiple times"),
                &def.decl.span,
            );
        }
        check_params(&mut errors, &def.decl.node.type_params);
        let expected = def.decl.node.type_params.len();
        for (_, imp) in &def.impls {
            check_params(&mut errors, &imp.node.type_params);
            let found = imp.node.type_params.len();
            if found != expected {
                error(
                    &mut errors,
                    format!(
                        "impl for `{name}` must declare the struct's {expected} type parameter(s), found {found}"
                    ),
                    &imp.node.struct_name.span,
                );
            }
        }
    }

    for ((module, name), def) in ctx.generic_fns.iter() {
        if ctx.used_fn_names[*module].contains(name) {
            error(
                &mut errors,
                format!("function `{name}` is declared multiple times"),
                &def.decl.span,
            );
        }
        check_params(&mut errors, &def.decl.node.type_params);
    }
    if let Some(def) = ctx.generic_fns.get(&(0, "main".to_string())) {
        error(
            &mut errors,
            "main cannot be generic".to_string(),
            &def.decl.span,
        );
    }

    for module in program.modules.iter() {
        for imp in module.module.impls.iter() {
            if let Some(def) = ctx.generic_structs.get(&imp.node.struct_name.node) {
                let expected = def.decl.node.type_params.len();
                error(
                    &mut errors,
                    format!(
                        "impl for `{}` must declare the struct's {expected} type parameter(s), found 0",
                        imp.node.struct_name.node
                    ),
                    &imp.node.struct_name.span,
                );
            }
        }
    }

    errors
}
