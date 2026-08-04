use std::collections::{HashMap, HashSet};

use crate::instantiate::{InstanceOrigin, InstanceOrigins};
use crate::loader::LoadedProgram;
use crate::parser::{NodeId, Type};

pub(crate) fn emitted_symbols(
    program: &LoadedProgram,
    origins: &InstanceOrigins,
) -> HashMap<NodeId, String> {
    let mut emitter = SymbolEmitter {
        program,
        origins,
        emitted: HashMap::new(),
    };
    emitter.emit_structs();
    emitter.emit_functions();
    emitter.emit_methods();
    emitter.emitted
}

struct SymbolEmitter<'a> {
    program: &'a LoadedProgram,
    origins: &'a InstanceOrigins,
    emitted: HashMap<NodeId, String>,
}

impl SymbolEmitter<'_> {
    fn emit_structs(&mut self) {
        let mut taken: HashSet<String> = self
            .program
            .modules
            .iter()
            .flat_map(|m| m.module.structs.iter())
            .filter(|s| !self.origins.contains_key(&s.id))
            .map(|s| s.node.name.clone())
            .collect();

        for module in self.program.modules.iter() {
            for decl in module.module.structs.iter() {
                let name = self.base_name(decl.id, &decl.node.name, &mut taken);
                self.emitted.insert(decl.id, name);
            }
        }
    }

    fn emit_functions(&mut self) {
        for (m, module) in self.program.modules.iter().enumerate() {
            let mut taken: HashSet<String> = module
                .module
                .extern_functions
                .iter()
                .map(|f| f.node.name.clone())
                .chain(
                    module
                        .module
                        .functions
                        .iter()
                        .filter(|f| !self.origins.contains_key(&f.id))
                        .map(|f| f.node.name.clone()),
                )
                .collect();

            for func in module.module.extern_functions.iter() {
                self.emitted.insert(func.id, func.node.name.clone());
            }
            for decl in module.module.functions.iter() {
                let base = self.base_name(decl.id, &decl.node.name, &mut taken);
                let symbol = function_symbol(m == 0, &module.prefix, &base);
                self.emitted.insert(decl.id, symbol);
            }
        }
    }

    fn emit_methods(&mut self) {
        for module in self.program.modules.iter() {
            for imp in module.module.impls.iter() {
                let base = imp
                    .node
                    .struct_ref
                    .target
                    .and_then(|t| self.emitted.get(&t))
                    .cloned()
                    .unwrap_or_else(|| imp.node.struct_ref.name.node.clone());
                for method in imp.node.functions.iter() {
                    self.emitted
                        .insert(method.id, method_symbol(&base, &method.node.name));
                }
            }
        }
    }

    fn base_name(&self, decl: NodeId, source_name: &str, taken: &mut HashSet<String>) -> String {
        let Some(InstanceOrigin { generic, args }) = self.origins.get(&decl) else {
            return source_name.to_string();
        };
        let name = unique_name(
            instance_base_name(generic, args, self.origins),
            |candidate| taken.contains(candidate),
        );
        taken.insert(name.clone());
        name
    }
}

fn function_symbol(is_entry: bool, prefix: &str, base: &str) -> String {
    if is_entry && base == "main" {
        "__kora_main".to_string()
    } else if prefix.is_empty() {
        format!("kora${base}")
    } else {
        format!("kora${prefix}${base}")
    }
}

/// $$ cannot occur in a module prefix, so methods can never collide with module-prefixed functions.
fn method_symbol(struct_base: &str, name: &str) -> String {
    format!("kora$${struct_base}${name}")
}

/// The generic's name plus $$ separated encoded types, pair$$int$$arr_char.
fn instance_base_name(generic: &str, args: &[Type], origins: &InstanceOrigins) -> String {
    let mut encoded = generic.to_string();
    for arg in args {
        encoded.push_str("$$");
        encoded.push_str(&encoded_type_name(arg, origins));
    }
    encoded
}

fn encoded_type_name(ty: &Type, origins: &InstanceOrigins) -> String {
    match ty {
        Type::Int => "int".to_string(),
        Type::Real => "real".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Char => "char".to_string(),
        Type::Opaque => "opaque".to_string(),
        Type::Array(inner) => format!("arr_{}", encoded_type_name(inner, origins)),
        Type::Optional(inner) => format!("opt_{}", encoded_type_name(inner, origins)),
        Type::Struct(sr) => sr
            .target
            .and_then(|t| origins.get(&t))
            .map(|origin| instance_base_name(&origin.generic, &origin.args, origins))
            .unwrap_or_else(|| sr.name.node.clone()),
        Type::Generic(sr, _) => sr.name.node.clone(),
        Type::Function(_, _) => "fn".to_string(),
    }
}

fn unique_name(base: String, taken: impl Fn(&str) -> bool) -> String {
    if !taken(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}${n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}
