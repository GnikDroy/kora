use std::collections::HashMap;

use super::errors::TypeErr;
use crate::parser::*;

/// Reserved intrinsic names, handled by the type checker and undeclarable by user code.
pub const INTRINSICS: &[&str] = &["len"];

pub fn is_intrinsic(name: &str) -> bool {
    INTRINSICS.contains(&name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolId(usize);

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Default)]
pub struct StructDef {
    pub members: Vec<(String, Type)>, // Order matters, so members are a `Vec`, not a map.
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
    uses: HashMap<NodeId, SymbolId>,
    structs: HashMap<String, StructDef>,
}

impl SymbolTable {
    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0]
    }

    /// The declaration a given identifier use resolves to, if it was resolved.
    pub fn symbol_of_use(&self, use_id: NodeId) -> Option<&Symbol> {
        self.uses.get(&use_id).map(|id| self.symbol(*id))
    }

    /// The type of a resolved identifier use.
    pub fn type_of_use(&self, use_id: NodeId) -> Option<Type> {
        self.symbol_of_use(use_id).map(|s| s.ty.clone())
    }

    pub fn struct_exists(&self, name: &str) -> bool {
        self.structs.contains_key(name)
    }

    pub fn resolve_struct_member(&self, name: &str, member: &str) -> Option<Type> {
        self.structs
            .get(name)?
            .members
            .iter()
            .find(|(field, _)| field == member)
            .map(|(_, ty)| ty.clone())
    }
}

/// Walks the AST with a live scope stack, builds a `SymbolTable`, and records
/// each identifier use by its `NodeId`. Reports undefined identifiers.
#[derive(Default)]
pub struct Resolver {
    table: SymbolTable,
    scopes: Vec<HashMap<String, SymbolId>>,
    errors: Vec<TypeErr>,
}

impl Resolver {
    pub fn new() -> Resolver {
        Resolver::default()
    }

    /// Resolve a whole module. Returns the populated table, or the collected
    /// diagnostics if any name failed to resolve.
    pub fn resolve(mut self, module: &Module) -> Result<SymbolTable, Vec<TypeErr>> {
        self.visit_module(module);
        if self.errors.is_empty() {
            Ok(self.table)
        } else {
            Err(self.errors)
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: String, ty: Type, span: &Span) -> SymbolId {
        if is_intrinsic(&name) {
            self.errors.push(TypeErr {
                msg: "Cannot declare a name reserved for a compiler intrinsic",
                span: span.clone(),
            });
        }

        // Same-scope redeclaration is an error; shadowing an outer scope is fine.
        let duplicate = self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(&name));
        if duplicate {
            self.errors.push(TypeErr {
                msg: "Redeclaration in the same scope",
                span: span.clone(),
            });
        }

        let id = SymbolId(self.table.symbols.len());
        self.table.symbols.push(Symbol {
            name: name.clone(),
            ty,
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, id);
        }
        id
    }

    fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

impl ASTVisitor for Resolver {
    fn visit_module(&mut self, module: &Module) {
        self.push_scope(); // global scope

        // Register every struct name first (empty) so member types and
        // forward/self references resolve regardless of declaration order.
        for struct_ in module.structs.iter() {
            if self.table.struct_exists(&struct_.node.name) {
                self.errors.push(TypeErr {
                    msg: "Redeclaration of struct",
                    span: struct_.span.clone(),
                });
            } else {
                self.table
                    .structs
                    .insert(struct_.node.name.clone(), StructDef::default());
            }
        }

        // Fill in members now that all struct names are known.
        for struct_ in module.structs.iter() {
            for member in struct_.node.members.iter() {
                self.visit_typename(&member.node.typename);

                let already = self.table.structs[&struct_.node.name]
                    .members
                    .iter()
                    .any(|(field, _)| field == &member.node.name);
                if already {
                    self.errors.push(TypeErr {
                        msg: "Redeclaration of struct member",
                        span: member.span.clone(),
                    });
                } else {
                    self.table
                        .structs
                        .get_mut(&struct_.node.name)
                        .unwrap()
                        .members
                        .push((member.node.name.clone(), member.node.typename.clone()));
                }
            }
        }

        // Top-level functions are visible everywhere (forward refs), so declare
        // them all before walking any body.
        for func in module.extern_functions.iter() {
            if let Some(return_type) = &func.node.return_type {
                self.visit_typename(return_type);
            }
            for arg in func.node.arguments.iter() {
                self.visit_typename(&arg.node.typename);
            }
            self.declare(func.node.name.clone(), func.node.get_type(), &func.span);
        }
        for func in module.functions.iter() {
            self.declare(func.node.name.clone(), func.node.get_type(), &func.span);
        }

        for func in module.functions.iter() {
            self.visit_function(func);
        }

        self.pop_scope();
    }

    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.push_scope();
        if let Some(return_type) = &func.node.return_type {
            self.visit_typename(return_type);
        }
        for pair in func.node.arguments.iter() {
            self.visit_typename(&pair.node.typename);
            self.declare(
                pair.node.name.clone(),
                pair.node.typename.clone(),
                &pair.span,
            );
        }
        self.visit_statement(&func.node.statement);
        self.pop_scope();
    }

    fn visit_typename(&mut self, ty: &Type) {
        match ty {
            Type::Struct(name) => {
                if !self.table.struct_exists(&name.node) {
                    self.errors.push(TypeErr {
                        msg: "Undefined type",
                        span: name.span.clone(),
                    });
                }
            }
            Type::Array(inner) => self.visit_typename(inner),
            Type::Function(ret, args) => {
                if let Some(ret) = ret {
                    self.visit_typename(ret);
                }
                for arg in args.iter() {
                    self.visit_typename(arg);
                }
            }
            _ => {}
        }
    }

    fn visit_compound_statement(&mut self, stmts: &[Spanned<Statement>]) {
        self.push_scope();
        for stmt in stmts.iter() {
            self.visit_statement(stmt);
        }
        self.pop_scope();
    }

    fn visit_let_statement(
        &mut self,
        pair: &Spanned<IdentifierTypePair>,
        expr: &Spanned<Expression>,
    ) {
        // The initializer is resolved before the name is bound, so `let x = x;`
        // refers to an outer `x`, not itself.
        self.visit_expression(expr);
        self.visit_typename(&pair.node.typename);
        self.declare(
            pair.node.name.clone(),
            pair.node.typename.clone(),
            &pair.span,
        );
    }

    fn visit_call_expression(&mut self, expr: &Spanned<Expression>, args: &[Spanned<Expression>]) {
        // An intrinsic callee is not a resolvable symbol; skip it, but still
        // resolve the arguments.
        if matches!(&expr.node, Expression::Identifier(name) if is_intrinsic(name)) {
            for arg in args.iter() {
                self.visit_expression(arg);
            }
            return;
        }
        walk_call_expression(self, expr, args);
    }

    fn visit_expression(&mut self, expr: &Spanned<Expression>) {
        if let Expression::Identifier(name) = &expr.node {
            match self.lookup(name) {
                Some(id) => {
                    self.table.uses.insert(expr.id, id);
                }
                None => self.errors.push(TypeErr {
                    msg: "Undefined identifier",
                    span: expr.span.clone(),
                }),
            }
        }
        walk_expression(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::{lexer, parser};

    use super::Resolver;

    fn resolve(source: &str) -> Result<super::SymbolTable, Vec<super::TypeErr>> {
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        Resolver::new().resolve(&module)
    }

    #[test]
    fn resolves_all_identifiers() {
        let source = r#"
            extern nil print(b: [char], a: int);

            int main() {
                let a: int = 5;
                if (a - a) {
                    print("Hello World", a);
                }
                ret a;
            }

            int sum(a: int, b: int) {
                ret a + b;
            }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn reports_undefined_identifiers() {
        let source = r#"
            int main() {
                let a: int = unident_1;
                unident_2;
                if (a) { unident_3; }
                ret a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-identifier errors");
        assert_eq!(errors.len(), 3, "errors: {:?}", errors);
    }

    #[test]
    fn reports_undefined_types() {
        let source = r#"
            struct Point { x: int, y: int }

            Bogus1 make(p: Bogus2) {
                let a: Point = new Point;
                let b: Bogus3 = new Bogus4;
                let c: [Bogus5] = new Point;
                ret a;
            }
        "#;
        let errors = resolve(source).expect_err("expected undefined-type errors");
        assert_eq!(errors.len(), 5, "errors: {:?}", errors);
    }

    #[test]
    fn reports_same_scope_redeclarations() {
        let cases = [
            r#"int main() { let x: int = 1; let x: int = 2; ret x; }"#,
            r#"int f(a: int, a: int) { ret a; } int main() { ret 0; }"#,
            r#"int f() { ret 0; } int f() { ret 1; } int main() { ret 0; }"#,
            r#"extern int g(a: int); int g(a: int) { ret a; } int main() { ret 0; }"#,
            r#"struct P { x: int } struct P { y: int } int main() { ret 0; }"#,
            r#"struct P { x: int, x: int } int main() { ret 0; }"#,
        ];
        for source in cases {
            let errors = resolve(source).expect_err(source);
            assert_eq!(errors.len(), 1, "source: {}, errors: {:?}", source, errors);
        }
    }

    #[test]
    fn intrinsic_names_cannot_be_declared() {
        let cases = [
            r#"int len(a: int) { ret a; } int main() { ret 0; }"#,
            r#"extern int len(a: int); int main() { ret 0; }"#,
            r#"int main() { let len: int = 1; ret len; }"#,
            r#"int main(len: int) { ret len; }"#,
        ];
        for source in cases {
            let errors = resolve(source).expect_err(source);
            assert!(
                errors.iter().any(|e| e.msg.contains("intrinsic")),
                "source: {}, errors: {:?}",
                source,
                errors
            );
        }
    }

    #[test]
    fn intrinsic_call_does_not_flag_undefined() {
        let source = r#"int main() { let a: [int] = new int[3]; ret len(a); }"#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn cross_scope_shadowing_is_allowed() {
        let source = r#"
            int main() {
                let x: int = 1;
                if (x == 1) {
                    let x: int = 2;
                    ret x;
                }
                ret x;
            }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn self_and_forward_referential_structs_resolve() {
        let source = r#"
            struct Node { next: Node, value: int }
            struct A { b: B }
            struct B { n: int }

            int main() { ret 0; }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn use_is_keyed_by_node_id() {
        use crate::parser::{Expression, Statement, Type};

        let source = "int f(a: int) { ret a; }";
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&module).expect("resolve");

        // Reach the `a` in `ret a;` and confirm its NodeId resolves to `int`.
        let body = &module.functions[0].node.statement;
        let Statement::Compound(stmts) = &body.node else {
            panic!("expected compound body");
        };
        let Statement::Return(expr) = &stmts[0].node else {
            panic!("expected return statement");
        };
        assert!(matches!(expr.node, Expression::Identifier(_)));
        assert_eq!(symbols.type_of_use(expr.id), Some(Type::Int));
    }
}
