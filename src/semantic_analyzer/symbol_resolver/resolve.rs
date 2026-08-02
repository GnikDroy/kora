use std::collections::HashMap;

use super::super::errors::TypeErr;
use super::check_typename;
use super::collect::GlobalsCollector;
use super::scope::{ImportMap, Scopes};
use super::table::{SymbolId, SymbolTable, is_intrinsic};
use crate::instantiate::Instantiated;
use crate::loader::LoadedProgram;
use crate::parser::*;

#[derive(Default)]
pub struct Resolver {
    table: SymbolTable,
    scopes: Scopes,
    errors: Vec<TypeErr>,
    loop_depth: usize,
    resolutions: HashMap<NodeId, NodeId>,
}

impl Resolver {
    pub fn new() -> Resolver {
        Resolver::default()
    }

    pub fn resolve(self, modules: &[&Module]) -> Result<SymbolTable, Vec<TypeErr>> {
        let imports = modules.iter().map(|_| ImportMap::new()).collect();
        self.run(modules, imports, &Instantiated::default())
    }

    pub fn resolve_program(
        self,
        program: &LoadedProgram,
        instances: &Instantiated,
    ) -> Result<SymbolTable, Vec<TypeErr>> {
        let modules: Vec<&Module> = program.modules.iter().map(|m| &m.module).collect();
        let index: HashMap<SourceId, usize> = program
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();
        let imports = program
            .modules
            .iter()
            .map(|m| {
                m.imports
                    .iter()
                    .filter_map(|import| {
                        index
                            .get(&import.target)
                            .map(|&target| (import.local_name.clone(), target))
                    })
                    .collect()
            })
            .collect();
        self.run(&modules, imports, instances)
    }

    fn run(
        mut self,
        modules: &[&Module],
        imports: Vec<ImportMap>,
        instances: &Instantiated,
    ) -> Result<SymbolTable, Vec<TypeErr>> {
        self.resolutions = instances.resolutions.clone();
        let globals = GlobalsCollector::new(&mut self.table, &mut self.errors, instances)
            .collect(modules);
        self.scopes = Scopes::new(globals, imports);
        for (index, module) in modules.iter().enumerate() {
            self.scopes.enter_source(index);
            for func in module.functions.iter() {
                self.visit_function(func);
            }
            for impl_ in module.impls.iter() {
                for func in impl_.node.functions.iter() {
                    self.visit_function(func);
                }
            }
        }
        if self.errors.is_empty() {
            Ok(self.table)
        } else {
            Err(self.errors)
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push();
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(
        &mut self,
        declaration_id: NodeId,
        name: String,
        ty: Option<Type>,
        span: &Span,
    ) -> SymbolId {
        if is_intrinsic(&name) {
            self.errors.push(TypeErr {
                msg: "Cannot declare a name reserved for a compiler intrinsic",
                span: span.clone(),
            });
        }
        let id = self.table.add_symbol(declaration_id, name.clone(), ty);
        if self.scopes.declare(name, id) {
            self.errors.push(TypeErr {
                msg: "Redeclaration in the same scope",
                span: span.clone(),
            });
        }
        id
    }
}

impl ASTVisitor for Resolver {
    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.push_scope();
        if let Some(return_type) = &func.node.return_type {
            self.visit_typename(return_type);
        }
        for pair in func.node.arguments.iter() {
            self.visit_typename(&pair.node.typename);
            self.declare(
                pair.id,
                pair.node.name.clone(),
                Some(pair.node.typename.clone()),
                &pair.span,
            );
        }
        self.visit_statement(&func.node.statement);
        self.pop_scope();
    }

    fn visit_typename(&mut self, ty: &Type) {
        check_typename(&self.table, &mut self.errors, ty);
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
        name: &Spanned<String>,
        typename: Option<&Type>,
        expr: &Spanned<Expression>,
    ) {
        // The initializer is resolved before the name is bound, so `let x = x;`
        // refers to an outer `x`, not itself.
        self.visit_expression(expr);
        if let Some(typename) = typename {
            self.visit_typename(typename);
        }
        self.declare(name.id, name.node.clone(), typename.cloned(), &name.span);
    }

    fn visit_while_statement(&mut self, cond: &Spanned<Expression>, stmt: &Spanned<Statement>) {
        self.visit_expression(cond);
        self.loop_depth += 1;
        self.push_scope();
        self.visit_statement(stmt);
        self.pop_scope();
        self.loop_depth -= 1;
    }

    fn visit_if_statement(
        &mut self,
        cond: &Spanned<Expression>,
        if_case: &Spanned<Statement>,
        else_case: Option<&Spanned<Statement>>,
    ) {
        self.visit_expression(cond);
        self.push_scope();
        self.visit_statement(if_case);
        self.pop_scope();
        if let Some(else_case) = else_case {
            self.push_scope();
            self.visit_statement(else_case);
            self.pop_scope();
        }
    }

    fn visit_for_statement(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) {
        self.push_scope();
        self.visit_statement(init);
        self.visit_expression(cond);
        self.visit_expression(step);
        self.loop_depth += 1;
        self.visit_statement(body);
        self.loop_depth -= 1;
        self.pop_scope();
    }

    fn visit_break_statement(&mut self, span: &Span) {
        if self.loop_depth == 0 {
            self.errors.push(TypeErr {
                msg: "break outside of a loop",
                span: span.clone(),
            });
        }
    }

    fn visit_continue_statement(&mut self, span: &Span) {
        if self.loop_depth == 0 {
            self.errors.push(TypeErr {
                msg: "continue outside of a loop",
                span: span.clone(),
            });
        }
    }

    fn visit_expression(&mut self, expr: &Spanned<Expression>) {
        if let Some(&decl) = self.resolutions.get(&expr.id) {
            let symbol = self.table.declarations[&decl];
            self.table.uses.insert(expr.id, symbol);
            return;
        }
        match &expr.node {
            Expression::Identifier(name) if !is_intrinsic(name) => match self.scopes.lookup(name) {
                Some(id) => {
                    self.table.uses.insert(expr.id, id);
                }
                None => self.errors.push(TypeErr {
                    msg: "Undefined identifier",
                    span: expr.span.clone(),
                }),
            },
            Expression::Access(left, member) => {
                // x.y where x is an import
                if let Expression::Identifier(m) = &left.node
                    && self.scopes.lookup(m).is_none()
                    && self.scopes.is_module(m)
                {
                    match self.scopes.lookup_qualified(m, member) {
                        Some(id) => {
                            self.table.uses.insert(expr.id, id);
                        }
                        None => self.errors.push(TypeErr {
                            msg: "unknown member of imported source",
                            span: expr.span.clone(),
                        }),
                    }
                } else {
                    walk_expression(self, expr);
                }
            }
            _ => walk_expression(self, expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::Resolver;
    use crate::parser::{Expression, Statement, Type};
    use crate::{lexer, parser};

    #[test]
    fn test_resolves_a_valid_program() {
        let source = r#"
            extern void print(b: cstring, a: int64);
            int main() {
                let a: int = 5;
                if (a - a) { print("Hello World", a); }
                return a;
            }
            int sum(a: int, b: int) { return a + b; }
        "#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_reports_undefined_identifiers() {
        let source = r#"
            int main() {
                let a: int = unknown1;
                a + unknown2;
                unknown3[a];
                -unknown4;
                unknown_fn(a, unknown_arg);
                return a;
            }
        "#;
        assert_eq!(resolve(source).expect_err("undefined").len(), 6);
    }

    #[test]
    fn test_reports_undefined_types() {
        let source = r#"
            struct Point { x: int, y: int }
            Bogus1 make(p: Bogus2) {
                let a: Point = new Point;
                let b: Bogus3 = new Bogus4;
                let c: [Bogus5] = new Point;
                return a;
            }
        "#;
        assert_eq!(resolve(source).expect_err("undefined types").len(), 5);
    }

    #[test]
    fn test_rejects_duplicate_locals() {
        for source in [
            r#"int main() { let x: int = 1; let x: int = 2; return x; }"#,
            r#"int f(a: int, a: int) { return a; } int main() { return 0; }"#,
        ] {
            assert_eq!(resolve(source).expect_err(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn test_rejects_intrinsic_local() {
        for source in [
            r#"int main() { let copy: int = 1; return copy; }"#,
            r#"int main(copy: int) { return copy; }"#,
        ] {
            let errors = resolve(source).expect_err(source);
            assert!(
                errors.iter().any(|e| e.msg.contains("intrinsic")),
                "{source}"
            );
        }
    }

    #[test]
    fn test_scopes_confine_declarations() {
        let cases = [
            r#"int main() { if (true) { let a: int = 1; } return a; }"#,
            r#"int main() { { let a: int = 1; } { let b: int = a; } return 0; }"#,
            r#"int main() { for (let i: int = 0; i < 3; i = i + 1) { i; } return i; }"#,
            r#"int main() { while (true) let x: int = 1; return x; }"#,
            r#"int main() { if (true) let x: int = 1; return x; }"#,
        ];
        for source in cases {
            assert_eq!(resolve(source).expect_err(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn test_cross_scope_shadowing_is_allowed() {
        let source =
            r#"int main() { let x: int = 1; if (x == 1) { let x: int = 2; return x; } return x; }"#;
        assert!(resolve(source).is_ok());
    }

    #[test]
    fn test_let_initializer_uses_outer_scope() {
        let ok = r#"int main() { let x: int = 1; if (x) { let x: int = x; return x; } return x; }"#;
        assert!(resolve(ok).is_ok());
        let bad = r#"int main() { let x: int = x; return 0; }"#;
        assert_eq!(resolve(bad).expect_err("self ref").len(), 1);
    }

    #[test]
    fn test_break_and_continue_require_a_loop() {
        let ok = r#"
            int main() {
                while (true) { break; }
                for (let i: int = 0; i < 3; i = i + 1) { continue; }
                while (true) { for (let j: int = 0; j < 2; j = j + 1) { break; continue; } }
                return 0;
            }
        "#;
        assert!(resolve(ok).is_ok());
        assert_eq!(
            resolve(r#"int main() { break; continue; return 0; }"#)
                .expect_err("outside")
                .len(),
            2
        );
        assert_eq!(
            resolve(r#"int main() { while (true) {} break; return 0; }"#)
                .expect_err("after loop")
                .len(),
            1
        );
    }

    #[test]
    fn test_functions_resolve_forward_and_recursively() {
        assert!(resolve(r#"int main() { return helper(); } int helper() { return 1; }"#).is_ok());
        assert!(
            resolve(r#"int fact(n: int) { return fact(n); } int main() { return 0; }"#).is_ok()
        );
    }

    #[test]
    fn test_per_source_scoping_hides_unimported_names() {
        let mut program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return helper(); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        assert!(resolve_program(&mut program).is_err());
    }

    #[test]
    fn test_qualified_access_resolves_imported_member() {
        let mut program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.helper(); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        let symbols = resolve_program(&mut program).expect("resolve");
        let helper = source_module(&program, "util.kora").module.functions[0].id;
        let helper_id = symbols.symbol_id_of_declaration(helper).unwrap();
        assert!(symbols.uses.values().any(|&id| id == helper_id));
    }

    #[test]
    fn test_qualified_access_requires_import() {
        let mut program = load_program(
            "main.kora",
            vec![("main.kora", r#"int main() { return util.helper(); }"#)],
        );
        assert!(resolve_program(&mut program).is_err());
    }

    #[test]
    fn test_unknown_imported_member_errors() {
        let mut program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.missing(); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        assert!(resolve_program(&mut program).is_err());
    }

    #[test]
    fn test_use_is_keyed_by_node_id() {
        let source = "int f(a: int) { return a; }";
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let body = &module.functions[0].node.statement;
        let Statement::Compound(stmts) = &body.node else {
            panic!("compound body");
        };
        let Statement::Return(Some(expr)) = &stmts[0].node else {
            panic!("return");
        };
        assert!(matches!(expr.node, Expression::Identifier(_)));
        assert_eq!(symbols.type_of_use(expr.id), Some(Type::Int));
    }

    #[test]
    fn test_declaration_is_keyed_by_node_id() {
        let source = r#"
            int main(a: int) {
                let x: int = 1;
                if (true) {
                    let x: real = 2.0;
                    x;
                }
                x;
                return a;
            }
        "#;
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

        let func = &module.functions[0];
        let Statement::Compound(stmts) = &func.node.statement.node else {
            panic!("compound body");
        };
        let Statement::Let(outer_pair, _, _) = &stmts[0].node else {
            panic!("let");
        };
        let Statement::If(_, if_body, _) = &stmts[1].node else {
            panic!("if");
        };
        let Statement::Compound(if_stmts) = &if_body.node else {
            panic!("if body");
        };
        let Statement::Let(inner_pair, _, _) = &if_stmts[0].node else {
            panic!("inner let");
        };
        let Statement::Simple(inner_use) = &if_stmts[1].node else {
            panic!("inner use");
        };
        let Statement::Simple(outer_use) = &stmts[2].node else {
            panic!("outer use");
        };

        let outer = symbols.symbol_id_of_declaration(outer_pair.id).unwrap();
        let inner = symbols.symbol_id_of_declaration(inner_pair.id).unwrap();
        assert_ne!(outer, inner);
        assert_eq!(symbols.symbol_id_of_use(outer_use.id), Some(outer));
        assert_eq!(symbols.symbol_id_of_use(inner_use.id), Some(inner));
        assert_eq!(symbols.symbol(inner).ty, Some(Type::Real));

        let param = symbols
            .symbol_id_of_declaration(func.node.arguments[0].id)
            .unwrap();
        assert_eq!(symbols.symbol(param).ty, Some(Type::Int));
        assert!(symbols.symbol_id_of_declaration(func.id).is_some());
    }
}
