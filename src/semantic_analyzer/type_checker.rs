use super::{errors::TypeErr, symbol_table::*};
use crate::parser::*;

#[derive(Default)]
pub struct TypeChecker {
    global_symbols: SymbolTable,
    current_symbols: SymbolTable,
    current_function_name: String,
    errors: Vec<TypeErr>,
}

impl TypeChecker {
    pub fn new(mut symbols: SymbolTable) -> TypeChecker {
        symbols.reverse();
        TypeChecker {
            global_symbols: symbols,
            ..Default::default()
        }
    }

    pub fn check(&self) -> Result<(), &[TypeErr]> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(&self.errors)
        }
    }

    fn get_array_type(&self, exprs: &[Expression]) -> Result<Type, TypeErr> {
        let types = exprs
            .iter()
            .map(|e| self.get_expression_type(e))
            .collect::<Result<Vec<Type>, TypeErr>>()?;

        if let Some(first) = types.first() {
            if types.iter().all(|x| x == first) {
                Ok(first.clone())
            } else {
                Err(TypeErr {
                    msg: "Array doesn't consist of homogeneous types.",
                })
            }
        } else {
            Err(TypeErr {
               msg: "Cannot infer type of empty array. An empty static array makes no sense either way."
            })
        }
    }

    fn get_binary_expression_type(
        &self,
        left: &Expression,
        op: &BinaryOp,
        right: &Expression,
    ) -> Result<Type, TypeErr> {
        use BinaryOp::*;
        use Type::*;

        let left_type = self.get_expression_type(left)?;
        let right_type = self.get_expression_type(right)?;

        #[rustfmt::skip]
        match (left_type, op, right_type) {
            (Int, Assign, Int)         => Ok(Int),
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

            (Real, Assign, Real)       => Ok(Int),
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

            (Bool, Assign, Bool)       => Ok(Int),
            (Bool, Equality, Bool)     => Ok(Bool),
            (Bool, NotEquality, Bool)  => Ok(Bool),
            (Bool, And, Bool)          => Ok(Bool),
            (Bool, Or, Bool)           => Ok(Bool),

            (Char, Assign, Char)       => Ok(Int),
            (Char, Equality, Char)     => Ok(Bool),
            (Char, NotEquality, Char)  => Ok(Bool),
            (Char, Greater, Char)      => Ok(Bool),
            (Char, Less, Char)         => Ok(Bool),
            (Char, GreaterEqual, Char) => Ok(Bool),
            (Char, LessEqual, Char)    => Ok(Bool),
            _ => Err(TypeErr {
                msg: "Binary operator cannot be applied to the types",
            }),
        }
    }

    fn get_unary_expression_type(&self, op: &UnaryOp, expr: &Expression) -> Result<Type, TypeErr> {
        use Type::*;
        use UnaryOp::*;

        let typename = self.get_expression_type(expr)?;

        #[rustfmt::skip]
        match (op, typename) {
            (Negate, Int)  => Ok(Int),
            (Negate, Real) => Ok(Int),

            (Not, Bool)    => Ok(Bool),
            _ => Err(TypeErr {
                msg: "Unary operator cannot be applied to the types"
            })
        }
    }

    fn get_call_expression_type(
        &self,
        f: &Expression,
        args: &Vec<Expression>,
    ) -> Result<Type, TypeErr> {
        match self.get_expression_type(f)? {
            Type::Function(ret_type, args_types) => {
                if args.len() != args_types.len() {
                    return Err(TypeErr {
                        msg: "Function has different number of arguments",
                    });
                }

                for (arg, arg_type) in args.iter().zip(args_types) {
                    if self.get_expression_type(arg)? != arg_type {
                        return Err(TypeErr {
                            msg: "Arguments passed to function do not match type signature for function",
                        });
                    }
                }
                Ok(*ret_type)
            }
            _ => Err(TypeErr {
                msg: "Call expression must have function type",
            }),
        }
    }

    fn get_array_index_expression_type(
        &self,
        left: &Expression,
        right: &Expression,
    ) -> Result<Type, TypeErr> {
        let left_type = self.get_expression_type(left)?;
        let right_type = self.get_expression_type(right)?;
        match (left_type, right_type) {
            (Type::Array(item_type), Type::Int) => Ok(*item_type),
            _ => Err(TypeErr {
                msg: "Array index expression must have array type on the left, and integer on the right",
            }),
        }
    }

    fn get_access_expression_type(
        &self,
        left: &Expression,
        right: &Expression,
    ) -> Result<Type, TypeErr> {
        let left_type = self.get_expression_type(left)?;
        if  let Type::Struct(name) = left_type 
            && let Expression::Identifier(member) = right {
            self.current_symbols.resolve_struct_member(&name, &member).ok_or(TypeErr{
                msg: "Invalid member for struct"
            })
        } else {
            Err(TypeErr {
                msg: "Access operator must have struct type on left and identifier on the right",
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
        expr: &Expression,
        typename: &Type,
    ) -> Result<Type, TypeErr> {
        let expr_type = self.get_expression_type(expr)?;
        if TypeChecker::is_cast_possible(&expr_type, typename) {
            Ok(typename.clone())
        } else {
            Err(TypeErr {
                msg: "Cannot cast to type",
            })
        }
    }

    fn get_construct_expression_type(
        &self,
        typename: &Type,
        size: &Option<Box<Expression>>,
    ) -> Result<Type, TypeErr> {
        match size {
            Some(size) => {
                let expr_type = self.get_expression_type(&size)?;
                if expr_type != Type::Int {
                    Err(TypeErr {
                        msg: "Must be int",
                    })
                } else {
                    Ok(Type::Array(Box::new(typename.clone())))
                }
            }
            _ => Ok(typename.clone())
        }
    }

    fn get_expression_type(&self, expr: &Expression) -> Result<Type, TypeErr> {
        use Expression::*;
        match expr {
            IntegerLiteral(_) => Ok(Type::Int),
            RealLiteral(_) => Ok(Type::Real),
            CharLiteral(_) => Ok(Type::Char),
            StringLiteral(_) => Ok(Type::Array(Box::new(Type::Char))),
            BoolLiteral(_) => Ok(Type::Bool),
            Array(exprs) => self.get_array_type(exprs),
            Identifier(name) => Ok(self.current_symbols.resolve(name).unwrap()),
            Binary(left, op, right) => self.get_binary_expression_type(left, op, right),
            Unary(op, term) => self.get_unary_expression_type(op, term),
            Call(f, args) => self.get_call_expression_type(f, args),
            Cast(expr, typename) => self.get_cast_expression_type(expr, typename),
            ArrayIndex(left, right) => self.get_array_index_expression_type(left, right),
            Access(left, right) => self.get_access_expression_type(left, right),
            Construct(typename, size) => self.get_construct_expression_type(typename, size)
        }
    }

    fn ensure_type(&mut self, expr: &Expression, expected: &Type) {
        let expr_type = self.get_expression_type(expr);
        match expr_type {
            Ok(typename) if typename == *expected => {}
            Err(type_err) => self.errors.push(type_err),
            _ => self.errors.push(TypeErr {
                msg: "Types don't match",
            }),
        };
    }
}

impl ASTVisitor for TypeChecker {
    fn visit_enter_scope(&mut self) {
        let scope = self.global_symbols.pop_scope().unwrap();
        self.current_symbols.add_scope(scope);
    }

    fn visit_exit_scope(&mut self) {
        self.current_symbols.pop_scope();
    }

    fn visit_function(&mut self, func: &Function) {
        self.current_function_name = func.name.clone();
        walk_function(self, func);
    }

    fn visit_let_statement(&mut self, pair: &IdentifierTypePair, expr: &Expression) {
        self.ensure_type(expr, &pair.typename);
        walk_let_statement(self, pair, expr);
    }

    fn visit_if_statement(
        &mut self,
        cond: &Expression,
        if_case: &Statement,
        else_case: Option<&Statement>,
    ) {
        self.ensure_type(cond, &Type::Bool);
        walk_if_statement(self, cond, if_case, else_case);
    }

    fn visit_while_statement(&mut self, cond: &Expression, stmt: &Statement) {
        self.ensure_type(cond, &Type::Bool);
        walk_while_statement(self, cond, stmt);
    }

    fn visit_simple_statement(&mut self, expr: &Expression) {
        if let Err(e) = self.get_expression_type(expr) {
            self.errors.push(e);
        }
        walk_simple_statement(self, expr);
    }

    fn visit_return_statement(&mut self, expr: &Expression) {
        let func_type = self
            .current_symbols
            .resolve(&self.current_function_name)
            .unwrap();
        if let Type::Function(ret_type, _) = func_type {
            self.ensure_type(expr, &ret_type);
        }
        walk_return_statement(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer,
        parser::{self, ASTVisitor},
        semantic_analyzer::symbol_table::SymbolTable,
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
        let mut symbol_table = SymbolTable::new();
        symbol_table.visit_module(&module);

        let mut checker = TypeChecker::new(symbol_table.clone());
        checker.visit_module(&module);
        assert_eq!(
            checker.check().is_ok(),
            true,
            "source_text: {}, symbol_table: {:#?} errors: {:?}",
            source,
            symbol_table,
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
        let mut symbol_table = SymbolTable::new();
        symbol_table.visit_module(&module);

        let mut checker = TypeChecker::new(symbol_table.clone());
        checker.visit_module(&module);
        assert_eq!(
            checker.check().is_err() && checker.check().unwrap_err().len() == 3,
            true,
            "source_text: {}, symbol_table: {:#?} errors: {:?}",
            source,
            symbol_table,
            checker.check().unwrap()
        );
    }
}
