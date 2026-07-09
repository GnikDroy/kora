mod collect;
pub(crate) mod mangle;
mod resolve;
mod table;

pub use resolve::Resolver;
pub use table::*;

use std::collections::HashMap;

use super::errors::TypeErr;
use crate::parser::Type;

/// A lexical scope: names visible at one point, each bound to its symbol.
pub(super) type Scope = HashMap<String, SymbolId>;

/// Report any undefined struct type reached inside a typename. Shared by the
/// collection and resolution passes.
pub(super) fn check_typename(table: &SymbolTable, errors: &mut Vec<TypeErr>, ty: &Type) {
    match ty {
        Type::Struct(name) => {
            if !table.struct_exists(&name.node) {
                errors.push(TypeErr {
                    msg: "Undefined type",
                    span: name.span.clone(),
                });
            }
        }
        Type::Array(inner) => check_typename(table, errors, inner),
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
