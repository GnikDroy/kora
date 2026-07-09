use std::collections::HashMap;

use super::{errors::TypeErr, symbol_resolver::*};
use crate::parser::*;

/// Built-in methods on `[T]`, dispatched by receiver type like struct
/// methods but implemented by the backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayMethod {
    Len,
    Push,
    Pop,
    Insert,
    Remove,
    Slice,
    Extend,
}

pub struct TypeChecker<'a> {
    symbols: &'a SymbolTable,
    current_return_type: Option<Type>,
    errors: Vec<TypeErr>,
    pub types: HashMap<NodeId, Type>,
    pub method_calls: HashMap<NodeId, SymbolId>,
    pub array_method_calls: HashMap<NodeId, ArrayMethod>,
    inferred: HashMap<SymbolId, Type>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> TypeChecker<'a> {
        TypeChecker {
            symbols,
            current_return_type: None,
            errors: Vec::new(),
            types: HashMap::new(),
            method_calls: HashMap::new(),
            array_method_calls: HashMap::new(),
            inferred: HashMap::new(),
        }
    }

    pub fn check(&self) -> Result<(), &[TypeErr]> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(&self.errors)
        }
    }

    fn get_array_type(
        &mut self,
        exprs: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Type, TypeErr> {
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
        &mut self,
        left: &Spanned<Expression>,
        op: &BinaryOp,
        right: &Spanned<Expression>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        use BinaryOp::*;
        use Type::*;

        let left_type = self.get_expression_type(left)?;
        let right_type = if matches!(op, Assign) {
            self.get_expression_type_expecting(right, &left_type)?
        } else {
            self.get_expression_type(right)?
        };

        #[rustfmt::skip]
        match (left_type, op, right_type) {
            (Int, Add, Int)            => Ok(Int),
            (Int, Subtract, Int)       => Ok(Int),
            (Int, Multiply, Int)       => Ok(Int),
            (Int, Divide, Int)         => Ok(Int),
            (Int, Modulo, Int)         => Ok(Int),
            (Int, BitAnd, Int)         => Ok(Int),
            (Int, BitOr, Int)          => Ok(Int),
            (Int, BitXor, Int)         => Ok(Int),
            (Int, ShiftLeft, Int)      => Ok(Int),
            (Int, ShiftRight, Int)     => Ok(Int),
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

            (l @ Array(_), Equality | NotEquality, r) if l == r && Self::is_comparable(&l) => Ok(Bool),
            (l @ Array(_), Add, r) if l == r => Ok(l),
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
        &mut self,
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

    fn get_copy_intrinsic_type(
        &mut self,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let [arg] = args else {
            return Err(TypeErr {
                msg: "copy expects exactly one argument",
                span: span.clone(),
            });
        };
        let ty = self.get_expression_type(arg)?;
        match ty {
            Type::Array(_) | Type::Struct(_) => Ok(ty),
            _ => Err(TypeErr {
                msg: "copy expects a reference type; scalars are value types",
                span: span.clone(),
            }),
        }
    }

    fn get_array_method_return_type(
        &mut self,
        f: &Spanned<Expression>,
        elem: Type,
        member: &str,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<Type>, TypeErr> {
        let (method, expected, ret) = match member {
            "len" => (ArrayMethod::Len, vec![], Some(Type::Int)),
            "push" => (ArrayMethod::Push, vec![elem], None),
            "pop" => (ArrayMethod::Pop, vec![], Some(elem)),
            "insert" => (ArrayMethod::Insert, vec![Type::Int, elem], None),
            "remove" => (ArrayMethod::Remove, vec![Type::Int], Some(elem)),
            "slice" => (
                ArrayMethod::Slice,
                vec![Type::Int, Type::Int],
                Some(Type::Array(Box::new(elem))),
            ),
            "extend" => (ArrayMethod::Extend, vec![Type::Array(Box::new(elem))], None),
            _ => {
                return Err(TypeErr {
                    msg: "Arrays have no such method: len, push, pop, insert, remove, slice, extend",
                    span: span.clone(),
                });
            }
        };
        if args.len() != expected.len() {
            return Err(TypeErr {
                msg: "Array method has different number of arguments",
                span: span.clone(),
            });
        }
        for (arg, arg_type) in args.iter().zip(expected.iter()) {
            if self.get_expression_type_expecting(arg, arg_type)? != *arg_type {
                return Err(TypeErr {
                    msg: "Arguments passed to array method do not match its signature",
                    span: arg.span.clone(),
                });
            }
        }
        self.array_method_calls.insert(f.id, method);
        Ok(ret)
    }

    fn get_method_call_return_type(
        &mut self,
        f: &Spanned<Expression>,
        method: SymbolId,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<Type>, TypeErr> {
        let Some(Type::Function(ret_type, arg_types)) = self.symbols.symbol(method).ty.clone()
        else {
            unreachable!("methods are declared with a function type");
        };
        // First argument is self.
        if args.len() != arg_types.len() - 1 {
            return Err(TypeErr {
                msg: "Method has different number of arguments",
                span: span.clone(),
            });
        }
        for (arg, arg_type) in args.iter().zip(arg_types[1..].iter()) {
            if self.get_expression_type_expecting(arg, arg_type)? != *arg_type {
                return Err(TypeErr {
                    msg: "Arguments passed to method do not match type signature for method",
                    span: arg.span.clone(),
                });
            }
        }
        self.method_calls.insert(f.id, method);
        Ok(ret_type.map(|return_type| *return_type))
    }

    fn get_call_expression_return_type(
        &mut self,
        f: &Spanned<Expression>,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<Type>, TypeErr> {
        // Intrinsics
        if matches!(&f.node, Expression::Identifier(name) if name == "copy") {
            return self.get_copy_intrinsic_type(args, span).map(Some);
        }

        // Method call `obj.method(...)`. Skipped when the access is a
        // module-qualified reference, which the resolver bound to a symbol.
        if let Expression::Access(obj, member) = &f.node
            && self.symbols.symbol_id_of_use(f.id).is_none()
        {
            match self.get_expression_type(obj)? {
                Type::Struct(struct_name) => {
                    if let Some(method) = self.symbols.struct_method(&struct_name.node, member) {
                        return self.get_method_call_return_type(f, method, args, span);
                    }
                }
                Type::Array(elem) => {
                    return self.get_array_method_return_type(f, *elem, member, args, span);
                }
                _ => {}
            }
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
                    if self.get_expression_type_expecting(arg, &arg_type)? != arg_type {
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
        &mut self,
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
        &mut self,
        left: &Spanned<Expression>,
        member: &str,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let left_type = self.get_expression_type(left)?;
        match left_type {
            Type::Struct(name) => self
                .symbols
                .struct_member(&name.node, member)
                .ok_or(TypeErr {
                    msg: "Invalid member for struct",
                    span: span.clone(),
                }),
            Type::Array(_) => Err(TypeErr {
                msg: "Array methods are not values; call them: a.len()",
                span: span.clone(),
            }),
            _ => Err(TypeErr {
                msg: "Access operator must have struct type on the left",
                span: span.clone(),
            }),
        }
    }

    fn is_comparable(ty: &Type) -> bool {
        match ty {
            Type::Int | Type::Real | Type::Bool | Type::Char => true,
            Type::Array(inner) => Self::is_comparable(inner),
            Type::Struct(_) | Type::Function(_, _) => false,
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
        &mut self,
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
        &mut self,
        typename: &Type,
        size: &Option<Box<Spanned<Expression>>>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        match size {
            Some(size) => {
                let expr_type = self.get_expression_type(size)?;
                if expr_type != Type::Int {
                    Err(TypeErr {
                        msg: "Array constructor must be passed an integer inside []",
                        span: span.clone(),
                    })
                } else if !matches!(typename, Type::Int | Type::Real | Type::Bool | Type::Char) {
                    // Only scalars have a zero fill so they can be constructed with a given size
                    Err(TypeErr {
                        msg: "new T[n] requires a scalar element type (int, real, char, bool); build reference arrays with [] then push()",
                        span: span.clone(),
                    })
                } else {
                    Ok(Type::Array(Box::new(typename.clone())))
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
            StructLiteral(_, _) => false,
        }
    }

    fn get_struct_literal_type(
        &mut self,
        typename: &Type,
        fields: &[(Spanned<String>, Spanned<Expression>)],
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let Type::Struct(name) = typename else {
            return Err(TypeErr {
                msg: "Only structs can be constructed with field initializers",
                span: span.clone(),
            });
        };
        for (i, (field, value)) in fields.iter().enumerate() {
            if fields[..i].iter().any(|(prev, _)| prev.node == field.node) {
                return Err(TypeErr {
                    msg: "Duplicate field in struct literal",
                    span: field.span.clone(),
                });
            }
            let Some(field_type) = self.symbols.struct_member(&name.node, &field.node) else {
                return Err(TypeErr {
                    msg: "Invalid member for struct",
                    span: field.span.clone(),
                });
            };
            if self.get_expression_type_expecting(value, &field_type)? != field_type {
                return Err(TypeErr {
                    msg: "Field value does not match the member's type",
                    span: value.span.clone(),
                });
            }
        }
        if Some(fields.len()) != self.symbols.struct_member_count(&name.node) {
            return Err(TypeErr {
                msg: "Struct literal must initialize every member",
                span: span.clone(),
            });
        }
        Ok(typename.clone())
    }

    fn get_expression_type(&mut self, expr: &Spanned<Expression>) -> Result<Type, TypeErr> {
        use Expression::*;
        let span = &expr.span;
        let typename = match &expr.node {
            IntegerLiteral(_) => Ok(Type::Int),
            RealLiteral(_) => Ok(Type::Real),
            CharLiteral(_) => Ok(Type::Char),
            StringLiteral(_) => Ok(Type::Array(Box::new(Type::Char))),
            BoolLiteral(_) => Ok(Type::Bool),
            Array(exprs) => self.get_array_type(exprs, span),
            Identifier(_) => {
                let id = self.symbols.symbol_id_of_use(expr.id).ok_or(TypeErr {
                    msg: "Undefined identifier",
                    span: span.clone(),
                })?;
                self.symbols
                    .symbol(id)
                    .ty
                    .clone()
                    .or_else(|| self.inferred.get(&id).cloned())
                    .ok_or(TypeErr {
                        msg: "Cannot determine the type of this identifier",
                        span: span.clone(),
                    })
            }
            Binary(left, op, right) => self.get_binary_expression_type(left, op, right, span),
            Unary(op, term) => self.get_unary_expression_type(op, term, span),
            Call(f, args) => self
                .get_call_expression_return_type(f, args, span)?
                .ok_or_else(|| TypeErr {
                    msg: "A void function returns no value and cannot be used as a value",
                    span: span.clone(),
                }),
            Cast(expr, typename) => self.get_cast_expression_type(expr, typename, span),
            ArrayIndex(left, right) => self.get_array_index_expression_type(left, right, span),
            Access(left, member) => match self.symbols.symbol_id_of_use(expr.id) {
                // A module-qualified access (`m.member`) the resolver bound to a symbol.
                Some(id) => self.symbols.symbol(id).ty.clone().ok_or(TypeErr {
                    msg: "Cannot determine the type of this member",
                    span: span.clone(),
                }),
                None => self.get_access_expression_type(left, member, span),
            },
            Construct(typename, size) => self.get_construct_expression_type(typename, size, span),
            StructLiteral(typename, fields) => self.get_struct_literal_type(typename, fields, span),
        }?;
        self.types.insert(expr.id, typename.clone());
        Ok(typename)
    }

    // If an expression is JUST a call to a void function. It is a valid statement.
    // In all other cases, a function that returns void cannot be used in other expressions.
    fn check_statement_expression(&mut self, expr: &Spanned<Expression>) {
        let result = match &expr.node {
            Expression::Call(f, args) => self
                .get_call_expression_return_type(f, args, &expr.span)
                .map(|typename| {
                    if let Some(typename) = typename {
                        self.types.insert(expr.id, typename);
                    }
                }),
            _ => self.get_expression_type(expr).map(|_| ()),
        };
        if let Err(e) = result {
            self.errors.push(e);
        }
    }

    /// Type-check an expression whose expected type is already known
    fn get_expression_type_expecting(
        &mut self,
        expr: &Spanned<Expression>,
        expected: &Type,
    ) -> Result<Type, TypeErr> {
        if let Expression::Array(elems) = &expr.node
            && let Type::Array(elem_type) = expected
        {
            for elem in elems.iter() {
                if self.get_expression_type_expecting(elem, elem_type)? != **elem_type {
                    return Err(TypeErr {
                        msg: "Array element does not match the expected element type",
                        span: elem.span.clone(),
                    });
                }
            }
            self.types.insert(expr.id, expected.clone());
            return Ok(expected.clone());
        }
        self.get_expression_type(expr)
    }

    fn ensure_type(&mut self, expr: &Spanned<Expression>, expected: &Type) {
        let expr_type = self.get_expression_type_expecting(expr, expected);
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
        name: &Spanned<String>,
        typename: Option<&Type>,
        expr: &Spanned<Expression>,
    ) {
        match typename {
            Some(typename) => self.ensure_type(expr, typename),
            None => match self.get_expression_type(expr) {
                Ok(typename) => {
                    let id = self.symbols.symbol_id_of_declaration(name.id).unwrap();
                    self.inferred.insert(id, typename);
                }
                Err(e) => self.errors.push(e),
            },
        }
        walk_let_statement(self, name, typename, expr);
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
        // The init must be visited first so an unannotated `let` is inferred
        // before the condition and step refer to it.
        self.visit_statement(init);
        self.ensure_type(cond, &Type::Bool);
        self.check_statement_expression(step);
        self.visit_statement(body);
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
        semantic_analyzer::symbol_resolver::Resolver,
    };

    use super::TypeChecker;
    use crate::loader::LoadedProgram;
    use crate::semantic_analyzer::symbol_resolver::test_support::{load_program, resolve_program};

    fn program_type_checks(program: &LoadedProgram) -> bool {
        let symbols = resolve_program(program).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        for module in &program.modules {
            checker.visit_module(&module.module);
        }
        checker.check().is_ok()
    }

    #[test]
    fn test_module_qualified_call_type_checks() {
        let program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.helper(); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        assert!(program_type_checks(&program));
    }

    #[test]
    fn test_module_qualified_call_arity_is_checked() {
        let program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.helper(1); }"#,
                ),
                ("util.kora", "int helper() { return 1; }"),
            ],
        );
        assert!(!program_type_checks(&program));
    }

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
                let e: [Person] = [];
                e.push(new Person);
                e[0].name = "Name";
                e[0].age = 23;
                e.push(new Person);
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
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

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
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

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
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

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
            let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&module);
            assert_eq!(checker.check().is_ok(), expect_ok, "source: {}", source);
        }
    }

    #[test]
    fn test_array_method_types() {
        let cases = [
            (
                r#"int main() { let a: [int] = new int[3]; return a.len(); }"#,
                true,
            ),
            (r#"int main() { return "hi".len(); }"#, true),
            (
                r#"int main() { let a = [1]; a.push(2); a.insert(0, 3); return a.remove(1); }"#,
                true,
            ),
            (
                r#"int main() { let a = [1]; a.remove(0); return 0; }"#,
                true,
            ),
            (
                r#"int main() { let m = [[1], [2]]; m[0].push(3); return m.remove(0).len(); }"#,
                true,
            ),
            (r#"int main() { let a = [1, 2, 3]; return a.pop(); }"#, true),
            (
                r#"int main() { let a = [1, 2, 3]; return a.slice(1, 3).len(); }"#,
                true,
            ),
            (
                r#"int main() { let a = [1]; let b = [2]; a.extend(b); return a.len(); }"#,
                true,
            ),
            (
                r#"int main() { let s = "abc".slice(0, 1); return s.len(); }"#,
                true,
            ),
            (
                r#"int main() { let a = [[1]]; a.extend([[2]]); return a[1][0]; }"#,
                true,
            ),
            (
                r#"int main() { let a = [1]; let x = a.extend([2]); return 0; }"#,
                false,
            ),
            (r#"int main() { let a = [1]; a.pop(1); return 0; }"#, false),
            (
                r#"int main() { let a = [1]; let x: bool = a.pop(); return 0; }"#,
                false,
            ),
            (r#"int main() { let a = [1]; return a.slice(1); }"#, false),
            (
                r#"int main() { let a = [1]; return a.slice(0, 1); }"#,
                false,
            ),
            (
                r#"int main() { let a = [1]; a.extend(2); return 0; }"#,
                false,
            ),
            (
                r#"int main() { let a = [1]; let b = ["x"]; a.extend(b); return 0; }"#,
                false,
            ),
            (
                r#"real main() { let a: [int] = new int[3]; return a.len(); }"#,
                false,
            ),
            (r#"int main() { let a = [1]; return a.len(1); }"#, false),
            (
                r#"int main() { let a = [1]; a.push(true); return 0; }"#,
                false,
            ),
            (
                r#"int main() { let a = [1]; a.push(1, 2); return 0; }"#,
                false,
            ),
            (
                r#"int main() { let a = [1]; a.insert(1.0, 2); return 0; }"#,
                false,
            ),
            (
                r#"int main() { let a = [1]; let x: bool = a.remove(0); return 0; }"#,
                false,
            ),
            (r#"int main() { let a = [1]; return a.push(2); }"#, false),
            (
                r#"int main() { let a = [1]; let f = a.len; return 0; }"#,
                false,
            ),
            (r#"int main() { let n = 5; return n.len(); }"#, false),
        ];

        for (source, expect_ok) in cases {
            let tokens = lexer::Lexer::lex(source).expect("lex");
            let module = parser::Parser::new(tokens).parse().expect("parse");
            let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
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
            let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
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
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

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
            let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
            let mut checker = TypeChecker::new(&symbols);
            checker.visit_module(&module);
            assert_eq!(checker.check().is_ok(), *expect_ok, "source: {}", source);
        }
    }

    #[test]
    fn test_empty_array_literals_where_type_is_expected() {
        check_cases(&[
            (r#"int main() { let a: [int] = []; return a.len(); }"#, true),
            (
                r#"int main() { let a: [int] = []; a = []; a.push(1); return a[0]; }"#,
                true,
            ),
            (
                r#"int count(v: [int]) { return v.len(); } int main() { return count([]); }"#,
                true,
            ),
            (
                r#"[int] empty() { return []; } int main() { return empty().len(); }"#,
                true,
            ),
            (
                r#"int main() { let m: [[int]] = [[], [1]]; return m[0].len(); }"#,
                true,
            ),
            (
                r#"int main() { let m: [[int]] = []; m.push([]); m[0].push(1); return m[0][0]; }"#,
                true,
            ),
            (r#"int main() { let a = []; return 0; }"#, false),
            (r#"int main() { let a: int = []; return 0; }"#, false),
            (r#"int main() { let a: [int] = [true]; return 0; }"#, false),
            (r#"int main() { let a: [int] = [[]]; return 0; }"#, false),
            (r#"int main() { []; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_struct_literals() {
        let prelude = r#"
            struct Point { x: int, y: int }
            struct Line { a: Point, b: Point }
            struct Bag { items: [int] }
        "#;
        let cases = [
            (
                r#"int main() { let p = new Point { x: 1, y: 2 }; return p.x + p.y; }"#,
                true,
            ),
            (
                r#"int main() { let l = new Line { a: new Point { x: 0, y: 0 }, b: new Point { x: 1, y: 1 } }; return l.b.x; }"#,
                true,
            ),
            (
                r#"int main() { let b = new Bag { items: [] }; b.items.push(3); return b.items[0]; }"#,
                true,
            ),
            (
                r#"int main() { return (new Point { x: 1, y: 2 }).x; }"#,
                true,
            ),
            (
                r#"int main() { let p = new Point { x: 1 }; return 0; }"#,
                false,
            ),
            (
                r#"int main() { let p = new Point { x: 1, y: 2, x: 3 }; return 0; }"#,
                false,
            ),
            (
                r#"int main() { let p = new Point { x: 1, y: 2, z: 3 }; return 0; }"#,
                false,
            ),
            (
                r#"int main() { let p = new Point { x: true, y: 2 }; return 0; }"#,
                false,
            ),
            (
                r#"int main() { let p = new int { x: 1 }; return 0; }"#,
                false,
            ),
            (
                r#"int main() { new Point { x: 1, y: 2 } = new Point { x: 3, y: 4 }; return 0; }"#,
                false,
            ),
        ];
        let sources: Vec<(String, bool)> = cases
            .iter()
            .map(|(body, ok)| (format!("{prelude}{body}"), *ok))
            .collect();
        let cases: Vec<(&str, bool)> = sources.iter().map(|(s, ok)| (s.as_str(), *ok)).collect();
        check_cases(&cases);
    }

    #[test]
    fn test_method_calls() {
        let prelude = r#"
            struct P { x: int }
            impl P {
                int get(self) { return self.x; }
                void set(self, v: int) { self.x = v; }
                P me(self) { return self; }
            }
        "#;
        let cases = [
            (
                "int main() { let p = new P; p.set(3); return p.get(); }",
                true,
            ),
            (
                "int main() { let p = new P; return p.me().me().get(); }",
                true,
            ),
            ("int main() { return (new P).get(); }", true),
            ("int main() { let p = new P; return p.get(1); }", false),
            (
                "int main() { let p = new P; p.set(true); return 0; }",
                false,
            ),
            (
                "int main() { let p = new P; let v = p.set(1); return 0; }",
                false,
            ),
            (
                "int main() { let p = new P; let f = p.get; return 0; }",
                false,
            ),
            ("int main() { let p = new P; return p.nope(); }", false),
            ("int main() { let p = new P; return p.x(); }", false),
            ("int main() { let n = 5; return n.get(); }", false),
        ];
        let sources: Vec<(String, bool)> = cases
            .iter()
            .map(|(body, ok)| (format!("{prelude}{body}"), *ok))
            .collect();
        let cases: Vec<(&str, bool)> = sources.iter().map(|(s, ok)| (s.as_str(), *ok)).collect();
        check_cases(&cases);
    }

    #[test]
    fn test_method_call_resolutions_are_recorded() {
        let source = r#"
            struct P { x: int }
            impl P { int get(self) { return self.x; } }
            int main() { let p = new P; return p.get(); }
        "#;
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        checker.check().expect("check");
        let (_, method) = checker.method_calls.iter().next().expect("one method call");
        assert_eq!(symbols.symbol(*method).name, "get");
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
                r#"int main() { let x: bool = true && false || true; return 0; }"#,
                true,
            ),
            (r#"int main() { let x: bool = 1 && 2; return 0; }"#, false),
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
            (r#"int main() { let a: [int] = []; return 0; }"#, true),
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
            (
                r#"int main() { let a: int = 1; let b: int = 2; a = b = 3; return a; }"#,
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
    fn test_let_type_inference() {
        check_cases(&[
            (r#"int main() { let x = 5; return x; }"#, true),
            (
                r#"int main() { let x = 5; let y = x + 1; return y; }"#,
                true,
            ),
            (
                r#"int main() { let r = 1.5 * 2.0; let ok = r > 2.0; if (ok) { return 1; } return 0; }"#,
                true,
            ),
            (r#"int main() { let a = [1, 2, 3]; return a[0]; }"#, true),
            (
                r#"struct P { x: int } int main() { let p = new P; return p.x; }"#,
                true,
            ),
            (
                r#"int f(a: int) { return a; } int main() { let x = f(41) + 1; return x; }"#,
                true,
            ),
            (r#"int main() { let x = 5; return x + 1.0; }"#, false),
            (r#"int main() { let x = 5.0; return x; }"#, false),
            (
                r#"void f() { } int main() { let x = f(); return 0; }"#,
                false,
            ),
            (r#"int main() { let x = []; return 0; }"#, false),
            (
                r#"int main() { for (let i = 0; i < 3; i = i + 1) { i; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { for (let b = true; b; b = !b) { } return 0; }"#,
                true,
            ),
        ]);
    }

    #[test]
    fn test_bitwise_operators_are_int_only() {
        check_cases(&[
            (r#"int main() { return 12 & 10 | 5 ^ 3; }"#, true),
            (r#"int main() { return 1 << 4 >> 2; }"#, true),
            (
                r#"int main() { let b = 5 & 2 == 0; if (b) { return 1; } return 0; }"#,
                true,
            ),
            (r#"int main() { let b = true & false; return 0; }"#, false),
            (r#"int main() { let r = 1.5 | 1.0; return 0; }"#, false),
            (r#"int main() { let r = 1.0 << 2; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_array_equality_is_structural_and_recursive() {
        check_cases(&[
            (
                r#"int main() { let b: bool = [1, 2] == [1, 2]; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let s: string = "abc"; if (s == "quit") { return 1; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { let s = "abc"; let b = s != "abc"; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let b = [[1], [2]] == [[1], [2]]; return 0; }"#,
                true,
            ),
            (r#"int main() { let b = [1] == [1.0]; return 0; }"#, false),
            (r#"int main() { let b = [1] == 1; return 0; }"#, false),
            (r#"int main() { let b = "a" == 'a'; return 0; }"#, false),
            (r#"int main() { let b = [1] < [2]; return 0; }"#, false),
            (
                r#"struct P { x: int } int main() { let b = new P[1] == new P[1]; return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_method_names_are_not_reserved() {
        check_cases(&[
            (
                r#"int len(x: int) { return x; } int main() { let a = [1]; return len(a.len()); }"#,
                true,
            ),
            (
                r#"int push(x: int) { return x; } int main() { return push(3); }"#,
                true,
            ),
        ]);
    }

    #[test]
    fn test_copy_intrinsic() {
        check_cases(&[
            (
                r#"int main() { let a = [1, 2]; let b = copy(a); b.push(3); return a.len(); }"#,
                true,
            ),
            (
                r#"struct P { x: int } int main() { let p = new P { x: 1 }; let q = copy(p); return q.x; }"#,
                true,
            ),
            (
                r#"int main() { let s = copy("hi"); return s.len(); }"#,
                true,
            ),
            (
                r#"int main() { let m = [[1]]; let n = copy(m); return n[0][0]; }"#,
                true,
            ),
            (r#"int main() { copy([1]); return 0; }"#, true),
            (r#"int main() { let x = copy(5); return 0; }"#, false),
            (r#"int main() { let x = copy('a'); return 0; }"#, false),
            (
                r#"int main() { let a = [1]; copy(a, a); return 0; }"#,
                false,
            ),
            (r#"int main() { let x = copy(); return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_sized_new_is_scalar_only() {
        check_cases(&[
            (r#"int main() { let a = new int[3]; return a[0]; }"#, true),
            (r#"int main() { let a = new char[8]; return 0; }"#, true),
            (r#"int main() { let a = new real[2]; return 0; }"#, true),
            (r#"int main() { let a = new bool[2]; return 0; }"#, true),
            (
                r#"struct P { x: int } int main() { let a = new P[3]; return 0; }"#,
                false,
            ),
            (r#"int main() { let a = new [int][3]; return 0; }"#, false),
            (r#"int main() { let a = new string[3]; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_array_plus_is_concatenation() {
        check_cases(&[
            (r#"int main() { let a = [1] + [2]; return a.len(); }"#, true),
            (r#"int main() { let a = [1, 2] + [3]; return a[2]; }"#, true),
            (
                r#"int main() { let s: string = "ab" + "cd"; return s.len(); }"#,
                true,
            ),
            (
                r#"int main() { let m = [[1]] + [[2]]; return m[1][0]; }"#,
                true,
            ),
            (r#"int main() { let a = [1] + [2.0]; return 0; }"#, false),
            (r#"int main() { let a = [1] + 2; return 0; }"#, false),
            (r#"int main() { let a = 1 + [2]; return 0; }"#, false),
            (r#"int main() { let a = [1] - [2]; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_string_alias_is_interchangeable_with_char_array() {
        check_cases(&[
            (
                r#"int main() { let s: string = "abc"; return s.len(); }"#,
                true,
            ),
            (
                r#"int main() { let s: string = "abc"; let t: [char] = s; t = s; s = t; return 0; }"#,
                true,
            ),
            (
                r#"string first_word() { return "hi"; }
                   void take(s: [char]) { }
                   int main() { take(first_word()); return 0; }"#,
                true,
            ),
            (
                r#"int main() { let words: [string] = ["a", "b"]; return words.len(); }"#,
                true,
            ),
            (
                r#"int main() { let s: string = "abc"; s[0] = 'x'; return s[0] as int; }"#,
                true,
            ),
            (r#"int main() { let s: string = 5; return 0; }"#, false),
            (r#"int main() { let s: string = 'a'; return 0; }"#, false),
        ]);
    }

    #[test]
    fn test_expression_types_are_recorded() {
        use crate::parser::{Expression, Statement, Type};

        let source = r#"
            void f() { }
            int main() { let x: real = 1.5 + 2.5; f(); return 0; }
        "#;
        let tokens = lexer::Lexer::lex(source).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new().resolve(&[&module]).expect("resolve");
        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        checker.check().expect("check");

        let body = &module.functions[1].node.statement;
        let Statement::Compound(stmts) = &body.node else {
            panic!("expected compound body");
        };
        let Statement::Let(_, _, init) = &stmts[0].node else {
            panic!("expected let statement");
        };
        let Expression::Binary(left, _, right) = &init.node else {
            panic!("expected binary initializer");
        };
        assert_eq!(checker.types.get(&init.id), Some(&Type::Real));
        assert_eq!(checker.types.get(&left.id), Some(&Type::Real));
        assert_eq!(checker.types.get(&right.id), Some(&Type::Real));

        let Statement::Simple(call) = &stmts[1].node else {
            panic!("expected call statement");
        };
        assert_eq!(checker.types.get(&call.id), None);
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
