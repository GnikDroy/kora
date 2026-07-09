mod collect;
pub(crate) mod mangle;
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
