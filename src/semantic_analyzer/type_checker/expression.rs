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
        let left_type = self.get_expression_type(left)?;
        let right_type = if matches!(op, BinaryOp::Assign) {
            self.get_expression_type_expecting(right, &left_type)?
        } else {
            self.get_expression_type(right)?
        };

        if matches!(op, BinaryOp::Assign) {
            if !self.is_assignable(left) || matches!(left_type, Type::Function(_, _)) {
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
                } else if !builtins::is_scalar(typename) {
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
        matches!(expr.node, Identifier(_) | ArrayIndex(_, _) | Access(_, _))
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
