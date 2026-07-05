use super::{errors::TypeErr, symbol_table::*};
use crate::parser::*;

pub struct TypeChecker<'a> {
    symbols: &'a SymbolTable,
    current_return_type: Option<Type>,
    errors: Vec<TypeErr>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> TypeChecker<'a> {
        TypeChecker {
            symbols,
            current_return_type: None,
            errors: Vec::new(),
        }
    }

    pub fn check(&self) -> Result<(), &[TypeErr]> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(&self.errors)
        }
    }

    fn get_array_type(&self, exprs: &[Spanned<Expression>], span: &Span) -> Result<Type, TypeErr> {
        let types = exprs
            .iter()
            .map(|e| self.get_expression_type(e))
            .collect::<Result<Vec<Type>, TypeErr>>()?;

        if let Some(first) = types.first() {
            if types.iter().all(|x| x == first) {
                Ok(Type::Array(Box::new(first.clone())))
            } else {
                Err(TypeErr {
                    msg: "Array doesn't consist of homogeneous types.",
                    span: span.clone(),
                })
            }
        } else {
            Err(TypeErr {
                msg: "Cannot infer type of empty array. An empty static array makes no sense either way.",
                span: span.clone(),
            })
        }
    }

    fn get_binary_expression_type(
        &self,
        left: &Spanned<Expression>,
        op: &BinaryOp,
        right: &Spanned<Expression>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        use BinaryOp::*;
        use Type::*;

        let left_type = self.get_expression_type(left)?;
        let right_type = self.get_expression_type(right)?;

        #[rustfmt::skip]
        match (left_type, op, right_type) {
            (Int, Add, Int)            => Ok(Int),
            (Int, Subtract, Int)       => Ok(Int),
            (Int, Multiply, Int)       => Ok(Int),
            (Int, Divide, Int)         => Ok(Int),
            (Int, Modulo, Int)         => Ok(Int),
            (Int, Equality, Int)       => Ok(Bool),
            (Int, NotEquality, Int)    => Ok(Bool),
            (Int, Greater, Int)        => Ok(Bool),
            (Int, Less, Int)           => Ok(Bool),
            (Int, GreaterEqual, Int)   => Ok(Bool),
            (Int, LessEqual, Int)      => Ok(Bool),

            (Real, Add, Real)          => Ok(Real),
            (Real, Subtract, Real)     => Ok(Real),
            (Real, Multiply, Real)     => Ok(Real),
            (Real, Divide, Real)       => Ok(Real),
            (Real, Equality, Real)     => Ok(Bool),
            (Real, NotEquality, Real)  => Ok(Bool),
            (Real, Greater, Real)      => Ok(Bool),
            (Real, Less, Real)         => Ok(Bool),
            (Real, GreaterEqual, Real) => Ok(Bool),
            (Real, LessEqual, Real)    => Ok(Bool),

            (Bool, Equality, Bool)     => Ok(Bool),
            (Bool, NotEquality, Bool)  => Ok(Bool),
            (Bool, And, Bool)          => Ok(Bool),
            (Bool, Or, Bool)           => Ok(Bool),

            (Char, Equality, Char)     => Ok(Bool),
            (Char, NotEquality, Char)  => Ok(Bool),
            (Char, Greater, Char)      => Ok(Bool),
            (Char, Less, Char)         => Ok(Bool),
            (Char, GreaterEqual, Char) => Ok(Bool),
            (Char, LessEqual, Char)    => Ok(Bool),
            (left_type, Assign, right_type) => {
                if !self.is_assignable(left) || matches!(left_type, Function(_, _)) {
                    Err(TypeErr{
                        msg: "LHS of assign expression is not assignable",
                        span: span.clone(),
                    })
                } else if left_type != right_type {
                    Err(TypeErr{
                        msg: "LHS and RHS of assign expression don't match",
                        span: span.clone(),
                    })
                } else {
                    Ok(left_type)
                }
            },
            _ => {
                Err(TypeErr {
                msg: "Binary operator cannot be applied to the types",
                span: span.clone(),
            })
            },
        }
    }

    fn get_unary_expression_type(
        &self,
        op: &UnaryOp,
        expr: &Spanned<Expression>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        use Type::*;
        use UnaryOp::*;

        let typename = self.get_expression_type(expr)?;

        #[rustfmt::skip]
        match (op, typename) {
            (Negate, Int)  => Ok(Int),
            (Negate, Real) => Ok(Real),

            (Not, Bool)    => Ok(Bool),
            _ => Err(TypeErr {
                msg: "Unary operator cannot be applied to the types",
                span: span.clone(),
            })
        }
    }

    fn get_len_intrinsic_type(
        &self,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let [arg] = args else {
            return Err(TypeErr {
                msg: "len expects exactly one argument",
                span: span.clone(),
            });
        };
        match self.get_expression_type(arg)? {
            Type::Array(_) => Ok(Type::Int),
            _ => Err(TypeErr {
                msg: "len expects an array or string argument",
                span: span.clone(),
            }),
        }
    }

    fn get_call_type(
        &self,
        f: &Spanned<Expression>,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<Type>, TypeErr> {
        if matches!(&f.node, Expression::Identifier(name) if name == "len") {
            return self.get_len_intrinsic_type(args, span).map(Some);
        }

        match self.get_expression_type(f)? {
            Type::Function(ret_type, args_types) => {
                if args.len() != args_types.len() {
                    return Err(TypeErr {
                        msg: "Function has different number of arguments",
                        span: span.clone(),
                    });
                }

                for (arg, arg_type) in args.iter().zip(args_types) {
                    if self.get_expression_type(arg)? != arg_type {
                        return Err(TypeErr {
                            msg: "Arguments passed to function do not match type signature for function",
                            span: arg.span.clone(),
                        });
                    }
                }
                Ok(ret_type.map(|return_type| *return_type))
            }
            _ => Err(TypeErr {
                msg: "Call expression must have function type",
                span: span.clone(),
            }),
        }
    }

    fn get_array_index_expression_type(
        &self,
        left: &Spanned<Expression>,
        right: &Spanned<Expression>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let left_type = self.get_expression_type(left)?;
        let right_type = self.get_expression_type(right)?;
        match (left_type, right_type) {
            (Type::Array(item_type), Type::Int) => Ok(*item_type),
            _ => Err(TypeErr {
                msg: "Array index expression must have array type on the left, and integer on the right",
                span: span.clone(),
            }),
        }
    }

    fn get_access_expression_type(
        &self,
        left: &Spanned<Expression>,
        member: &str,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let left_type = self.get_expression_type(left)?;
        if let Type::Struct(name) = left_type {
            self.symbols
                .resolve_struct_member(&name.node, member)
                .ok_or(TypeErr {
                    msg: "Invalid member for struct",
                    span: span.clone(),
                })
        } else {
            Err(TypeErr {
                msg: "Access operator must have struct type on the left",
                span: span.clone(),
            })
        }
    }

    fn is_cast_possible(from: &Type, to: &Type) -> bool {
        use Type::*;
        #[rustfmt::skip]
        matches!((from, to),
            (Int, Real)
            | (Int, Char)
            | (Real, Int)
            | (Real, Char)
            | (Char, Int)
            | (Char, Real))
    }

    fn get_cast_expression_type(
        &self,
        expr: &Spanned<Expression>,
        typename: &Type,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let expr_type = self.get_expression_type(expr)?;
        if TypeChecker::is_cast_possible(&expr_type, typename) {
            Ok(typename.clone())
        } else {
            Err(TypeErr {
                msg: "Cannot cast to type",
                span: span.clone(),
            })
        }
    }

    fn get_construct_expression_type(
        &self,
        typename: &Type,
        size: &Option<Box<Spanned<Expression>>>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        match size {
            Some(size) => {
                let expr_type = self.get_expression_type(size)?;
                if expr_type == Type::Int {
                    Ok(Type::Array(Box::new(typename.clone())))
                } else {
                    Err(TypeErr {
                        msg: "Array constructor must be passed an integer inside []",
                        span: span.clone(),
                    })
                }
            }
            _ => match typename {
                Type::Struct(_) => Ok(typename.clone()),
                _ => Err(TypeErr {
                    msg: "Only structs can be constructed without a size: new <struct>",
                    span: span.clone(),
                }),
            },
        }
    }

    fn is_assignable(&self, expr: &Spanned<Expression>) -> bool {
        use Expression::*;
        match &expr.node {
            IntegerLiteral(_) => false,
            RealLiteral(_) => false,
            CharLiteral(_) => false,
            StringLiteral(_) => false,
            BoolLiteral(_) => false,
            Array(_) => false,
            Identifier(_) => true,
            Binary(_, _, _) => false,
            Unary(_, _) => false,
            Call(_, _) => false,
            Cast(_, _) => false,
            ArrayIndex(_, _) => true,
            Access(_, _) => true,
            Construct(_, _) => false,
        }
    }

    fn get_expression_type(&self, expr: &Spanned<Expression>) -> Result<Type, TypeErr> {
        use Expression::*;
        let span = &expr.span;
        match &expr.node {
            IntegerLiteral(_) => Ok(Type::Int),
            RealLiteral(_) => Ok(Type::Real),
            CharLiteral(_) => Ok(Type::Char),
            StringLiteral(_) => Ok(Type::Array(Box::new(Type::Char))),
            BoolLiteral(_) => Ok(Type::Bool),
            Array(exprs) => self.get_array_type(exprs, span),
            Identifier(_) => self.symbols.type_of_use(expr.id).ok_or(TypeErr {
                msg: "Undefined identifier",
                span: span.clone(),
            }),
            Binary(left, op, right) => self.get_binary_expression_type(left, op, right, span),
            Unary(op, term) => self.get_unary_expression_type(op, term, span),
            Call(f, args) => self.get_call_type(f, args, span)?.ok_or_else(|| TypeErr {
                msg: "A void function returns no value and cannot be used as a value",
                span: span.clone(),
            }),
            Cast(expr, typename) => self.get_cast_expression_type(expr, typename, span),
            ArrayIndex(left, right) => self.get_array_index_expression_type(left, right, span),
            Access(left, member) => self.get_access_expression_type(left, member, span),
            Construct(typename, size) => self.get_construct_expression_type(typename, size, span),
        }
    }

    fn check_statement_expression(&mut self, expr: &Spanned<Expression>) {
        let result = match &expr.node {
            Expression::Call(f, args) => self.get_call_type(f, args, &expr.span).map(|_| ()),
            _ => self.get_expression_type(expr).map(|_| ()),
        };
        if let Err(e) = result {
            self.errors.push(e);
        }
    }

    fn ensure_type(&mut self, expr: &Spanned<Expression>, expected: &Type) {
        let expr_type = self.get_expression_type(expr);
        match expr_type {
            Ok(typename) if typename == *expected => {}
            Err(type_err) => self.errors.push(type_err),
            _ => self.errors.push(TypeErr {
                msg: "Types don't match",
                span: expr.span.clone(),
            }),
        };
    }
}

impl ASTVisitor for TypeChecker<'_> {
    fn visit_function(&mut self, func: &Spanned<Function>) {
        self.current_return_type = func.node.return_type.clone();
        walk_function(self, func);
    }

    fn visit_let_statement(
        &mut self,
        pair: &Spanned<IdentifierTypePair>,
        expr: &Spanned<Expression>,
    ) {
        self.ensure_type(expr, &pair.node.typename);
        walk_let_statement(self, pair, expr);
    }

    fn visit_if_statement(
        &mut self,
        cond: &Spanned<Expression>,
        if_case: &Spanned<Statement>,
        else_case: Option<&Spanned<Statement>>,
    ) {
        self.ensure_type(cond, &Type::Bool);
        walk_if_statement(self, cond, if_case, else_case);
    }

    fn visit_while_statement(&mut self, cond: &Spanned<Expression>, stmt: &Spanned<Statement>) {
        self.ensure_type(cond, &Type::Bool);
        walk_while_statement(self, cond, stmt);
    }

    fn visit_simple_statement(&mut self, expr: &Spanned<Expression>) {
        self.check_statement_expression(expr);
        walk_simple_statement(self, expr);
    }

    fn visit_for_statement(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) {
        self.ensure_type(cond, &Type::Bool);
        self.check_statement_expression(step);
        walk_for_statement(self, init, cond, step, body);
    }

    fn visit_return_statement(&mut self, expr: Option<&Spanned<Expression>>, span: &Span) {
        match (self.current_return_type.clone(), expr) {
            (Some(ret_type), Some(expr)) => self.ensure_type(expr, &ret_type),
            (Some(_), None) => self.errors.push(TypeErr {
                msg: "A function with a return type must return a value",
                span: span.clone(),
            }),
            (None, Some(expr)) => self.errors.push(TypeErr {
                msg: "A void function cannot return a value",
                span: expr.span.clone(),
            }),
            (None, None) => {}
        }
        walk_return_statement(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer,
        parser::{self, ASTVisitor},
        semantic_analyzer::symbol_table::Resolver,
    };

    use super::TypeChecker;

    #[test]
    fn test_valid() {
        let source = r#"
            struct Person {
                name: [char],
                age: int,
            }
            
            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                let d: char = 'a';
                let e: [Person] = new Person[23];
                e[0] = new Person;
                e[0].name = "Name";
                e[0].age = 23;
                e[1] = new Person;
                if (a - b == 1) {
                    print(a, b);
                }
                if (c / 2.0 == 10.0) {
                    let d: real = c / 2.0 + 10.0;
                }
                return a;
            }
            
            void print(a: int, b: int) {
                while (a == 10) {
                    print(b, 1);
                    a = a - 1;
                }
            }
            
            int sum(a: int, b: int) {
                return a + b;
            }
        "#;

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let symbols = Resolver::new().resolve(&module).expect("resolve");

        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        assert_eq!(
            checker.check().is_ok(),
            true,
            "source_text: {}, errors: {:?}",
            source,
            checker.check().unwrap_err()
        );
    }

    #[test]
    fn test_invalid() {
        let source = r#"
            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                if (a - b == 1) {
                    print(a, b);
                }
                if (c / 2.0 == 10.0) {
                    let d: real = c / 2.0 + 10;
                }
                2 = 4;
                return a;
            }
            
            void print(a: int, b: int) {
                while (a) {
                    print("Hello, World", 1);
                    a = a - 1;
                }
            }
            
            int sum(a: int, b: int) {
                return a + b;
            }
        "#;

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let symbols = Resolver::new().resolve(&module).expect("resolve");

        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        assert_eq!(
            checker.check().is_err() && checker.check().unwrap_err().len() == 4,
            true,
            "source_text: {}, errors: {:?}",
            source,
            checker.check().unwrap()
        );
    }

    #[test]
    fn test_error_carries_span() {
        // The `true` mismatched against `int` sits on line 3; the type error
        // must point there rather than at a default (0, 0) location.
        let source = "int main() {\n\n    let x: int = true;\n}\n";

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let symbols = Resolver::new().resolve(&module).expect("resolve");

        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);

        let errors = checker.check().expect_err("expected a type error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
        assert_eq!(errors[0].span.start.row, 3, "span: {:?}", errors[0].span);
        assert!(errors[0].span.start.col > 0, "span: {:?}", errors[0].span);
    }

    #[test]
    fn test_int_division_yields_int() {
        let ok = r#"int main() { let x: int = 7 / 2; return 0; }"#;
        let bad = r#"int main() { let x: real = 7 / 2; return 0; }"#;

        for (source, expect_ok) in [(ok, true), (bad, false)] {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let module = parser::Parser::new(tokens).parse().expect("parse");
            let symbols = Resolver::new().resolve(&module).expect("resolve");
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&module);
            assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
        }
    }

    #[test]
    fn test_len_intrinsic_types() {
        let cases = [
            (
                r#"int main() { let a: [int] = new int[3]; return len(a); }"#,
                true,
            ),
            (r#"int main() { return len("hi"); }"#, true),
            (
                r#"real main() { let a: [int] = new int[3]; return len(a); }"#,
                false,
            ),
            (r#"int main() { return len(5); }"#, false),
            (
                r#"int main() { let a: [int] = new int[3]; return len(a, a); }"#,
                false,
            ),
        ];

        for (source, expect_ok) in cases {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let module = parser::Parser::new(tokens).parse().expect("parse");
            let symbols = Resolver::new().resolve(&module).expect("resolve");
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&module);
            assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
        }
    }

    #[test]
    fn test_void_function_semantics() {
        let cases = [
            (r#"void f() { } int main() { f(); return 0; }"#, true),
            (r#"void f() { return 1; }"#, false),
            (
                r#"void f() { } int main() { let x: int = f(); return x; }"#,
                false,
            ),
            (r#"void f() { } int main() { return f(); }"#, false),
            (
                r#"void f() { } int g(a: int) { return a; } int main() { return g(f()); }"#,
                false,
            ),
        ];

        for (source, expect_ok) in cases {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let module = parser::Parser::new(tokens).parse().expect("parse");
            let symbols = Resolver::new().resolve(&module).expect("resolve");
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&module);
            assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
        }
    }

    #[test]
    fn test_call_result_is_not_assignable() {
        let source = r#"
            int f() { return 1; }

            int main() {
                f() = 1;
                return 0;
            }
        "#;

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let symbols = Resolver::new().resolve(&module).expect("resolve");

        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);

        let errors = checker
            .check()
            .expect_err("expected an unassignable-LHS error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }

    fn check_cases(cases: &[(&str, bool)]) {
        for (source, expect_ok) in cases {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let module = parser::Parser::new(tokens).parse().expect("parse");
            let symbols = Resolver::new().resolve(&module).expect("resolve");
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&module);
            assert_eq!(checker.check().is_ok(), *expect_ok, "source: {}", source);
        }
    }

    #[test]
    fn test_modulo_is_int_only() {
        check_cases(&[
            (r#"int main() { return 7 % 2; }"#, true),
            (
                r#"int main() { let x: real = 7.0 % 2.0; return 0; }"#,
                false,
            ),
            (
                r#"int main() { let b: bool = true % false; return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_sizeless_new_is_structs_only() {
        check_cases(&[
            (
                r#"struct P { x: int } int main() { let p: P = new P; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let a: [int] = new int[3]; return 0; }"#,
                true,
            ),
            (r#"int main() { let x: int = new int; return x; }"#, false),
            (
                r#"int main() { let a: [int] = new [int]; return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_for_statement_types() {
        check_cases(&[
            (
                r#"int main() { for (let i: int = 0; i < 3; i = i + 1) { i; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { for (let i: int = 0; i; i = i + 1) { } return 0; }"#,
                false,
            ),
            (
                r#"void f() { } int main() { for (let i: int = 0; i < 3; f()) { } return 0; }"#,
                true,
            ),
        ]);
    }

    #[test]
    fn test_bare_return_matrix() {
        check_cases(&[
            (
                r#"void f(a: bool) { if (a) { return; } a = !a; } int main() { return 0; }"#,
                true,
            ),
            (r#"int f() { return 1; } int main() { return 0; }"#, true),
            (r#"int f() { return; } int main() { return 0; }"#, false),
            (r#"void f() { return 1; } int main() { return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_function_names_are_not_assignable() {
        check_cases(&[(
            r#"
                int f() { return 1; }
                int g() { return 2; }
                int main() { f = g; return 0; }
            "#,
            false,
        )]);
    }

    #[test]
    fn test_binary_operator_type_rules() {
        check_cases(&[
            (r#"int main() { let x: int = 1 + 2 * 3; return x; }"#, true),
            (r#"int main() { let x: real = 1.0 + 2.0; return 0; }"#, true),
            (r#"int main() { let x: int = 1 + 2.0; return x; }"#, false),
            (r#"int main() { let x: bool = 1 < 2; return 0; }"#, true),
            (r#"int main() { let x: bool = 'a' < 'b'; return 0; }"#, true),
            (
                r#"int main() { let x: bool = true & false | true; return 0; }"#,
                true,
            ),
            (r#"int main() { let x: bool = 1 & 2; return 0; }"#, false),
            (
                r#"int main() { let x: char = 'a' + 'b'; return 0; }"#,
                false,
            ),
            (r#"int main() { let x: bool = 1 == 2.0; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_unary_operator_type_rules() {
        check_cases(&[
            (r#"int main() { let x: int = -5; return x; }"#, true),
            (r#"int main() { let x: real = -5.0; return 0; }"#, true),
            (r#"int main() { let x: bool = !true; return 0; }"#, true),
            (r#"int main() { let x: bool = -true; return 0; }"#, false),
            (r#"int main() { let x: int = !5; return x; }"#, false),
        ]);
    }

    #[test]
    fn test_array_literal_homogeneity() {
        check_cases(&[
            (
                r#"int main() { let a: [int] = [1, 2, 3]; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let a: [[int]] = [[1], [2]]; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let a: [int] = [1, 2.0]; return 0; }"#,
                false,
            ),
            (r#"int main() { let a: [int] = []; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_array_indexing_types() {
        check_cases(&[
            (
                r#"int main() { let a: [int] = new int[3]; let x: int = a[0]; return x; }"#,
                true,
            ),
            (
                r#"int main() { let a: [int] = new int[3]; let x: int = a[1.0]; return x; }"#,
                false,
            ),
            (
                r#"int main() { let x: int = 5; let y: int = x[0]; return y; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_struct_member_access_types() {
        check_cases(&[
            (
                r#"struct P { x: int } int main() { let p: P = new P; let a: int = p.x; return a; }"#,
                true,
            ),
            (
                r#"struct P { x: int } int main() { let p: P = new P; let a: int = p.y; return a; }"#,
                false,
            ),
            (
                r#"int main() { let x: int = 5; let a: int = x.y; return a; }"#,
                false,
            ),
            (
                r#"struct P { x: int } int main() { let p: P = new P; let a: real = p.x; return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_cast_rules() {
        check_cases(&[
            (r#"int main() { let x: real = 1 as real; return 0; }"#, true),
            (r#"int main() { let x: int = 1.5 as int; return x; }"#, true),
            (r#"int main() { let x: int = 'a' as int; return x; }"#, true),
            (
                r#"int main() { let x: int = true as int; return x; }"#,
                false,
            ),
            (r#"int main() { let x: int = 5 as int; return x; }"#, false),
        ]);
    }

    #[test]
    fn test_conditions_must_be_bool() {
        check_cases(&[
            (r#"int main() { if (true) { } return 0; }"#, true),
            (r#"int main() { while (false) { } return 0; }"#, true),
            (r#"int main() { if (1) { } return 0; }"#, false),
            (r#"int main() { while (1) { } return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_assignment_type_must_match() {
        check_cases(&[
            (r#"int main() { let x: int = 1; x = 2; return x; }"#, true),
            (
                r#"int main() { let x: real = 1.0; x = 2.0; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let x: int = 1; x = 2.0; return x; }"#,
                false,
            ),
            (
                r#"int main() { let a: [int] = new int[2]; a[0] = 5; return 0; }"#,
                true,
            ),
        ]);
    }

    #[test]
    fn test_unassignable_lhs_is_rejected() {
        check_cases(&[
            (r#"int main() { 2 = 4; return 0; }"#, false),
            (r#"int main() { let x: int = 1; -x = 2; return 0; }"#, false),
            (
                r#"int main() { let x: int = 1; (x + 1) = 2; return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_call_arity_and_argument_types() {
        check_cases(&[
            (
                r#"int add(a: int, b: int) { return a + b; } int main() { return add(1, 2); }"#,
                true,
            ),
            (
                r#"int add(a: int, b: int) { return a + b; } int main() { return add(1); }"#,
                false,
            ),
            (
                r#"int add(a: int, b: int) { return a + b; } int main() { return add(1, 2.0); }"#,
                false,
            ),
            (r#"int main() { let x: int = 5; return x(1); }"#, false),
        ]);
    }

    #[test]
    fn test_return_type_must_match() {
        check_cases(&[
            (r#"int f() { return 1; } int main() { return 0; }"#, true),
            (r#"real f() { return 1.0; } int main() { return 0; }"#, true),
            (
                r#"int f() { return true; } int main() { return 0; }"#,
                false,
            ),
            (r#"real f() { return 1; } int main() { return 0; }"#, false),
        ]);
    }
}
