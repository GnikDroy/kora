//! Builds the source graph

mod errors;
mod paths;
pub use errors::*;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::CompileErr;
use crate::lexer::Lexer;
use crate::parser::{Module, Parser, SourceId, Span};
use paths::{invalid_component, module_prefix, resolve_path};

#[derive(Debug)]
pub struct SourceEntry {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug)]
pub struct LoadedModule {
    pub id: SourceId,
    pub module: Module,
    /// Import alias -> the target's module index.
    pub imports: HashMap<String, usize>,
    pub prefix: String,
}

#[derive(Debug)]
pub struct LoadedProgram {
    pub sources: Vec<SourceEntry>,
    pub modules: Vec<LoadedModule>,
}

pub struct Loader<P> {
    provider: P,
    base: PathBuf,
    ids: HashMap<PathBuf, SourceId>,
    requester: HashMap<SourceId, Span>,
    sources: Vec<SourceEntry>,
    queue: VecDeque<SourceId>,
    modules: Vec<LoadedModule>,
    errors: Vec<CompileErr>,
}

impl<P: Fn(&Path) -> Option<String>> Loader<P> {
    pub fn new(provider: P) -> Self {
        Loader {
            provider,
            base: PathBuf::new(),
            ids: HashMap::new(),
            requester: HashMap::new(),
            sources: Vec::new(),
            queue: VecDeque::new(),
            modules: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn schedule(&mut self, path: &Path, requester: Option<Span>) -> SourceId {
        if let Some(&id) = self.ids.get(path) {
            return id;
        }
        let id = SourceId(self.sources.len() as u32);
        self.sources.push(SourceEntry {
            path: path.to_path_buf(),
            text: String::new(),
        });
        self.ids.insert(path.to_path_buf(), id);
        if let Some(span) = requester {
            self.requester.insert(id, span);
        }
        self.queue.push_back(id);
        id
    }

    fn is_std(path: &Path) -> bool {
        path.to_str()
            .is_some_and(|s| s == "std" || s.starts_with("std/"))
    }

    fn process(&mut self, id: SourceId) {
        let path = self.sources[id.0 as usize].path.clone();
        let request = if Self::is_std(&path) {
            path.clone()
        } else {
            self.base.join(&path)
        };
        let Some(text) = (self.provider)(&request) else {
            self.errors.push(CompileErr::Load(LoadErr {
                msg: format!("cannot find source `{}`", request.display()),
                span: self.requester.get(&id).cloned(),
            }));
            return;
        };
        self.sources[id.0 as usize].text = text.clone();

        let tokens = match Lexer::lex(&text) {
            Ok(t) => t,
            Err(e) => {
                self.errors.push(CompileErr::Lex(e));
                return;
            }
        };

        let module = match Parser::with_source(tokens, id).parse() {
            Ok(m) => m,
            Err(e) => {
                self.errors.push(CompileErr::Parse(e));
                return;
            }
        };

        let mut imports = HashMap::new();
        for import in &module.imports {
            if let Some(component) = invalid_component(&import.node.path) {
                self.errors.push(CompileErr::Load(LoadErr {
                    msg: format!("import path component '{component}' must be a valid identifier"),
                    span: Some(import.span.clone()),
                }));
                continue;
            }
            // std/ is reserved
            let target_path = if import.node.path == "std" || import.node.path.starts_with("std/") {
                PathBuf::from(&import.node.path)
            } else {
                let Some(resolved) = resolve_path(&path, &import.node.path) else {
                    self.errors.push(CompileErr::Load(LoadErr {
                        msg: format!("import `{}` climbs above the root", import.node.path),
                        span: Some(import.span.clone()),
                    }));
                    continue;
                };
                resolved
            };
            let target = self.schedule(&target_path, Some(import.span.clone()));
            let local_name = import.node.alias.clone().unwrap_or_else(|| {
                target_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            // modules land in SourceId order, so an id doubles as a module index
            imports.insert(local_name, target.0 as usize);
        }

        let prefix = if id.0 == 0 {
            String::new()
        } else {
            let root = self.sources[0]
                .path
                .parent()
                .unwrap_or_else(|| Path::new(""));
            module_prefix(&path, root)
        };

        self.modules.push(LoadedModule {
            id,
            module,
            imports,
            prefix,
        });
    }

    pub fn load(mut self, entry: &str) -> Result<LoadedProgram, Vec<CompileErr>> {
        let entry_path = Path::new(entry);
        match entry_path.file_name() {
            Some(file) => {
                self.base = entry_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default();
                self.schedule(Path::new(file), None);
            }
            None => self.errors.push(CompileErr::Load(LoadErr {
                msg: format!("`{entry}` does not name a source file"),
                span: None,
            })),
        }
        while let Some(id) = self.queue.pop_front() {
            self.process(id);
        }
        if self.errors.is_empty() {
            Ok(LoadedProgram {
                sources: self.sources,
                modules: self.modules,
            })
        } else {
            Err(self.errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::SourceId;

    fn provider(files: Vec<(&'static str, &'static str)>) -> impl Fn(&Path) -> Option<String> {
        let map: HashMap<String, String> = files
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |path: &Path| path.to_str().and_then(|p| map.get(p)).cloned()
    }

    fn module_of<'a>(program: &'a LoadedProgram, path: &str) -> &'a LoadedModule {
        program
            .modules
            .iter()
            .find(|m| program.sources[m.id.0 as usize].path.to_str() == Some(path))
            .expect("module present")
    }

    #[test]
    fn test_loads_entry_and_one_import() {
        let p = provider(vec![
            (
                "main.kora",
                r#"import "util.kora"; int main() { return 0; }"#,
            ),
            ("util.kora", "int helper() { return 1; }"),
        ]);
        let program = Loader::new(&p).load("main.kora").expect("load");
        assert_eq!(program.modules.len(), 2);

        let main = module_of(&program, "main.kora");
        assert_eq!(main.id, SourceId(0));
        assert_eq!(main.imports.len(), 1);
        assert_eq!(
            program.sources[main.imports["util"]].path.to_str(),
            Some("util.kora")
        );
    }

    #[test]
    fn test_alias_sets_local_name() {
        let p = provider(vec![
            (
                "main.kora",
                r#"import "sub/util.kora" u; int main() { return 0; }"#,
            ),
            ("sub/util.kora", "int helper() { return 1; }"),
        ]);
        let program = Loader::new(&p).load("main.kora").expect("load");
        let main = module_of(&program, "main.kora");
        assert!(main.imports.contains_key("u"));
    }

    #[test]
    fn test_diamond_loads_shared_source_once() {
        let p = provider(vec![
            (
                "main.kora",
                r#"import "b.kora"; import "c.kora"; int main(){return 0;}"#,
            ),
            ("b.kora", r#"import "d.kora"; int fb(){return 0;}"#),
            ("c.kora", r#"import "d.kora"; int fc(){return 0;}"#),
            ("d.kora", "int fd(){return 0;}"),
        ]);
        let program = Loader::new(&p).load("main.kora").expect("load");
        assert_eq!(program.modules.len(), 4);

        let b = module_of(&program, "b.kora");
        let c = module_of(&program, "c.kora");
        assert_eq!(b.imports["d"], c.imports["d"]);
    }

    #[test]
    fn test_cycles_terminate() {
        let p = provider(vec![
            ("a.kora", r#"import "b.kora"; int fa(){return 0;}"#),
            ("b.kora", r#"import "a.kora"; int fb(){return 0;}"#),
        ]);
        let program = Loader::new(&p).load("a.kora").expect("load");
        assert_eq!(program.modules.len(), 2);
    }

    #[test]
    fn test_importer_relative_paths() {
        let p = provider(vec![
            (
                "app/main.kora",
                r#"import "lib/x.kora"; int main(){return 0;}"#,
            ),
            (
                "app/lib/x.kora",
                r#"import "../shared/y.kora"; int fx(){return 0;}"#,
            ),
            ("app/shared/y.kora", "int fy(){return 0;}"),
        ]);
        let program = Loader::new(&p).load("app/main.kora").expect("load");
        assert_eq!(program.modules.len(), 3);
    }

    #[test]
    fn test_import_components_must_be_identifiers() {
        for bad in [
            r#"import "my-lib.kora";"#,
            r#"import "foo.bar/x.kora";"#,
            r#"import "1st.kora";"#,
            r#"import "a b/c.kora";"#,
        ] {
            let source: &'static str =
                Box::leak(format!("{bad} int main() {{ return 0; }}").into_boxed_str());
            let p = provider(vec![("main.kora", source)]);
            let errors = Loader::new(&p).load("main.kora").unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.to_string().contains("must be a valid identifier")),
                "{bad}: {errors:?}"
            );
        }
    }

    #[test]
    fn test_identifier_like_import_components_load() {
        let p = provider(vec![
            (
                "main.kora",
                r#"import "_util/v2.kora"; int main() { return 0; }"#,
            ),
            ("_util/v2.kora", "int f() { return 0; }"),
        ]);
        assert!(Loader::new(&p).load("main.kora").is_ok());
    }

    #[test]
    fn test_entry_may_live_outside_the_working_directory() {
        // Leading `..`s belong to the entry's base, not to import resolution.
        let p = provider(vec![
            (
                "../lib/main.kora",
                r#"import "util.kora"; int main() { return util.f(); }"#,
            ),
            ("../lib/util.kora", "int f() { return 0; }"),
        ]);
        let program = Loader::new(&p).load("../lib/main.kora").expect("load");
        assert_eq!(program.modules.len(), 2);
        assert_eq!(program.sources[0].path.to_str(), Some("main.kora"));
        assert_eq!(program.sources[1].path.to_str(), Some("util.kora"));
    }

    #[test]
    fn test_std_namespace_is_absolute() {
        // std/* resolves to the same path regardless of the importer's location
        let p = provider(vec![
            (
                "app/main.kora",
                r#"import "std/conv"; int main() { return 0; }"#,
            ),
            ("std/conv", r#"import "std/math"; int f() { return 0; }"#),
            ("std/math", "int g() { return 0; }"),
        ]);
        let program = Loader::new(&p).load("app/main.kora").expect("load");
        assert_eq!(program.modules.len(), 3);
        let main = module_of(&program, "main.kora");
        assert_eq!(
            program.sources[main.imports["conv"]].path.to_str(),
            Some("std/conv")
        );
        assert!(module_of(&program, "std/math").imports.is_empty());
    }

    #[test]
    fn test_climbing_above_root_errors() {
        let p = provider(vec![(
            "main.kora",
            r#"import "../secret.kora"; int main(){return 0;}"#,
        )]);
        let errs = Loader::new(&p).load("main.kora").expect_err("should fail");
        assert!(errs.iter().any(|e| matches!(
            e,
            CompileErr::Load(LoadErr { msg, .. }) if msg.contains("climbs above the root")
        )));
    }

    #[test]
    fn test_missing_source_errors_against_importer() {
        let p = provider(vec![(
            "main.kora",
            r#"import "gone.kora"; int main(){return 0;}"#,
        )]);
        let errs = Loader::new(&p).load("main.kora").expect_err("should fail");
        assert_eq!(errs.len(), 1);
        let CompileErr::Load(LoadErr { msg, span }) = &errs[0] else {
            panic!("expected a load error");
        };
        assert!(msg.contains("gone.kora"));
        assert!(span.is_some());
    }
}
