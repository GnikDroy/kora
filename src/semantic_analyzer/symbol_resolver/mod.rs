mod collect;
mod resolve;
mod scope;
mod table;

pub use resolve::Resolver;
pub use table::*;

use super::errors::TypeErr;
use crate::parser::Type;

/// Report any undefined struct type reached inside a typename. Shared by the
/// collection and resolution passes.
pub(super) fn check_typename(table: &SymbolTable, errors: &mut Vec<TypeErr>, ty: &Type) {
    match ty {
        Type::Struct(sr) => {
            if !table.struct_exists(&sr.name.node) {
                errors.push(TypeErr {
                    msg: "Undefined type",
                    span: sr.name.span.clone(),
                });
            }
        }
        Type::Generic(name, _) => {
            errors.push(TypeErr {
                msg: "generic type was not instantiated",
                span: name.span.clone(),
            });
        }
        Type::Array(inner) | Type::Optional(inner) => check_typename(table, errors, inner),
        Type::Function(return_type, args) => {
            if let Some(return_type) = return_type {
                check_typename(table, errors, return_type);
            }
            for arg in args.iter() {
                check_typename(table, errors, arg);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use std::collections::HashMap;
    use std::path::Path;

    use super::{Resolver, SymbolTable, TypeErr};
    use crate::loader::{LoadedModule, LoadedProgram, Loader};
    use crate::{lexer, parser};

    pub(super) fn resolve(source: &str) -> Result<SymbolTable, Vec<TypeErr>> {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        Resolver::new().resolve(&[&module])
    }

    pub(crate) fn resolve_program(program: &LoadedProgram) -> Result<SymbolTable, Vec<TypeErr>> {
        Resolver::new().resolve_program(program)
    }

    pub(crate) fn load_program(
        entry: &str,
        files: Vec<(&'static str, &'static str)>,
    ) -> LoadedProgram {
        let map: HashMap<String, String> = files
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let provider = move |p: &Path| p.to_str().and_then(|s| map.get(s)).cloned();
        Loader::new(&provider).load(entry).expect("load")
    }

    pub(super) fn source_module<'a>(program: &'a LoadedProgram, path: &str) -> &'a LoadedModule {
        program
            .modules
            .iter()
            .find(|m| program.sources[m.id.0 as usize].path.to_str() == Some(path))
            .expect("module present")
    }
}
