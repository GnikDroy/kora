use std::path::{Component, Path, PathBuf};

fn component_name(c: Component<'_>) -> Option<String> {
    match c {
        Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
        Component::ParentDir => Some("..".to_string()),
        _ => None,
    }
}

fn sanitize_symbol(s: &str) -> String {
    let sanitizer = |c: char| {
        if c.is_ascii_alphanumeric() || c == '$' {
            c
        } else {
            '_'
        }
    };

    s.chars().map(sanitizer).collect()
}

pub(super) fn to_logical(path: &Path) -> PathBuf {
    match path.to_str() {
        Some(s) => PathBuf::from(s.replace('\\', "/")),
        None => path.to_path_buf(),
    }
}

pub(super) fn module_prefix(path: &Path, root: &Path) -> String {
    // std/* is a fixed namespace, never relative to the entry directory.
    let relative = make_relative(path, root);
    let target = if path.starts_with("std") {
        path
    } else {
        relative.as_path()
    };

    let joined = target
        .components()
        .filter_map(component_name)
        .collect::<Vec<_>>()
        .join("$");

    sanitize_symbol(joined.strip_suffix(".kora").unwrap_or(&joined))
}

fn make_relative(path: &Path, root: &Path) -> PathBuf {
    if path.is_absolute() == root.is_absolute() {
        relative_to(path, root)
    } else {
        path.to_path_buf()
    }
}

fn relative_to(path: &Path, root: &Path) -> PathBuf {
    let path: Vec<_> = path.components().collect();
    let root: Vec<_> = root.components().collect();

    let common = path.iter().zip(&root).take_while(|(a, b)| a == b).count();

    let mut rel = PathBuf::new();

    for _ in &root[common..] {
        rel.push("..");
    }

    for component in &path[common..] {
        rel.push(component.as_os_str());
    }

    rel
}

pub(super) fn resolve_path(importer: &Path, rel: &str) -> Option<PathBuf> {
    let mut joined = importer
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    joined.push(rel);

    let mut out: Vec<String> = Vec::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop()?;
            }
            other => out.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    Some(PathBuf::from(out.join("/")))
}

pub(super) fn invalid_component(rel: &str) -> Option<String> {
    for component in Path::new(rel).components() {
        let Component::Normal(name) = component else {
            continue;
        };
        let name = name.to_string_lossy();
        let name = name.strip_suffix(".kora").unwrap_or(&name);
        let mut chars = name.chars();
        let valid = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_is_relative_to_the_entry_directory() {
        let root = Path::new("/home/dev/game");
        assert_eq!(
            module_prefix(Path::new("/home/dev/game/util.kora"), root),
            "util"
        );
        assert_eq!(
            module_prefix(Path::new("/home/dev/game/lib/geo.kora"), root),
            "lib$geo"
        );
    }

    #[test]
    fn test_std_paths_pass_through() {
        assert_eq!(
            module_prefix(Path::new("std/io"), Path::new("/home/dev/game")),
            "std$io"
        );
        assert_eq!(module_prefix(Path::new("std/io"), Path::new("")), "std$io");
    }

    #[test]
    fn test_imports_above_the_root_stay_distinct() {
        let root = Path::new("/home/dev/game");
        let above = module_prefix(Path::new("/home/dev/shared.kora"), root);
        let inside = module_prefix(Path::new("/home/dev/game/shared.kora"), root);
        assert_eq!(above, "__$shared");
        assert_ne!(above, inside);
    }

    #[test]
    fn test_prefix_chars_are_identifier_safe() {
        let prefix = module_prefix(
            Path::new("/tmp/kora-e2e.1/my lib/a.b.kora"),
            Path::new("/tmp/kora-e2e.1"),
        );
        assert_eq!(prefix, "my_lib$a_b");
    }
}
