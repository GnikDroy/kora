use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::loader::LoadedProgram;
use crate::parser::{NodeId, Type};

pub(crate) type InstanceOrigins = HashMap<NodeId, (String, Vec<Type>)>;

pub(crate) fn mangle(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        format!("kora${name}")
    } else {
        format!("kora${prefix}${name}")
    }
}

pub(crate) fn mangle_method(struct_name: &str, name: &str) -> String {
    format!("kora$${struct_name}${name}")
}

pub(crate) fn mangle_prefix(path: &Path, root: &Path) -> String {
    let rel = if path.is_absolute() == root.is_absolute() {
        relative_to(path, root)
    } else {
        path.to_path_buf()
    };
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".to_string()),
            _ => None,
        })
        .collect();
    let joined = parts.join("$");
    joined
        .strip_suffix(".kora")
        .unwrap_or(&joined)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '$' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn encode_instance(name: &str, args: &[Type], origins: &InstanceOrigins) -> String {
    let mut encoded = name.to_string();
    for arg in args {
        encoded.push_str("$$");
        encoded.push_str(&encode_type(arg, origins));
    }
    encoded
}

fn encode_type(ty: &Type, origins: &InstanceOrigins) -> String {
    match ty {
        Type::Int => "int".to_string(),
        Type::Real => "real".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Char => "char".to_string(),
        Type::Opaque => "opaque".to_string(),
        Type::Array(inner) => format!("arr_{}", encode_type(inner, origins)),
        Type::Optional(inner) => format!("opt_{}", encode_type(inner, origins)),
        Type::Struct(sr) => sr
            .target
            .and_then(|t| origins.get(&t))
            .map(|(generic, args)| encode_instance(generic, args, origins))
            .unwrap_or_else(|| sr.name.node.clone()),
        Type::Generic(name, _) => name.node.clone(),
        Type::Function(_, _) => "fn".to_string(),
    }
}

pub(crate) fn emitted_names(
    program: &LoadedProgram,
    origins: &InstanceOrigins,
) -> HashMap<NodeId, String> {
    let mut emitted = HashMap::new();

    let mut taken: HashSet<String> = program
        .modules
        .iter()
        .flat_map(|m| m.module.structs.iter())
        .filter(|s| !origins.contains_key(&s.id))
        .map(|s| s.node.name.clone())
        .collect();
    for module in program.modules.iter() {
        for decl in module.module.structs.iter() {
            if let Some((generic, args)) = origins.get(&decl.id) {
                let name = unique_name(encode_instance(generic, args, origins), |candidate| {
                    taken.contains(candidate)
                });
                taken.insert(name.clone());
                emitted.insert(decl.id, name);
            }
        }
    }

    for module in program.modules.iter() {
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
                    .filter(|f| !origins.contains_key(&f.id))
                    .map(|f| f.node.name.clone()),
            )
            .collect();
        for decl in module.module.functions.iter() {
            if let Some((generic, args)) = origins.get(&decl.id) {
                let name = unique_name(encode_instance(generic, args, origins), |candidate| {
                    taken.contains(candidate)
                });
                taken.insert(name.clone());
                emitted.insert(decl.id, name);
            }
        }
    }

    emitted
}

pub(crate) fn unique_name(base: String, taken: impl Fn(&str) -> bool) -> String {
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

fn relative_to(path: &Path, root: &Path) -> PathBuf {
    let mut path_iter = path.components();
    let mut root_iter = root.components();
    loop {
        match (path_iter.clone().next(), root_iter.clone().next()) {
            (Some(p), Some(r)) if p == r => {
                path_iter.next();
                root_iter.next();
            }
            _ => break,
        }
    }
    let mut rel = PathBuf::new();
    for _ in root_iter {
        rel.push("..");
    }
    for c in path_iter {
        rel.push(c.as_os_str());
    }
    rel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_is_relative_to_the_entry_directory() {
        let root = Path::new("/home/dev/game");
        assert_eq!(
            mangle_prefix(Path::new("/home/dev/game/util.kora"), root),
            "util"
        );
        assert_eq!(
            mangle_prefix(Path::new("/home/dev/game/lib/geo.kora"), root),
            "lib$geo"
        );
    }

    #[test]
    fn test_std_paths_pass_through() {
        assert_eq!(
            mangle_prefix(Path::new("std/io"), Path::new("/home/dev/game")),
            "std$io"
        );
        assert_eq!(mangle_prefix(Path::new("std/io"), Path::new("")), "std$io");
    }

    #[test]
    fn test_imports_above_the_root_stay_distinct() {
        let root = Path::new("/home/dev/game");
        let above = mangle_prefix(Path::new("/home/dev/shared.kora"), root);
        let inside = mangle_prefix(Path::new("/home/dev/game/shared.kora"), root);
        assert_eq!(above, "__$shared");
        assert_ne!(above, inside);
    }

    #[test]
    fn test_prefix_chars_are_identifier_safe() {
        let prefix = mangle_prefix(
            Path::new("/tmp/kora-e2e.1/my lib/a.b.kora"),
            Path::new("/tmp/kora-e2e.1"),
        );
        assert_eq!(prefix, "my_lib$a_b");
    }
}
