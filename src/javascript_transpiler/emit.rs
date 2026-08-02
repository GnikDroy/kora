use super::JavascriptTranspiler;
use super::error::TranspilerErr;
use crate::parser::*;
use crate::semantic_analyzer::ArrayMethod;

const INTRINSICS: &str = "\
function __kora_panic(message) {
    throw new Error(message);
}

function __kora_missing_extern(name) {
    return () => __kora_panic(\"extern '\" + name + \"' is not provided by this host\");
}

function __kora_runtime_equality_intrinsic(a, b) {
    if (a === b) return true;
    if (a == null || b == null) return a == b;
    if (!Array.isArray(a) || !Array.isArray(b)) return a === b;
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
        if (!__kora_runtime_equality_intrinsic(a[i], b[i])) return false;
    }
    return true;
}

function __kora_runtime_index(a, i) {
    if (i < 0 || i >= a.length) __kora_panic(\"index out of bounds\");
    return a[i];
}

function __kora_runtime_index_set(a, i, v) {
    if (i < 0 || i >= a.length) __kora_panic(\"index out of bounds\");
    return a[i] = v;
}

function __kora_runtime_unwrap(x) {
    if (x === null || x === undefined) __kora_panic(\"force-unwrapped a none value\");
    return x;
}

function __kora_runtime_div(a, b) {
    if (b === 0) __kora_panic(\"division by zero\");
    return Math.trunc(a / b);
}

function __kora_runtime_mod(a, b) {
    if (b === 0) __kora_panic(\"division by zero\");
    return a % b;
}

function __kora_runtime_pop(a) {
    if (a.length === 0) __kora_panic(\"pop from empty array\");
    return a.pop();
}

function __kora_runtime_insert(a, i, v) {
    if (i < 0 || i > a.length) __kora_panic(\"index out of bounds\");
    a.splice(i, 0, v);
}

function __kora_runtime_remove(a, i) {
    if (i < 0 || i >= a.length) __kora_panic(\"index out of bounds\");
    return a.splice(i, 1)[0];
}

function __kora_runtime_check_len(n) {
    if (n < 0) __kora_panic(\"negative array length\");
    return n;
}
";

fn compares_structurally(t: Option<&Type>) -> bool {
    match t {
        Some(Type::Array(_)) => true,
        Some(Type::Optional(inner)) => matches!(**inner, Type::Array(_)),
        _ => false,
    }
}

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

    /// NOTE: Type checker rejects struct cycles, so we terminate.
    fn emit_default(&mut self, ty: &Type) {
        match ty {
            Type::Struct(sr) => self.emit_struct_zero(sr.target),
            Type::Array(_) => self.source.push_str("[]"),
            Type::Optional(_) | Type::Opaque => self.source.push_str("null"),
            Type::Real => self.source.push_str("0.0"),
            Type::Bool => self.source.push_str("false"),
            Type::Char => self.source.push_str("\"\\0\""),
            _ => self.source.push('0'),
        }
    }

    fn emit_struct_zero(&mut self, decl: Option<NodeId>) {
        let members = decl
            .and_then(|d| self.struct_members.get(&d).cloned())
            .unwrap_or_default();
        self.source.push_str("({");
        for (i, (field, ty)) in members.iter().enumerate() {
            if i > 0 {
                self.source.push(',');
            }
            self.source.push_str(field);
            self.source.push(':');
            self.emit_default(ty);
        }
        self.source.push_str("})");
    }

    pub fn emit_program(&mut self, modules: &[&Module]) {
        for module in modules {
            walk_module(self, module);
        }
        self.source.push('\n');
        self.source.push_str(INTRINSICS);
    }
}

impl ASTVisitor for JavascriptTranspiler {
    fn visit_module(&mut self, module: &Module) {
        self.emit_program(&[module]);
    }

    fn visit_extern_function(&mut self, func: &Spanned<ExternFunction>) {
        let name = &self.emitted[&func.id];
        self.source.push_str(&format!(
            "var {name} = typeof {name} === \"function\" ? {name} : __kora_missing_extern(\"{name}\");"
        ));
    }

    fn visit_struct(&mut self, _: &Spanned<Struct>) {}

    fn visit_function(&mut self, func: &Spanned<Function>) {
        let name = self.emitted[&func.id].clone();
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

    fn visit_none_literal(&mut self) {
        self.source.push_str("null");
    }

    fn visit_unwrap_expression(&mut self, expr: &Spanned<Expression>) {
        self.source.push_str("__kora_runtime_unwrap(");
        self.visit_expression(expr);
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
        if matches!(op, BinaryOp::Assign)
            && let Expression::ArrayIndex(array, index) = &left.node
        {
            self.source.push_str("__kora_runtime_index_set(");
            self.visit_expression(array);
            self.source.push(',');
            self.visit_expression(index);
            self.source.push(',');
            self.visit_expression(right);
            self.source.push(')');
            return;
        }

        if matches!(op, BinaryOp::Equality | BinaryOp::NotEquality)
            && !matches!(left.node, Expression::NoneLiteral)
            && !matches!(right.node, Expression::NoneLiteral)
            && (compares_structurally(self.types.get(&left.id))
                || compares_structurally(self.types.get(&right.id)))
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

        // Optional comparison: `x == none`, `x != none`, or two optionals.
        // Loose `==`/`!=` so a `none` (null) matches an uninitialized field
        // (undefined) from a bare `new Struct`.
        if matches!(op, BinaryOp::Equality | BinaryOp::NotEquality)
            && (matches!(left.node, Expression::NoneLiteral)
                || matches!(right.node, Expression::NoneLiteral)
                || matches!(self.types.get(&left.id), Some(Type::Optional(_)))
                || matches!(self.types.get(&right.id), Some(Type::Optional(_))))
        {
            self.operand(left);
            self.source.push_str(if matches!(op, BinaryOp::Equality) {
                "=="
            } else {
                "!="
            });
            self.operand(right);
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

        let int_div =
            matches!(op, BinaryOp::Divide) && self.types.get(&left.id) == Some(&Type::Int);
        if int_div || matches!(op, BinaryOp::Modulo) {
            self.source.push_str(if int_div {
                "__kora_runtime_div("
            } else {
                "__kora_runtime_mod("
            });
            self.visit_expression(left);
            self.source.push(',');
            self.visit_expression(right);
            self.source.push(')');
            return;
        }
        self.operand(left);
        self.source
            .push_str(JavascriptTranspiler::repr_binary_operator(op));
        self.operand(right);
    }

    fn visit_unary_expression(&mut self, op: &UnaryOp, expr: &Spanned<Expression>) {
        self.source
            .push_str(JavascriptTranspiler::repr_unary_operator(op));
        self.operand(expr);
    }

    fn visit_call_expression(&mut self, expr: &Spanned<Expression>, exprs: &[Spanned<Expression>]) {
        // `copy(x)` shallow copy of an aggregate, by argument type.
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
            match method {
                ArrayMethod::Pop => {
                    self.source.push_str("__kora_runtime_pop(");
                    self.visit_expression(obj);
                    self.source.push(')');
                    return;
                }
                ArrayMethod::Insert => {
                    self.source.push_str("__kora_runtime_insert(");
                    self.visit_expression(obj);
                    self.source.push(',');
                    self.visit_expression(&exprs[0]);
                    self.source.push(',');
                    self.visit_expression(&exprs[1]);
                    self.source.push(')');
                    return;
                }
                ArrayMethod::Remove => {
                    self.source.push_str("__kora_runtime_remove(");
                    self.visit_expression(obj);
                    self.source.push(',');
                    self.visit_expression(&exprs[0]);
                    self.source.push(')');
                    return;
                }
                _ => {}
            }
            self.operand(obj);
            match method {
                ArrayMethod::Len => self.source.push_str(".length"),
                ArrayMethod::Push => {
                    self.source.push_str(".push(");
                    self.visit_expression(&exprs[0]);
                    self.source.push(')');
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
                ArrayMethod::Pop | ArrayMethod::Insert | ArrayMethod::Remove => unreachable!(),
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

        if let Some(name) = self.function_call_names.get(&expr.id) {
            let name = name.clone();
            let is_async = self.async_fns.contains(&name);
            if is_async {
                self.source.push_str("(await ");
            }
            self.source.push_str(&name);
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
        self.source.push_str("__kora_runtime_index(");
        self.visit_expression(left);
        self.source.push(',');
        self.visit_expression(right);
        self.source.push(')');
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
            Some(expr) => match typename {
                Type::Struct(sr) => {
                    self.source
                        .push_str("Array.from({length:__kora_runtime_check_len(");
                    self.visit_expression(expr);
                    self.source.push_str(")},()=>");
                    self.emit_struct_zero(sr.target);
                    self.source.push(')');
                }
                _ => {
                    self.source.push_str("new Array(__kora_runtime_check_len(");
                    self.visit_expression(expr);
                    self.source.push_str(")).fill(");
                    self.emit_default(typename);
                    self.source.push(')');
                }
            },
            None => match typename {
                Type::Struct(sr) => self.emit_struct_zero(sr.target),
                _ => self.source.push_str("({})"),
            },
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
