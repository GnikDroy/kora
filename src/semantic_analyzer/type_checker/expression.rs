use super::TypeChecker;
use super::builtins;
use crate::parser::*;
use crate::semantic_analyzer::errors::TypeErr;
use crate::semantic_analyzer::symbol_resolver::*;

impl TypeChecker<'_> {
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
                msg: "Cannot infer type of empty array.",
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
        let is_equality = matches!(op, BinaryOp::Equality | BinaryOp::NotEquality);
        let left_none = matches!(left.node, Expression::NoneLiteral);
        let right_none = matches!(right.node, Expression::NoneLiteral);
        if is_equality && (left_none || right_none) {
            if left_none && right_none {
                return Err(TypeErr {
                    msg: "`none` cannot be compared with `none`",
                    span: span.clone(),
                });
            }
            let other = if left_none { right } else { left };
            let other_type = self.get_expression_type(other)?;
            if let Type::Optional(_) = other_type {
                self.types
                    .insert(if left_none { left.id } else { right.id }, other_type);
                return Ok(Type::Bool);
            }
            return Err(TypeErr {
                msg: "`none` can only be compared against an optional (T?)",
                span: span.clone(),
            });
        }

        let left_type = self.get_expression_type(left)?;

        // This assign block has to appear before right type is computed.
        if matches!(op, BinaryOp::Assign) {
            let right_type = self.get_expression_type_expecting(right, &left_type)?;
            // A fn-typed variable, array slot, or field is assignable, but a
            // bare function name is not a location.
            if !self.is_assignable(left) || self.is_function_name(left, &left_type) {
                return Err(TypeErr {
                    msg: "LHS of assign expression is not assignable",
                    span: span.clone(),
                });
            }
            if left_type != right_type {
                return Err(TypeErr {
                    msg: "LHS and RHS of assign expression don't match",
                    span: span.clone(),
                });
            }
            return Ok(left_type);
        }

        let right_type = self.get_expression_type(right)?;

        // Equality for optionals
        if is_equality && (builtins::is_optional(&left_type) || builtins::is_optional(&right_type))
        {
            let l = builtins::strip_optional(&left_type);
            let r = builtins::strip_optional(&right_type);
            if l == r && builtins::is_comparable(l) {
                return Ok(Type::Bool);
            }
            return Err(TypeErr {
                msg: "Optionals can only be compared with a matching, comparable type",
                span: span.clone(),
            });
        }

        builtins::binary_result(&left_type, op, &right_type).ok_or_else(|| TypeErr {
            msg: "Binary operator cannot be applied to the types",
            span: span.clone(),
        })
    }

    fn get_unary_expression_type(
        &mut self,
        op: &UnaryOp,
        expr: &Spanned<Expression>,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let operand = self.get_expression_type(expr)?;
        builtins::unary_result(op, &operand).ok_or_else(|| TypeErr {
            msg: "Unary operator cannot be applied to the types",
            span: span.clone(),
        })
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
        builtins::copy_result(&ty).ok_or_else(|| TypeErr {
            msg: "copy expects a reference type; scalars are value types",
            span: span.clone(),
        })
    }

    fn get_array_method_return_type(
        &mut self,
        f: &Spanned<Expression>,
        elem: Type,
        member: &str,
        args: &[Spanned<Expression>],
        span: &Span,
    ) -> Result<Option<Type>, TypeErr> {
        let Some((method, expected, ret)) = builtins::array_method(elem, member) else {
            return Err(TypeErr {
                msg: "Arrays have no such method: len, push, pop, insert, remove, slice, extend",
                span: span.clone(),
            });
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

        // Method call `obj.method(...)`. Skipped when the access is a module.
        if let Expression::Access(obj, member) = &f.node
            && self.symbols.symbol_id_of_use(f.id).is_none()
        {
            match self.get_expression_type(obj)? {
                Type::Struct(sr) => {
                    if let Some(decl) = self.symbols.struct_decl_of(&sr)
                        && let Some(method) = self.symbols.struct_method(decl, member)
                    {
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
            Type::Struct(sr) => self
                .symbols
                .struct_decl_of(&sr)
                .and_then(|decl| self.symbols.struct_member(&self.modules, decl, member))
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

    fn get_cast_expression_type(
        &mut self,
        expr: &Spanned<Expression>,
        typename: &Type,
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let expr_type = self.get_expression_type(expr)?;
        if builtins::is_cast_possible(&expr_type, typename) {
            Ok(typename.clone())
        } else {
            Err(TypeErr {
                msg: "Cannot cast to type",
                span: span.clone(),
            })
        }
    }

    fn struct_is_default_constructible(&self, sr: &StructRef) -> bool {
        let Some(decl) = self.symbols.struct_decl_of(sr) else {
            return false;
        };
        self.struct_has_default(decl, &mut Vec::new())
    }

    /// A struct has a default when every member
    /// - is a scalar
    /// - is an optional
    /// - is a list
    /// - default constructible struct (requires cycle detection)
    fn struct_has_default(&self, decl: NodeId, visiting: &mut Vec<NodeId>) -> bool {
        if visiting.contains(&decl) {
            return false;
        }
        let members: Vec<Type> = self
            .symbols
            .struct_members(&self.modules, decl)
            .iter()
            .map(|m| m.node.typename.clone())
            .collect();
        visiting.push(decl);
        let ok = members.iter().all(|ty| match ty {
            Type::Struct(inner) => match self.symbols.struct_decl_of(inner) {
                Some(inner_decl) => self.struct_has_default(inner_decl, visiting),
                None => false,
            },
            Type::Function(_, _) => false,
            _ => true,
        });
        visiting.pop();
        ok
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
                    return Err(TypeErr {
                        msg: "Array constructor must be passed an integer inside []",
                        span: span.clone(),
                    });
                }
                let ok = match typename {
                    Type::Struct(sr) => self.struct_is_default_constructible(sr),
                    Type::Opaque => true,
                    other => builtins::is_scalar(other),
                };
                if !ok {
                    return Err(TypeErr {
                        msg: "new T[n] needs a scalar element or a default-constructible struct (members all scalar, optional, or list); build others with [] then push()",
                        span: span.clone(),
                    });
                }
                Ok(Type::Array(Box::new(typename.clone())))
            }
            _ => match typename {
                Type::Struct(sr) if self.struct_is_default_constructible(sr) => {
                    Ok(typename.clone())
                }
                Type::Struct(_) => Err(TypeErr {
                    msg: "new S needs every member to have a default (scalar, optional, or list); use a struct literal new S { ... } instead",
                    span: span.clone(),
                }),
                _ => Err(TypeErr {
                    msg: "Only structs can be constructed without a size: new <struct>",
                    span: span.clone(),
                }),
            },
        }
    }

    fn is_assignable(&self, expr: &Spanned<Expression>) -> bool {
        use Expression::*;
        matches!(expr.node, Identifier(_) | ArrayIndex(_, _) | Access(_, _))
    }

    fn is_function_name(&self, left: &Spanned<Expression>, left_type: &Type) -> bool {
        matches!(left_type, Type::Function(_, _))
            && matches!(&left.node, Expression::Identifier(_))
            && self
                .symbols
                .symbol_id_of_use(left.id)
                .is_some_and(|id| self.is_function_symbol(id))
    }

    fn is_function_symbol(&self, id: SymbolId) -> bool {
        self.modules.iter().any(|m| {
            m.functions
                .iter()
                .map(|f| f.id)
                .chain(m.extern_functions.iter().map(|f| f.id))
                .any(|decl| self.symbols.symbol_id_of_declaration(decl) == Some(id))
        })
    }

    fn get_struct_literal_type(
        &mut self,
        typename: &Type,
        fields: &[(Spanned<String>, Spanned<Expression>)],
        span: &Span,
    ) -> Result<Type, TypeErr> {
        let Type::Struct(sr) = typename else {
            return Err(TypeErr {
                msg: "Only structs can be constructed with field initializers",
                span: span.clone(),
            });
        };
        let decl = self.symbols.struct_decl_of(sr);
        for (i, (field, value)) in fields.iter().enumerate() {
            if fields[..i].iter().any(|(prev, _)| prev.node == field.node) {
                return Err(TypeErr {
                    msg: "Duplicate field in struct literal",
                    span: field.span.clone(),
                });
            }
            let Some(field_type) =
                decl.and_then(|d| self.symbols.struct_member(&self.modules, d, &field.node))
            else {
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
        if Some(fields.len()) != decl.map(|d| self.symbols.struct_members(&self.modules, d).len()) {
            return Err(TypeErr {
                msg: "Struct literal must initialize every member",
                span: span.clone(),
            });
        }
        Ok(typename.clone())
    }

    pub(super) fn get_expression_type(
        &mut self,
        expr: &Spanned<Expression>,
    ) -> Result<Type, TypeErr> {
        use Expression::*;
        let span = &expr.span;
        let typename = match &expr.node {
            IntegerLiteral(_) => Ok(Type::Int),
            RealLiteral(_) => Ok(Type::Real),
            CharLiteral(_) => Ok(Type::Char),
            StringLiteral(_) => Ok(Type::Array(Box::new(Type::Char))),
            BoolLiteral(_) => Ok(Type::Bool),
            NoneLiteral => Err(TypeErr {
                msg: "Cannot infer the type of `none`; give the target an optional type T?",
                span: span.clone(),
            }),
            TypeApplication(_, _) => Err(TypeErr {
                msg: "generic call was not instantiated",
                span: span.clone(),
            }),
            Unwrap(inner) => match self.get_expression_type(inner)? {
                Type::Optional(ty) => Ok(*ty),
                _ => Err(TypeErr {
                    msg: "`!` force-unwraps an optional (T?); this operand is not optional",
                    span: span.clone(),
                }),
            },
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
    pub(super) fn check_statement_expression(&mut self, expr: &Spanned<Expression>) {
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
        // Optionals wrap implicity, but unwrap explicity with !
        if let Type::Optional(inner) = expected {
            if matches!(&expr.node, Expression::NoneLiteral) {
                self.types.insert(expr.id, expected.clone());
                return Ok(expected.clone());
            }
            let actual = self.get_expression_type_expecting(expr, inner)?;
            if actual == **inner || actual == *expected {
                return Ok(expected.clone());
            }
            return Ok(actual);
        }

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

    pub(super) fn ensure_type(&mut self, expr: &Spanned<Expression>, expected: &Type) {
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

#[cfg(test)]
mod tests {
    use super::super::test_support::{analyze, check_cases, program_type_checks};
    use super::TypeChecker;
    use crate::parser::ASTVisitor;
    use crate::semantic_analyzer::symbol_resolver::test_support::load_program;

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

        let (symbols, program) = analyze(source);
        let module = &program.modules[0].module;
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
        checker.visit_module(module);
        let result = checker.check();
        assert!(
            result.is_ok(),
            "source_text: {}, errors: {:?}",
            source,
            result.unwrap_err()
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

        let (symbols, program) = analyze(source);
        let module = &program.modules[0].module;
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
        checker.visit_module(module);
        let result = checker.check();
        assert!(result.is_err(), "source_text: {}", source);
        assert_eq!(result.unwrap_err().len(), 4, "source_text: {}", source);
    }

    #[test]
    fn test_error_carries_span() {
        // The `true` mismatched against `int` sits on line 3; the type error
        // must point there rather than at a default (0, 0) location.
        let source = "int main() {\n\n    let x: int = true;\n}\n";

        let (symbols, program) = analyze(source);
        let module = &program.modules[0].module;
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
        checker.visit_module(module);

        let errors = checker.check().expect_err("expected a type error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
        assert_eq!(errors[0].span.start.row, 3, "span: {:?}", errors[0].span);
        assert!(errors[0].span.start.col > 0, "span: {:?}", errors[0].span);
    }

    #[test]
    fn test_array_method_types() {
        check_cases(&[
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
        ]);
    }

    #[test]
    fn test_void_function_semantics() {
        check_cases(&[
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
        ]);
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

        let (symbols, program) = analyze(source);
        let module = &program.modules[0].module;
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
        checker.visit_module(module);

        let errors = checker
            .check()
            .expect_err("expected an unassignable-LHS error");
        assert_eq!(errors.len(), 1, "errors: {:?}", errors);
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
    fn test_struct_literals_build_recursive_structures() {
        check_cases(&[
            (
                r#"struct Node { value: int, next: Node? }
                   int main() {
                       let leaf = new Node { value: 3, next: none };
                       let head = new Node { value: 1, next: new Node { value: 2, next: leaf } };
                       return head.next!.value;
                   }"#,
                true,
            ),
            (
                r#"struct Node { value: int, next: Node? }
                   int main() {
                       let a = new Node { value: 1, next: none };
                       a.next = a;
                       return a.next!.value;
                   }"#,
                true,
            ),
            (
                r#"struct Point { x: int, y: int }
                   struct Segment { start: Point, end: Point }
                   int main() {
                       let p = new Point { x: 0, y: 0 };
                       let s = new Segment { start: p, end: new Point { x: 1, y: 1 } };
                       return s.start.x + s.end.x;
                   }"#,
                true,
            ),
        ]);
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
        let (symbols, program) = analyze(source);
        let module = &program.modules[0].module;
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
        checker.visit_module(module);
        checker.check().expect("check");
        let (_, method) = checker.method_calls.iter().next().expect("one method call");
        assert_eq!(symbols.symbol(*method).name, "get");
    }

    #[test]
    fn test_sizeless_new_is_default_constructible_structs_only() {
        check_cases(&[
            (
                r#"struct P { x: int } int main() { let p: P = new P; return 0; }"#,
                true,
            ),
            (
                r#"struct P { name: [char], age: int } int main() { let p = new P; return 0; }"#,
                true,
            ),
            (
                r#"struct Node { value: int, next: Node? } int main() { let n = new Node; return 0; }"#,
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
            (
                r#"struct P { x: int } struct Line { a: P, b: P } int main() { let l = new Line; return 0; }"#,
                true,
            ),
            (
                r#"struct A { b: B? } struct B { a: A } int main() { let a = new A; let b = new B; return 0; }"#,
                true,
            ),
            (
                r#"struct N { next: N } int main() { let n = new N; return 0; }"#,
                false,
            ),
            (
                r#"struct A { b: B } struct B { a: A } int main() { let a = new A; return 0; }"#,
                false,
            ),
            (
                r#"struct N { p: P } struct P { q: Q } struct Q { n: N } int main() { let n = new N; return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_sized_new_allows_scalars_and_default_constructible_structs() {
        check_cases(&[
            (r#"int main() { let a = new int[3]; return a[0]; }"#, true),
            (r#"int main() { let a = new char[8]; return 0; }"#, true),
            (r#"int main() { let a = new real[2]; return 0; }"#, true),
            (r#"int main() { let a = new bool[2]; return 0; }"#, true),
            (
                r#"struct P { x: int } int main() { let a = new P[3]; return a[0].x; }"#,
                true,
            ),
            (
                r#"struct P { tags: [int], k: int } int main() { let a = new P[2]; return a[0].k; }"#,
                true,
            ),
            (
                r#"struct P { x: int } struct Line { a: P } int main() { let a = new Line[2]; return a[0].a.x; }"#,
                true,
            ),
            (
                r#"struct N { next: N } int main() { let a = new N[2]; return 0; }"#,
                false,
            ),
            (r#"int main() { let a = new [int][3]; return 0; }"#, false),
            (r#"int main() { let a = new string[3]; return 0; }"#, false),
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
    fn test_optionals() {
        check_cases(&[
            (r#"int main() { let x: int? = 5; return 0; }"#, true),
            (r#"int main() { let x: int? = none; return 0; }"#, true),
            (
                r#"int main() { let x: int? = 5; x = none; x = 7; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let x: int? = 5; let y: int = x!; return y; }"#,
                true,
            ),
            (
                r#"int main() { let x: int? = none; if (x == none) { return 1; } if (x != none) { return 2; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { let a: int? = 1; let b: int? = 2; if (a == b) { return 1; } if (a == 3) { return 2; } return 0; }"#,
                true,
            ),
            (
                r#"struct Node { value: int, next: Node? }
                   int main() { let n = new Node { value: 1, next: none }; return n.value; }"#,
                true,
            ),
            (
                r#"struct Node { value: int, next: Node? }
                   int head(n: Node) { if (n.next != none) { return n.next!.value; } return n.value; }
                   int main() { return 0; }"#,
                true,
            ),
            (
                r#"int? find(k: int) { if (k > 0) { return k; } return none; }
                   int main() { let r = find(2); if (r != none) { return r!; } return 0; }"#,
                true,
            ),
            (
                r#"int main() { let xs: [int?] = [1, none, 3]; return 0; }"#,
                true,
            ),
            (r#"int main() { let a: [int]? = none; return 0; }"#, true),
            (r#"int main() { let a: [int]? = []; return 0; }"#, true),
            (r#"int main() { let x = none; return 0; }"#, false),
            (r#"int main() { let x: int = none; return 0; }"#, false),
            (r#"int main() { let x: int = 5; return x!; }"#, false),
            (
                r#"int main() { let x: int? = 5; let y: int = x; return y; }"#,
                false,
            ),
            (
                r#"int main() { let x: int = 5; if (x == none) { return 1; } return 0; }"#,
                false,
            ),
            (r#"int main() { if (none == none) { } return 0; }"#, false),
            (r#"int main() { if (none) { } return 0; }"#, false),
            (r#"int main() { let x: int? = true; return 0; }"#, false),
            (
                r#"int main() { let a: int? = 1; let b: real? = 2.0; if (a == b) { } return 0; }"#,
                false,
            ),
        ]);
    }

    #[test]
    fn test_extern_ctype_projection() {
        check_cases(&[
            (
                "extern int32 f(x: int8); int main() { return f(300); }",
                true,
            ),
            (
                "extern float32 g(x: float32); int main() { let y: real = g(1.5); return 0; }",
                true,
            ),
            (
                r#"extern cstring h(s: cstring); int main() { let t: string = h("x"); return t.len(); }"#,
                true,
            ),
            (
                r#"extern cstring? e(n: cstring); int main() { if (e("p") == none) { return 1; } return 0; }"#,
                true,
            ),
            (
                r#"extern csize strlen(s: cstring); int main() { return strlen("abc"); }"#,
                true,
            ),
            (
                "extern int32 f(); int main() { let x: real = f(); return 0; }",
                false,
            ),
            (
                "extern void f(x: float32); int main() { f(1); return 0; }",
                false,
            ),
        ]);
    }

    #[test]
    fn test_opaque() {
        check_cases(&[
            (
                r#"extern opaque make();
                   extern void use_handle(h: opaque);
                   opaque pass(h: opaque) { return h; }
                   struct S { h: opaque, m: opaque? }
                   int main() {
                       let a = make();
                       use_handle(pass(a));
                       let s = new S { h: a, m: none };
                       s.m = a;
                       let xs = [a, make()];
                       let ys = new opaque[3];
                       let r = 0;
                       if (a == a && a != make()) { r = r + 1; }
                       if (s.m != none && s.m! == a) { r = r + 2; }
                       if (xs == ys) { r = r + 4; }
                       return r;
                   }"#,
                true,
            ),
            (
                "extern opaque? maybe(); int main() { if (maybe() == none) { return 1; } return 0; }",
                true,
            ),
            (
                "extern opaque make(); int main() { let h = make(); return h + h; }",
                false,
            ),
            (
                "extern opaque make(); int main() { let h = make(); return h as int; }",
                false,
            ),
            (
                "extern opaque make(); int main() { return 5 as opaque; }",
                false,
            ),
            (
                "extern opaque make(); int main() { let x = make()[0]; return 0; }",
                false,
            ),
            (
                "extern opaque make(); int main() { let x = make().field; return 0; }",
                false,
            ),
            (
                "extern opaque make(); int main() { let c = copy(make()); return 0; }",
                false,
            ),
            (
                "extern opaque make(); int main() { if (make() < make()) { return 1; } return 0; }",
                false,
            ),
            ("int main() { let h: opaque = none; return 0; }", false),
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
    fn test_expression_types_are_recorded() {
        use crate::parser::{Expression, Statement, Type};

        let source = r#"
            void f() { }
            int main() { let x: real = 1.5 + 2.5; f(); return 0; }
        "#;
        let (symbols, program) = analyze(source);
        let module = &program.modules[0].module;
        let mut checker = TypeChecker::new(
            &symbols,
            program.modules.iter().map(|m| &m.module).collect(),
        );
        checker.visit_module(module);
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
        assert!(program_type_checks(program));
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
        assert!(!program_type_checks(program));
    }

    #[test]
    fn test_module_qualified_call_argument_type_is_checked() {
        let program = load_program(
            "main.kora",
            vec![
                (
                    "main.kora",
                    r#"import "util.kora"; int main() { return util.twice(1.0); }"#,
                ),
                ("util.kora", "int twice(n: int) { return n + n; }"),
            ],
        );
        assert!(!program_type_checks(program));
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
    fn test_int_division_yields_int() {
        check_cases(&[
            (r#"int main() { let x: int = 7 / 2; return 0; }"#, true),
            (r#"int main() { let x: real = 7 / 2; return 0; }"#, false),
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
    fn test_cast_rules() {
        check_cases(&[
            (r#"int main() { let x: real = 1 as real; return 0; }"#, true),
            (r#"int main() { let x: int = 1.5 as int; return x; }"#, true),
            (r#"int main() { let x: int = 'a' as int; return x; }"#, true),
            (
                r#"int main() { let x: char = 65 as char; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let x: char = 1.5 as char; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let x: real = 'a' as real; return 0; }"#,
                true,
            ),
            (
                r#"int main() { let x: int = true as int; return x; }"#,
                false,
            ),
            (r#"int main() { let x: int = 5 as int; return x; }"#, false),
        ]);
    }
}
