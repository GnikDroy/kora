//! Builds the source graph

mod errors;
pub use errors::*;

use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};

use crate::lexer::Lexer;
use crate::parser::{Module, Parser, SourceId, Span};

#[derive(Debug)]
pub struct SourceEntry {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug)]
pub struct ResolvedImport {
    pub local_name: String,
    pub target: SourceId,
    pub span: Span,
}

#[derive(Debug)]
pub struct LoadedModule {
    pub id: SourceId,
    pub module: Module,
    pub imports: Vec<ResolvedImport>,
}

#[derive(Debug)]
pub struct LoadedProgram {
    pub sources: Vec<SourceEntry>,
    pub modules: Vec<LoadedModule>,
}

fn resolve_path(importer: &Path, rel: &str) -> Option<PathBuf> {
    let mut joined = importer
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    joined.push(rel);

    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    // None when we try to escape root.
                    return None;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

pub struct Loader<P> {
    provider: P,
    ids: HashMap<PathBuf, SourceId>,
    requester: HashMap<SourceId, Span>,
    sources: Vec<SourceEntry>,
    queue: VecDeque<SourceId>,
    modules: Vec<LoadedModule>,
    errors: Vec<LoadError>,
}

impl<P: Fn(&Path) -> Option<String>> Loader<P> {
    pub fn new(provider: P) -> Self {
        Loader {
            provider,
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

    fn process(&mut self, id: SourceId) {
        let path = self.sources[id.0 as usize].path.clone();
        let Some(text) = (self.provider)(&path) else {
            self.errors.push(LoadError {
                msg: format!("cannot find source `{}`", path.display()),
                source: id,
                span: self.requester.get(&id).cloned(),
            });
            return;
        };
        self.sources[id.0 as usize].text = text.clone();

        let tokens = match Lexer::lex(&text) {
            Ok(t) => t,
            Err(e) => {
                self.errors.push(LoadError {
                    msg: e.to_string(),
                    source: id,
                    span: Some(Span {
                        source: id,
                        start: e.position.clone(),
                        end: e.position,
                    }),
                });
                return;
            }
        };

        let module = match Parser::with_source(tokens, id).parse() {
            Ok(m) => m,
            Err(e) => {
                let span = e.token.as_ref().map(|t| Span {
                    source: id,
                    start: t.start.clone(),
                    end: t.end.clone(),
                });
                self.errors.push(LoadError {
                    msg: e.to_string(),
                    source: id,
                    span,
                });
                return;
            }
        };

        let mut imports = Vec::new();
        for import in &module.imports {
            let Some(target_path) = resolve_path(&path, &import.node.path) else {
                self.errors.push(LoadError {
                    msg: format!("import `{}` climbs above the root", import.node.path),
                    source: id,
                    span: Some(import.span.clone()),
                });
                continue;
            };
            let target = self.schedule(&target_path, Some(import.span.clone()));
            let local_name = import.node.alias.clone().unwrap_or_else(|| {
                target_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            imports.push(ResolvedImport {
                local_name,
                target,
                span: import.span.clone(),
            });
        }

        self.modules.push(LoadedModule {
            id,
            module,
            imports,
        });
    }

    pub fn load(mut self, entry: &str) -> Result<LoadedProgram, Vec<LoadError>> {
        match resolve_path(Path::new(""), entry) {
            Some(path) => {
                self.schedule(&path, None);
            }
            None => self.errors.push(LoadError {
                msg: format!("entry `{entry}` climbs above the root"),
                source: SourceId::ANON,
                span: None,
            }),
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
        assert_eq!(main.imports[0].local_name, "util");
        assert_eq!(
            program.sources[main.imports[0].target.0 as usize]
                .path
                .to_str(),
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
        assert_eq!(main.imports[0].local_name, "u");
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
        assert_eq!(b.imports[0].target, c.imports[0].target);
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
    fn test_climbing_above_root_errors() {
        let p = provider(vec![(
            "main.kora",
            r#"import "../secret.kora"; int main(){return 0;}"#,
        )]);
        let errs = Loader::new(&p).load("main.kora").expect_err("should fail");
        assert!(errs.iter().any(|e| e.msg.contains("climbs above the root")));
    }

    #[test]
    fn test_missing_source_errors_against_importer() {
        let p = provider(vec![(
            "main.kora",
            r#"import "gone.kora"; int main(){return 0;}"#,
        )]);
        let errs = Loader::new(&p).load("main.kora").expect_err("should fail");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].msg.contains("gone.kora"));
        assert!(errs[0].span.is_some());
    }
}
