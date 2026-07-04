use std::collections::{HashMap, HashSet};

use super::errors::TypeErr;
use crate::parser::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolId(usize);

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
    uses: HashMap<NodeId, SymbolId>,
    struct_members: HashMap<(String, String), Type>,
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

    pub fn resolve_struct_member(&self, name: &str, member: &str) -> Option<Type> {
        self.struct_members
            .get(&(name.to_owned(), member.to_owned()))
            .cloned()
    }
}

/// Walks the AST with a live scope stack, builds a `SymbolTable`, and records
/// each identifier use by its `NodeId`. Reports undefined identifiers as it goes
/// (this subsumes the old `UnidentifiedIdentifierChecker`).
#[derive(Default)]
pub struct Resolver {
    table: SymbolTable,
    scopes: Vec<HashMap<String, SymbolId>>,
    struct_names: HashSet<String>,
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

    fn declare(&mut self, name: String, ty: Type) -> SymbolId {
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

        // Collect every struct name first so member types and forward/self
        // references resolve regardless of declaration order.
        for struct_ in module.structs.iter() {
            self.struct_names.insert(struct_.node.name.clone());
        }

        // Structs and top-level functions are visible everywhere (forward refs),
        // so register them all before walking any body.
        for struct_ in module.structs.iter() {
            for member in struct_.node.members.iter() {
                self.visit_typename(&member.node.typename);
                self.table.struct_members.insert(
                    (struct_.node.name.clone(), member.node.name.clone()),
                    member.node.typename.clone(),
                );
            }
        }
        for func in module.extern_functions.iter() {
            self.visit_typename(&func.node.return_type);
            for arg in func.node.arguments.iter() {
                self.visit_typename(&arg.node.typename);
            }
            self.declare(func.node.name.clone(), func.node.get_type());
        }
        for func in module.functions.iter() {
            self.declare(func.node.name.clone(), func.node.get_type());
        }

        for func in module.functions.iter() {
            self.visit_function(func);
        }

        self.pop_scope();
    }

    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.push_scope();
        self.visit_typename(&func.node.return_type);
        for pair in func.node.arguments.iter() {
            self.visit_typename(&pair.node.typename);
            self.declare(pair.node.name.clone(), pair.node.typename.clone());
        }
        self.visit_statement(&func.node.statement);
        self.pop_scope();
    }

    fn visit_typename(&mut self, ty: &Type) {
        match ty {
            Type::Struct(name) => {
                if !self.struct_names.contains(&name.node) {
                    self.errors.push(TypeErr {
                        msg: "Undefined type",
                        span: name.span.clone(),
                    });
                }
            }
            Type::Array(inner) => self.visit_typename(inner),
            Type::Function(ret, args) => {
                self.visit_typename(ret);
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
        self.declare(pair.node.name.clone(), pair.node.typename.clone());
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
