//! The Kora standard library, written in Kora

use std::path::Path;

const MODULES: &[(&str, &str)] = &[
    ("std/libc", include_str!("../runtime/std/libc.kora")),
    ("std/fs", include_str!("../runtime/std/fs.kora")),
    ("std/env", include_str!("../runtime/std/env.kora")),
    ("std/proc", include_str!("../runtime/std/proc.kora")),
    ("std/conv", include_str!("../runtime/std/conv.kora")),
    ("std/io", include_str!("../runtime/std/io.kora")),
    ("std/term", include_str!("../runtime/std/term.kora")),
    ("std/math", include_str!("../runtime/std/math.kora")),
    ("std/str", include_str!("../runtime/std/str.kora")),
    ("std/time", include_str!("../runtime/std/time.kora")),
    (
        "std/algorithm",
        include_str!("../runtime/std/algorithm.kora"),
    ),
    (
        "std/collections/hasher",
        include_str!("../runtime/std/collections/hasher.kora"),
    ),
    (
        "std/collections/map",
        include_str!("../runtime/std/collections/map.kora"),
    ),
    (
        "std/collections/set",
        include_str!("../runtime/std/collections/set.kora"),
    ),
    (
        "std/collections/stack",
        include_str!("../runtime/std/collections/stack.kora"),
    ),
    (
        "std/collections/queue",
        include_str!("../runtime/std/collections/queue.kora"),
    ),
    (
        "std/collections/list",
        include_str!("../runtime/std/collections/list.kora"),
    ),
];

pub fn is_std_path(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|s| s == "std" || s.starts_with("std/"))
}

pub fn source(path: &Path) -> Option<String> {
    let key = path.to_str()?;
    MODULES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, src)| src.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_modules_resolve_by_extensionless_path() {
        assert!(source(Path::new("std/conv")).is_some());
        assert!(source(Path::new("std/math")).is_some());
        assert!(source(Path::new("std/str")).is_some());
        assert!(source(Path::new("std/missing")).is_none());
        assert!(source(Path::new("conv")).is_none());
    }

    #[test]
    fn test_is_std_path() {
        assert!(is_std_path(Path::new("std/conv")));
        assert!(is_std_path(Path::new("std")));
        assert!(!is_std_path(Path::new("app/std_helpers")));
        assert!(!is_std_path(Path::new("util")));
    }
}
