mod error;

use crate::parser::*;

use self::error::TranspilerErr;

#[derive(Default, Debug)]
pub struct JsTranspiler {
    source: String,
    errors: Vec<TranspilerErr>,
}

impl JsTranspiler {
    pub fn new() -> JsTranspiler {
        JsTranspiler {
            ..Default::default()
        }
    }

    pub fn get_source(&self) -> Result<&str, &[TranspilerErr]> {
        if self.errors.is_empty() {
            Ok(&self.source)
        } else {
            Err(&self.errors)
        }
    }

    fn repr_unary_operator(op: &UnaryOp) -> &'static str {
        use UnaryOp::*;
        #[rustfmt::skip]
        match op {
            Negate => "-",
            Not    => "!",
            New    => "new",
        }
    }

    fn repr_binary_operator(op: &BinaryOp) -> &'static str {
        use BinaryOp::*;
        #[rustfmt::skip]
        match op {
            Assign       => "=",
            Add          => "+",
            Subtract     => "-",
            Multiply     => "*",
            Divide       => "/",
            Modulo       => "%",
            Equality     => "===",
            NotEquality  => "!==",
            And          => "&&",
            Or           => "||",
            Greater      => ">",
            GreaterEqual => ">=",
            Less         => "<",
            LessEqual    => "<=",
            Cast         => panic!(),
        }
    }
}
impl ASTVisitor for JsTranspiler {
    fn visit_extern_function(&mut self, _: &ExternFunction) {}

    fn visit_struct(&mut self, _: &Struct) {}

    fn visit_function(&mut self, func: &Function) {
        let arg_list: String = func
            .arguments
            .iter()
            .map(|arg| &arg.name)
            .cloned()
            .intersperse(", ".to_owned())
            .collect();
        self.source
            .push_str(&format!("async function {}({})", &func.name, &arg_list));

        match &func.statement {
            Statement::Compound(_) => {
                self.visit_statement(&func.statement);
            }
            _ => {
                self.source.push('{');
                self.visit_statement(&func.statement);
                self.source.push('}');
            }
        }
    }

    fn visit_statement(&mut self, stmt: &Statement) {
        walk_statement(self, stmt);
        self.source.push(';');
    }

    fn visit_let_statement(&mut self, pair: &IdentifierTypePair, expr: &Expression) {
        self.source.push_str(&format!("let {} = ", pair.name));
        self.visit_expression(expr);
    }

    fn visit_return_statement(&mut self, expr: &Expression) {
        self.source.push_str("return ");
        walk_return_statement(self, expr);
    }

    fn visit_compound_statement(&mut self, stmts: &[Statement]) {
        self.source.push('{');
        walk_compound_statement(self, stmts);
        self.source.push('}');
    }

    fn visit_while_statement(&mut self, cond: &Expression, stmt: &Statement) {
        self.source.push_str("while (");
        self.visit_expression(cond);
        self.source.push(')');
        self.visit_statement(stmt);
    }

    fn visit_if_statement(
        &mut self,
        cond: &Expression,
        if_case: &Statement,
        else_case: Option<&Statement>,
    ) {
        self.source.push_str("if (");
        self.visit_expression(cond);
        self.source.push(')');
        self.visit_statement(if_case);
        if let Some(stmt) = else_case {
            self.source.push_str("else");
            self.visit_statement(stmt);
        }
    }

    fn visit_integer_literal(&mut self, num: &isize) {
        self.source.push_str(&num.to_string());
    }

    fn visit_real_literal(&mut self, num: &f64) {
        self.source.push_str(&num.to_string());
    }

    fn visit_boolean_literal(&mut self, b: &bool) {
        self.source.push_str(&format!("{}", b));
    }

    fn visit_char_literal(&mut self, c: &u8) {
        let c = *c as char;
        match c {
            '\\' | '\'' => {
                self.source.push_str(&format!("'\\{}'", c));
            }
            _ => self.source.push_str(&format!("'{}'", c)),
        }
    }

    fn visit_string_literal(&mut self, s: &String) {
        self.source.push_str(&format!("\"{}\"", s));
    }

    fn visit_identifier(&mut self, s: &String) {
        if s == "input_int" {
            self.source.push_str("await input_int")
        } else {
            self.source.push_str(s.as_str());
        }
    }

    fn visit_array(&mut self, exprs: &[Expression]) {
        self.source.push('[');
        for expr in exprs.iter() {
            self.source.push('(');
            self.visit_expression(expr);
            self.source.push(')');
            self.source.push(',');
        }
        self.source.push(']');
    }

    fn visit_binary_expression(&mut self, left: &Expression, op: &BinaryOp, right: &Expression) {
        self.source.push('(');
        self.visit_expression(left);
        self.source.push(')');
        if !matches!(op, BinaryOp::Cast) {
            self.source.push_str(JsTranspiler::repr_binary_operator(op));
        } else {
        }
        self.source.push('(');
        self.visit_expression(right);
        self.source.push(')');
    }

    fn visit_unary_expression(&mut self, op: &UnaryOp, expr: &Expression) {
        self.source.push_str(JsTranspiler::repr_unary_operator(op));
        self.source.push('(');
        self.visit_expression(expr);
        self.source.push(')');
    }

    fn visit_call_expression(&mut self, expr: &Expression, exprs: &[Expression]) {
        self.source.push('(');
        self.source.push_str("await");
        self.visit_expression(expr);
        self.source.push('(');
        for expr in exprs.iter().map(Some).intersperse(None) {
            match expr {
                Some(expr) => {
                    self.visit_expression(expr);
                }
                None => {
                    self.source.push(',');
                }
            }
        }
        self.source.push(')');
        self.source.push(')');
    }

    fn visit_array_index_expression(&mut self, left: &Expression, right: &Expression) {
        self.source.push('(');
        self.visit_expression(left);
        self.source.push('[');
        self.visit_expression(right);
        self.source.push(']');
        self.source.push(')');
    }

    fn visit_cast_expression(&mut self, expr: &Expression, typename: &Type) {
        match typename {
            Type::Bool => {
                self.source.push_str("Boolean(");
                self.visit_expression(expr);
                self.source.push(')');
            }
            Type::Array(typename) if **typename == Type::Char => {
                self.source.push_str("String(");
                self.visit_expression(expr);
                self.source.push(')');
            }
            Type::Int | Type::Real => {
                self.source.push_str("Number(");
                self.visit_expression(expr);
                self.source.push(')');
            }
            _ => self.errors.push(TranspilerErr {
                msg: "Casting to type not supported",
            }),
        }
    }

    fn visit_construct_expression(&mut self, typename: &Type, size: &Option<Box<Expression>>) {
        match size {
            Some(expr) => match typename {
                Type::Struct(_) => {
                    self.source.push_str("new Array(");
                    self.visit_expression(expr);
                    self.source.push_str(").fill({})");
                }
                _ => {
                    self.source.push_str("new Array(");
                    self.visit_expression(expr);
                    self.source.push_str(");");
                }
            },
            None => {}
        }
    }

    fn visit_access_expression(&mut self, left: &Expression, right: &Expression) {
        self.source.push('(');
        self.visit_expression(left);
        self.source.push('.');
        self.visit_expression(right);
        self.source.push(')');
    }
}

mod tests {
    #[test]
    fn valid() {
        use crate::{
            js_transpiler::JsTranspiler,
            lexer,
            parser::{self, ASTVisitor},
            semantic_analyzer::{SymbolTable, TypeChecker, UnidentifiedIdentifierChecker},
        };

        let source = r#"
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

        let mut checker = UnidentifiedIdentifierChecker::new(symbol_table.clone());
        checker.visit_module(&module);
        assert_eq!(checker.check().is_ok(), true);

        let mut checker = TypeChecker::new(symbol_table.clone());
        checker.visit_module(&module);
        assert_eq!(checker.check().is_ok(), true);

        let mut transpiler = JsTranspiler::new();
        transpiler.visit_module(&module);
        if let Ok(source) = transpiler.get_source() {
            println!("{}", source);
        }
    }
}
