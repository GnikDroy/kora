use super::JavascriptTranspiler;
use super::error::TranspilerErr;
use super::mangle::mangle;
use crate::parser::*;
use crate::semantic_analyzer::ArrayMethod;

const EQUALITY_INTRINSIC: &str = "\
function __kora_runtime_equality_intrinsic(a, b) {
    if (a === b) return true;
    if (!Array.isArray(a) || !Array.isArray(b)) return a === b;
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
        if (!__kora_runtime_equality_intrinsic(a[i], b[i])) return false;
    }
    return true;
}
";

impl JavascriptTranspiler {
    #[rustfmt::skip]
    fn repr_unary_operator(op: &UnaryOp) -> &'static str {
        use UnaryOp::*;
        match op {
            Negate => "-",
            Not    => "!",
        }
    }

    #[rustfmt::skip]
    fn repr_binary_operator(op: &BinaryOp) -> &'static str {
        use BinaryOp::*;
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
            BitAnd       => "&",
            BitOr        => "|",
            BitXor       => "^",
            ShiftLeft    => "<<",
            ShiftRight   => ">>",
            Cast         => panic!(),
        }
    }

    fn operand(&mut self, e: &Spanned<Expression>) {
        if matches!(e.node, Expression::Binary(..) | Expression::Unary(..)) {
            self.source.push('(');
            self.visit_expression(e);
            self.source.push(')');
        } else {
            self.visit_expression(e);
        }
    }
}

impl ASTVisitor for JavascriptTranspiler {
    fn visit_module(&mut self, module: &Module) {
        walk_module(self, module);
        self.source.push('\n');
        self.source.push_str(EQUALITY_INTRINSIC);
    }

    fn visit_extern_function(&mut self, _: &Spanned<ExternFunction>) {}

    fn visit_struct(&mut self, _: &Spanned<Struct>) {}

    fn visit_impl(&mut self, impl_: &Spanned<Impl>) {
        self.current_impl = Some(impl_.node.struct_name.node.clone());
        walk_impl(self, impl_);
        self.current_impl = None;
    }

    fn visit_function(&mut self, func: &Spanned<Function>) {
        let name = match &self.current_impl {
            Some(struct_name) => mangle(struct_name, &func.node.name),
            None => func.node.name.clone(),
        };
        let arg_list: String = func
            .node
            .arguments
            .iter()
            .map(|arg| arg.node.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let async_prefix = if self.async_fns.contains(&name) {
            "async "
        } else {
            ""
        };
        self.source
            .push_str(&format!("{}function {}({})", async_prefix, name, arg_list));

        match &func.node.statement.node {
            Statement::Compound(_) => {
                self.visit_statement(&func.node.statement);
            }
            _ => {
                self.source.push('{');
                self.visit_statement(&func.node.statement);
                self.source.push('}');
            }
        }
    }

    fn visit_statement(&mut self, stmt: &Spanned<Statement>) {
        walk_statement(self, stmt);
        let needs_semicolon = matches!(
            &stmt.node,
            Statement::Simple(_)
                | Statement::Return(_)
                | Statement::Let(_, _, _)
                | Statement::Break
                | Statement::Continue
        );
        if needs_semicolon {
            self.source.push(';');
        }
    }

    fn visit_let_statement(
        &mut self,
        name: &Spanned<String>,
        _typename: Option<&Type>,
        expr: &Spanned<Expression>,
    ) {
        self.source.push_str(&format!("let {} = ", name.node));
        self.visit_expression(expr);
    }

    fn visit_return_statement(&mut self, expr: Option<&Spanned<Expression>>, _span: &Span) {
        self.source.push_str("return");
        if let Some(expr) = expr {
            self.source.push(' ');
            self.visit_expression(expr);
        }
    }

    fn visit_compound_statement(&mut self, stmts: &[Spanned<Statement>]) {
        self.source.push('{');
        walk_compound_statement(self, stmts);
        self.source.push('}');
    }

    fn visit_while_statement(&mut self, cond: &Spanned<Expression>, stmt: &Spanned<Statement>) {
        self.source.push_str("while (");
        self.visit_expression(cond);
        self.source.push(')');
        self.visit_statement(stmt);
    }

    fn visit_for_statement(
        &mut self,
        init: &Spanned<Statement>,
        cond: &Spanned<Expression>,
        step: &Spanned<Expression>,
        body: &Spanned<Statement>,
    ) {
        self.source.push_str("for (");
        walk_statement(self, init);
        self.source.push(';');
        self.visit_expression(cond);
        self.source.push(';');
        self.visit_expression(step);
        self.source.push(')');
        self.visit_statement(body);
    }

    fn visit_break_statement(&mut self, _span: &Span) {
        self.source.push_str("break");
    }

    fn visit_continue_statement(&mut self, _span: &Span) {
        self.source.push_str("continue");
    }

    fn visit_if_statement(
        &mut self,
        cond: &Spanned<Expression>,
        if_case: &Spanned<Statement>,
        else_case: Option<&Spanned<Statement>>,
    ) {
        self.source.push_str("if (");
        self.visit_expression(cond);
        self.source.push(')');
        self.visit_statement(if_case);
        if let Some(stmt) = else_case {
            self.source.push_str("else ");
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
        self.source.push_str(if *b { "true" } else { "false" });
    }

    fn visit_char_literal(&mut self, c: &u8) {
        let c = *c as char;
        self.source.push('\'');
        match c {
            '\\' => self.source.push_str("\\\\"),
            '\'' => self.source.push_str("\\'"),
            '\n' => self.source.push_str("\\n"),
            '\r' => self.source.push_str("\\r"),
            '\t' => self.source.push_str("\\t"),
            '\0' => self.source.push_str("\\0"),
            _ => self.source.push(c),
        }
        self.source.push('\'');
    }

    fn visit_string_literal(&mut self, s: &String) {
        // Kora's [char] is a mutable char array; a bare JS string would
        // silently ignore `s[i] = 'x'`.
        self.source.push_str("Array.from(");
        self.source.push('"');
        for c in s.chars() {
            match c {
                '\\' => self.source.push_str("\\\\"),
                '"' => self.source.push_str("\\\""),
                '\n' => self.source.push_str("\\n"),
                '\r' => self.source.push_str("\\r"),
                '\t' => self.source.push_str("\\t"),
                '\0' => self.source.push_str("\\0"),
                _ => self.source.push(c),
            }
        }
        self.source.push('"');
        self.source.push(')');
    }

    fn visit_identifier(&mut self, s: &String) {
        self.source.push_str(s.as_str());
    }

    fn visit_array(&mut self, exprs: &[Spanned<Expression>]) {
        self.source.push('[');
        for (i, expr) in exprs.iter().enumerate() {
            if i > 0 {
                self.source.push(',');
            }
            self.visit_expression(expr);
        }
        self.source.push(']');
    }

    fn visit_binary_expression(
        &mut self,
        left: &Spanned<Expression>,
        op: &BinaryOp,
        right: &Spanned<Expression>,
    ) {
        if matches!(op, BinaryOp::Equality | BinaryOp::NotEquality)
            && matches!(self.types.get(&left.id), Some(Type::Array(_)))
        {
            if matches!(op, BinaryOp::NotEquality) {
                self.source.push('!');
            }
            self.source.push_str("__kora_runtime_equality_intrinsic(");
            self.visit_expression(left);
            self.source.push(',');
            self.visit_expression(right);
            self.source.push(')');
            return;
        }

        // `+` on arrays is pure concatenation. JS `+` would string-coerce,
        // so emit the (pure) `.concat`.
        if matches!(op, BinaryOp::Add) && matches!(self.types.get(&left.id), Some(Type::Array(_))) {
            self.operand(left);
            self.source.push_str(".concat(");
            self.visit_expression(right);
            self.source.push(')');
            return;
        }

        // 64-bit-correct bitwise: compute in BigInt, then wrap back to i64.
        use BinaryOp::{BitAnd, BitOr, BitXor, ShiftLeft, ShiftRight};
        if matches!(op, BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight) {
            self.source.push_str("Number(BigInt.asIntN(64,BigInt(");
            self.visit_expression(left);
            self.source.push(')');
            self.source
                .push_str(JavascriptTranspiler::repr_binary_operator(op));
            self.source.push_str("BigInt(");
            self.visit_expression(right);
            self.source.push_str(")))");
            return;
        }

        // JS `/` is float division; Kora `int / int` truncates toward zero.
        let int_div =
            matches!(op, BinaryOp::Divide) && self.types.get(&left.id) == Some(&Type::Int);
        if int_div {
            self.source.push_str("Math.trunc(");
        }
        self.operand(left);
        self.source
            .push_str(JavascriptTranspiler::repr_binary_operator(op));
        self.operand(right);
        if int_div {
            self.source.push(')');
        }
    }

    fn visit_unary_expression(&mut self, op: &UnaryOp, expr: &Spanned<Expression>) {
        self.source
            .push_str(JavascriptTranspiler::repr_unary_operator(op));
        self.operand(expr);
    }

    fn visit_call_expression(&mut self, expr: &Spanned<Expression>, exprs: &[Spanned<Expression>]) {
        // `copy(x)` — shallow copy of an aggregate, by argument type.
        if matches!(&expr.node, Expression::Identifier(name) if name == "copy") {
            let arg = &exprs[0];
            let (before, after) = match self.types.get(&arg.id) {
                Some(Type::Struct(_)) => ("({...", "})"),
                _ => ("Array.from(", ")"),
            };
            self.source.push_str(before);
            self.visit_expression(arg);
            self.source.push_str(after);
            return;
        }

        if let Some(method) = self.array_method_calls.get(&expr.id).copied() {
            let Expression::Access(obj, _) = &expr.node else {
                unreachable!("array method calls are calls on an access expression");
            };
            self.operand(obj);
            match method {
                ArrayMethod::Len => self.source.push_str(".length"),
                ArrayMethod::Push => {
                    self.source.push_str(".push(");
                    self.visit_expression(&exprs[0]);
                    self.source.push(')');
                }
                ArrayMethod::Pop => self.source.push_str(".pop()"),
                ArrayMethod::Insert => {
                    self.source.push_str(".splice(");
                    self.visit_expression(&exprs[0]);
                    self.source.push_str(",0,");
                    self.visit_expression(&exprs[1]);
                    self.source.push(')');
                }
                ArrayMethod::Remove => {
                    self.source.push_str(".splice(");
                    self.visit_expression(&exprs[0]);
                    self.source.push_str(",1)[0]");
                }
                ArrayMethod::Slice => {
                    self.source.push_str(".slice(");
                    self.visit_expression(&exprs[0]);
                    self.source.push(',');
                    self.visit_expression(&exprs[1]);
                    self.source.push(')');
                }
                // Mutating append-many; JS `.concat` is pure, so spread-push.
                ArrayMethod::Extend => {
                    self.source.push_str(".push(...");
                    self.visit_expression(&exprs[0]);
                    self.source.push(')');
                }
            }
            return;
        }

        // `p.age(x)` becomes `Person$age(p, x)`; the object is self.
        if let Some(name) = self.method_calls.get(&expr.id) {
            let name = name.clone();
            let Expression::Access(obj, _) = &expr.node else {
                unreachable!("method calls are calls on an access expression");
            };
            let is_async = self.async_fns.contains(&name);
            if is_async {
                self.source.push_str("(await ");
            }
            self.source.push_str(&name);
            self.source.push('(');
            self.visit_expression(obj);
            for expr in exprs.iter() {
                self.source.push(',');
                self.visit_expression(expr);
            }
            self.source.push(')');
            if is_async {
                self.source.push(')');
            }
            return;
        }

        // Only await calls into functions we know are async.
        let is_async = match &expr.node {
            Expression::Identifier(name) => self.async_fns.contains(name),
            _ => false,
        };
        if is_async {
            self.source.push_str("(await ");
        }
        self.visit_expression(expr);
        self.source.push('(');
        for (i, expr) in exprs.iter().enumerate() {
            if i > 0 {
                self.source.push(',');
            }
            self.visit_expression(expr);
        }
        self.source.push(')');
        if is_async {
            self.source.push(')');
        }
    }

    fn visit_array_index_expression(
        &mut self,
        left: &Spanned<Expression>,
        right: &Spanned<Expression>,
    ) {
        self.operand(left);
        self.source.push('[');
        self.visit_expression(right);
        self.source.push(']');
    }

    fn visit_cast_expression(&mut self, expr: &Spanned<Expression>, typename: &Type) {
        use Type::*;
        let (before, after) = match (self.types.get(&expr.id), typename) {
            (Some(Real), Int) => ("Math.trunc(", ")"),
            (Some(Char), Int | Real) => ("(", ").charCodeAt(0)"),
            (Some(Int), Real) => ("(", ")"),
            (Some(Int), Char) => ("String.fromCharCode(", ")"),
            (Some(Real), Char) => ("String.fromCharCode(Math.trunc(", "))"),
            _ => {
                self.errors.push(TranspilerErr {
                    msg: "Casting to type not supported",
                });
                return;
            }
        };
        self.source.push_str(before);
        self.visit_expression(expr);
        self.source.push_str(after);
    }

    fn visit_construct_expression(
        &mut self,
        typename: &Type,
        size: &Option<Box<Spanned<Expression>>>,
    ) {
        match size {
            // `new T[n]` is scalar-only (the checker rejects reference T), so
            // the element always has a zero fill.
            Some(expr) => {
                let zero = match typename {
                    Type::Real => "0.0",
                    Type::Bool => "false",
                    Type::Char => "\"\\0\"",
                    _ => "0",
                };
                self.source.push_str("new Array(");
                self.visit_expression(expr);
                self.source.push_str(&format!(").fill({})", zero));
            }
            None => {
                self.source.push_str("({})");
            }
        }
    }

    fn visit_access_expression(&mut self, left: &Spanned<Expression>, member: &str) {
        self.operand(left);
        self.source.push('.');
        self.source.push_str(member);
    }

    fn visit_struct_literal(
        &mut self,
        _typename: &Type,
        fields: &[(Spanned<String>, Spanned<Expression>)],
    ) {
        self.source.push_str("({");
        for (i, (name, value)) in fields.iter().enumerate() {
            if i > 0 {
                self.source.push(',');
            }
            self.source.push_str(&name.node);
            self.source.push(':');
            self.visit_expression(value);
        }
        self.source.push_str("})");
    }
}
