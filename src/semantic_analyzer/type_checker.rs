use super::{errors::TypeErr, symbol_table::*};
use crate::parser::*;

pub struct TypeChecker<'a> {
    symbols: &'a SymbolTable,
    current_return_type: Type,
    errors: Vec<TypeErr>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> TypeChecker<'a> {
        TypeChecker {
            symbols,
            current_return_type: Type::Nil,
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
            (Int, Divide, Int)         => Ok(Real),
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
                if !self.is_assignable(left) {
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

    fn get_call_expression_type(
        &self,
        f: &Spanned<Expression>,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Type, TypeErr> {
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
                Ok(*ret_type)
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
            _ => Ok(typename.clone()),
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
            Call(f, args) => self.get_call_expression_type(f, args, span),
            Cast(expr, typename) => self.get_cast_expression_type(expr, typename, span),
            ArrayIndex(left, right) => self.get_array_index_expression_type(left, right, span),
            Access(left, member) => self.get_access_expression_type(left, member, span),
            Construct(typename, size) => self.get_construct_expression_type(typename, size, span),
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
        if let Err(e) = self.get_expression_type(expr) {
            self.errors.push(e);
        }
        walk_simple_statement(self, expr);
    }

    fn visit_return_statement(&mut self, expr: &Spanned<Expression>) {
        let ret_type = self.current_return_type.clone();
        self.ensure_type(expr, &ret_type);
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
    fn valid() {
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
                e[0].name = "Name";
                e[0].age = 23;
                e[1] = new Person;
                if (a - b == 1) {
                    print(a, b);
                }
                if (c / 2.0 == 10.0) {
                    let d: real = c / 2.0 + 10.0;
                }
                ret a;
            }
            
            nil print(a: int, b: int) {
                while (a == 10) {
                    print(b, 1);
                    a = a - 1;
                }
            }
            
            int sum(a: int, b: int) {
                ret a + b;
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
    fn invalid() {
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
                ret a;
            }
            
            nil print(a: int, b: int) {
                while (a) {
                    print("Hello, World", 1);
                    a = a - 1;
                }
            }
            
            int sum(a: int, b: int) {
                ret a + b;
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
    fn error_carries_span() {
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
    fn call_result_is_not_assignable() {
        let source = r#"
            int f() { ret 1; }

            int main() {
                f() = 1;
                ret 0;
            }
        "#;

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let symbols = Resolver::new().resolve(&module).expect("resolve");

        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);

        let errors = checker.check().expect_err("expected an unassignable-LHS error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
    }
}
