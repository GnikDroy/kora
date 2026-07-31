//! The Kora standard library, written in Kora

use std::path::Path;

const MODULES: &[(&str, &str)] = &[
    ("std/conv", include_str!("../runtime/std/conv.kora")),
    ("std/io", include_str!("../runtime/std/io.kora")),
    ("std/term", include_str!("../runtime/std/term.kora")),
    ("std/math", include_str!("../runtime/std/math.kora")),
    ("std/str", include_str!("../runtime/std/str.kora")),
    ("std/time", include_str!("../runtime/std/time.kora")),
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
