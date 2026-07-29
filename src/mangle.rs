use std::path::{Component, Path};

pub(crate) fn mangle(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        format!("kora${name}")
    } else {
        format!("kora${prefix}${name}")
    }
}

pub(crate) fn mangle_prefix(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let joined = parts.join("$");
    joined.strip_suffix(".kora").unwrap_or(&joined).to_string()
}
