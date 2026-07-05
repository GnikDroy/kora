mod error;

use std::collections::{HashMap, HashSet};

use self::error::TranspilerErr;
use crate::parser::*;
use crate::semantic_analyzer::ArrayMethod;

#[derive(Default, Debug)]
pub struct JsTranspiler {
    source: String,
    errors: Vec<TranspilerErr>,
    types: HashMap<NodeId, Type>,
    method_calls: HashMap<NodeId, String>,
    array_method_calls: HashMap<NodeId, ArrayMethod>,
    current_impl: Option<String>,
    /// What color is your function?
    /// https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/
    /// Necessary because javascript is a colored language.
    async_fns: HashSet<String>,
}

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

impl JsTranspiler {
    pub fn new(
        types: HashMap<NodeId, Type>,
        method_calls: HashMap<NodeId, String>,
        array_method_calls: HashMap<NodeId, ArrayMethod>,
        async_externs: HashSet<String>,
    ) -> JsTranspiler {
        JsTranspiler {
            source: String::new(),
            errors: Vec::new(),
            types,
            method_calls,
            array_method_calls,
            current_impl: None,
            async_fns: async_externs,
        }
    }

    fn mangle(struct_name: &str, method: &str) -> String {
        format!("{struct_name}${method}")
    }

    fn compute_async_set(&mut self, module: &Module) {
        let mut async_fns = self.async_fns.clone();
        let method_calls = &self.method_calls;

        let mut callees: Vec<(String, HashSet<String>)> = module
            .functions
            .iter()
            .map(|f| {
                (
                    f.node.name.clone(),
                    collect_called_names(&f.node.statement.node, method_calls),
                )
            })
            .chain(module.impls.iter().flat_map(|impl_| {
                impl_.node.functions.iter().map(|f| {
                    (
                        JsTranspiler::mangle(&impl_.node.struct_name.node, &f.node.name),
                        collect_called_names(&f.node.statement.node, method_calls),
                    )
                })
            }))
            .collect();

        loop {
            let mut changed = false;
            for (name, called) in &callees {
                if async_fns.contains(name) {
                    continue;
                }
                if called.iter().any(|c| async_fns.contains(c)) {
                    async_fns.insert(name.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        callees.clear();
        self.async_fns = async_fns;
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
            BitAnd       => "&",
            BitOr        => "|",
            BitXor       => "^",
            ShiftLeft    => "<<",
            ShiftRight   => ">>",
            Cast         => panic!(),
        }
    }
}
impl ASTVisitor for JsTranspiler {
    fn visit_module(&mut self, module: &Module) {
        self.compute_async_set(module);
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
            Some(struct_name) => JsTranspiler::mangle(struct_name, &func.node.name),
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
            .push_str(&format!("{}function {}({})", async_prefix, name, &arg_list));

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
        self.source.push_str(&format!("{}", b));
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
        for expr in exprs.iter() {
            self.source.push('(');
            self.visit_expression(expr);
            self.source.push(')');
            self.source.push(',');
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
            self.source.push('(');
            self.visit_expression(left);
            self.source.push_str(").concat(");
            self.visit_expression(right);
            self.source.push(')');
            return;
        }

        // JS `/` is float division. For Kora `int / int` we need
        // truncation toward zero to match C-style integer semantics.
        let int_div =
            matches!(op, BinaryOp::Divide) && self.types.get(&left.id) == Some(&Type::Int);

        use BinaryOp::{BitAnd, BitOr, BitXor, ShiftLeft, ShiftRight};
        let bitwise = matches!(op, BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight);

        if int_div {
            self.source.push_str("Math.trunc(");
        }
        if bitwise {
            self.source.push_str("Number(BigInt.asIntN(64,BigInt(");
        } else {
            self.source.push('(');
        }
        self.visit_expression(left);
        self.source.push(')');
        if !matches!(op, BinaryOp::Cast) {
            self.source.push_str(JsTranspiler::repr_binary_operator(op));
        }
        if bitwise {
            self.source.push_str("BigInt(");
        } else {
            self.source.push('(');
        }
        self.visit_expression(right);
        self.source.push(')');
        if bitwise {
            self.source.push_str("))");
        }
        if int_div {
            self.source.push(')');
        }
    }

    fn visit_unary_expression(&mut self, op: &UnaryOp, expr: &Spanned<Expression>) {
        self.source.push_str(JsTranspiler::repr_unary_operator(op));
        self.source.push('(');
        self.visit_expression(expr);
        self.source.push(')');
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
            self.source.push('(');
            self.visit_expression(obj);
            self.source.push(')');
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
            self.source.push('(');
            if self.async_fns.contains(&name) {
                self.source.push_str("await ");
            }
            self.source.push_str(&name);
            self.source.push('(');
            self.visit_expression(obj);
            for expr in exprs.iter() {
                self.source.push(',');
                self.visit_expression(expr);
            }
            self.source.push_str("))");
            return;
        }

        // Only await calls into functions we know are async.
        let is_async_call = match &expr.node {
            Expression::Identifier(name) => self.async_fns.contains(name),
            _ => false,
        };

        self.source.push('(');
        if is_async_call {
            self.source.push_str("await ");
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
        self.source.push(')');
    }

    fn visit_array_index_expression(
        &mut self,
        left: &Spanned<Expression>,
        right: &Spanned<Expression>,
    ) {
        self.source.push('(');
        self.visit_expression(left);
        self.source.push('[');
        self.visit_expression(right);
        self.source.push(']');
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
        self.source.push('(');
        self.visit_expression(left);
        self.source.push('.');
        self.source.push_str(member);
        self.source.push(')');
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

/// Collect the names of every function called inside a statement. Method
/// calls count under their mangled names.
fn collect_called_names(
    stmt: &Statement,
    method_calls: &HashMap<NodeId, String>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    walk_stmt(stmt, method_calls, &mut names);
    names
}

fn walk_stmt(stmt: &Statement, mc: &HashMap<NodeId, String>, out: &mut HashSet<String>) {
    match stmt {
        Statement::Empty | Statement::Break | Statement::Continue => {}
        Statement::Simple(e) | Statement::Let(_, _, e) => walk_expr(e, mc, out),
        Statement::Return(e) => {
            if let Some(e) = e {
                walk_expr(e, mc, out);
            }
        }
        Statement::While(cond, body) => {
            walk_expr(cond, mc, out);
            walk_stmt(&body.node, mc, out);
        }
        Statement::For(init, cond, step, body) => {
            walk_stmt(&init.node, mc, out);
            walk_expr(cond, mc, out);
            walk_expr(step, mc, out);
            walk_stmt(&body.node, mc, out);
        }
        Statement::If(cond, if_case, else_case) => {
            walk_expr(cond, mc, out);
            walk_stmt(&if_case.node, mc, out);
            if let Some(e) = else_case {
                walk_stmt(&e.node, mc, out);
            }
        }
        Statement::Compound(stmts) => {
            for s in stmts {
                walk_stmt(&s.node, mc, out);
            }
        }
    }
}

fn walk_expr(expr: &Spanned<Expression>, mc: &HashMap<NodeId, String>, out: &mut HashSet<String>) {
    match &expr.node {
        Expression::IntegerLiteral(_)
        | Expression::RealLiteral(_)
        | Expression::CharLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BoolLiteral(_)
        | Expression::Identifier(_) => {}
        Expression::Array(exprs) => {
            for e in exprs {
                walk_expr(e, mc, out);
            }
        }
        Expression::Binary(l, _, r) => {
            walk_expr(l, mc, out);
            walk_expr(r, mc, out);
        }
        Expression::Unary(_, e) => walk_expr(e, mc, out),
        Expression::Call(f, args) => {
            if let Expression::Identifier(name) = &f.node {
                out.insert(name.clone());
            } else if let Some(name) = mc.get(&f.id) {
                out.insert(name.clone());
                walk_expr(f, mc, out);
            } else {
                walk_expr(f, mc, out);
            }
            for a in args {
                walk_expr(a, mc, out);
            }
        }
        Expression::ArrayIndex(l, r) => {
            walk_expr(l, mc, out);
            walk_expr(r, mc, out);
        }
        Expression::Cast(e, _) => walk_expr(e, mc, out),
        Expression::Access(e, _) => walk_expr(e, mc, out),
        Expression::Construct(_, size) => {
            if let Some(s) = size {
                walk_expr(s, mc, out);
            }
        }
        Expression::StructLiteral(_, fields) => {
            for (_, value) in fields {
                walk_expr(value, mc, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::{
        js_transpiler::JsTranspiler,
        lexer,
        parser::{self, ASTVisitor},
        semantic_analyzer::{Resolver, ReturnChecker, TypeChecker},
    };

    fn transpile(source: &str) -> String {
        let prelude = r#"
            extern void clear();
            extern void print(a: string);
            extern string input();
            extern void sleep(ms: int);
            extern bool is_key_down(key: string);
            extern real random();
            extern void draw_clear();
            extern void set_color(c: string);
            extern void fill_rect(x: int, y: int, w: int, h: int);
            extern void fill_circle(x: int, y: int, r: int);
            extern void draw_text(s: string, x: int, y: int);
            extern void stroke_rect(x: int, y: int, w: int, h: int);
            extern void stroke_circle(x: int, y: int, r: int);
            extern void draw_line(x1: int, y1: int, x2: int, y2: int);
            extern void fill_triangle(x1: int, y1: int, x2: int, y2: int, x3: int, y3: int);
            extern void set_line_width(w: int);
            extern void set_font_size(px: int);
            extern void set_alpha(a: real);
            extern int canvas_width();
            extern int canvas_height();
            extern int text_width(s: string);
            extern int mouse_x();
            extern int mouse_y();
            extern bool is_mouse_down();
            extern void save();
            extern void restore();
            extern void translate(x: int, y: int);
            extern void rotate(a: real);
            extern real sqrt(x: real);
            extern real sin(x: real);
            extern real cos(x: real);
            extern real atan2(y: real, x: real);
        "#;
        let full = format!("{prelude}{source}");

        let tokens = lexer::Lexer::lex(&full).expect("lex");
        let module = parser::Parser::new(tokens).parse().expect("parse");
        let symbols = Resolver::new()
            .resolve(&[&module])
            .unwrap_or_else(|errs| panic!("resolve: {errs:?}"));

        let mut checker = TypeChecker::new(&symbols);
        checker.visit_module(&module);
        checker
            .check()
            .unwrap_or_else(|errs| panic!("type check: {errs:?}"));

        let mut return_checker = ReturnChecker::new();
        return_checker.visit_module(&module);
        return_checker
            .check()
            .unwrap_or_else(|errs| panic!("return check: {errs:?}"));

        let method_calls = checker
            .method_calls
            .iter()
            .map(|(id, sym)| (*id, symbols.symbol(*sym).name.clone()))
            .collect();
        let mut transpiler = JsTranspiler::new(
            checker.types,
            method_calls,
            checker.array_method_calls,
            HashSet::from(["input".to_string()]),
        );
        transpiler.visit_module(&module);
        transpiler
            .get_source()
            .map(|s| s.to_string())
            .unwrap_or_else(|errs| panic!("transpile: {errs:?}"))
    }

    #[test]
    fn valid() {
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
        assert_eq!(checker.check().is_ok(), true);

        let mut transpiler = JsTranspiler::new(
            checker.types,
            HashMap::new(),
            HashMap::new(),
            HashSet::from(["input".to_string()]),
        );
        transpiler.visit_module(&module);
        if let Ok(source) = transpiler.get_source() {
            println!("{}", source);
        }
    }

    #[test]
    fn test_methods_emit_mangled_global_functions() {
        let js = transpile(
            r#"
            struct P { x: int }
            impl P {
                int get(self) { return self.x; }
                P me(self) { return self; }
                void set(self, v: int) { self.x = v; }
            }
            int main() {
                let p = new P;
                p.set(3);
                return p.me().get();
            }
        "#,
        );
        assert!(js.contains("function P$get(self)"), "{js}");
        assert!(js.contains("function P$me(self)"), "{js}");
        assert!(js.contains("function P$set(self, v)"), "{js}");
        assert!(js.contains("(P$set(p,3))"), "{js}");
        assert!(js.contains("(P$get((P$me(p))))"), "{js}");
    }

    #[test]
    fn test_async_coloring_propagates_through_method_calls() {
        let js = transpile(
            r#"
            struct P { x: int }
            impl P {
                string ask(self) { return input(); }
                string relay(self) { return self.ask(); }
            }
            int main() {
                let p = new P;
                let a = p.relay();
                return 0;
            }
        "#,
        );
        assert!(js.contains("async function P$ask(self)"), "{js}");
        assert!(js.contains("async function P$relay(self)"), "{js}");
        assert!(js.contains("async function main()"), "{js}");
        assert!(js.contains("(await P$ask(self))"), "{js}");
        assert!(js.contains("(await P$relay(p))"), "{js}");
    }

    #[test]
    fn test_array_methods_emit_js_builtins() {
        let js = transpile(
            r#"
            int main() {
                let a = [1, 2];
                a.push(3);
                a.insert(0, 4);
                let x = a.remove(1);
                let y = a.pop();
                let b = a.slice(0, 1);
                a.extend([9, 9]);
                return a.len() + x + y + b.len();
            }
        "#,
        );
        assert!(js.contains("(a).push(3)"), "{js}");
        assert!(js.contains("(a).splice(0,0,4)"), "{js}");
        assert!(js.contains("(a).splice(1,1)[0]"), "{js}");
        assert!(js.contains("(a).pop()"), "{js}");
        assert!(js.contains("(a).slice(0,1)"), "{js}");
        assert!(js.contains("(a).push(..."), "{js}");
        assert!(js.contains("(a).length"), "{js}");
    }

    #[test]
    fn test_copy_and_array_plus_emit() {
        let js = transpile(
            r#"
            struct P { x: int }
            int main() {
                let a = [1, 2];
                let b = copy(a);
                let c = a + b;
                let p = new P { x: 1 };
                let q = copy(p);
                return c.len() + q.x;
            }
        "#,
        );
        assert!(js.contains("Array.from(a)"), "{js}");
        assert!(js.contains("({...p})") || js.contains("{...p}"), "{js}");
        assert!(js.contains(").concat("), "{js}");
        assert!(js.contains("({x:1})") || js.contains("{x:"), "{js}");
    }

    #[test]
    fn test_array_equality_emits_structural_compare() {
        let js = transpile(
            r#"
            int main() {
                let s = "abc";
                if (s == "quit") { return 1; }
                if (s != "exit") { return 2; }
                if (s[0] == 'a') { return 3; }
                return 0;
            }
        "#,
        );
        assert!(js.contains("__kora_runtime_equality_intrinsic("), "{js}");
        assert!(js.contains("!__kora_runtime_equality_intrinsic("), "{js}");
        assert!(js.contains("('a')"), "{js}");
        assert!(
            !js.contains("__kora_runtime_equality_intrinsic(('a')"),
            "{js}"
        );
    }

    #[test]
    fn test_cast_semantics_match_native_codegen() {
        let js = transpile(
            r#"
            int main() {
                let a = 2.9 as int;
                let b = 'a' as int;
                let c = 65 as char;
                let d = 1 as real;
                let e = 2.9 as char;
                return a + b;
            }
        "#,
        );
        assert!(js.contains("Math.trunc(2.9)"), "{js}");
        assert!(js.contains(".charCodeAt(0)"), "{js}");
        assert!(js.contains("String.fromCharCode(65)"), "{js}");
        assert!(js.contains("String.fromCharCode(Math.trunc(2.9))"), "{js}");
    }

    #[test]
    fn test_string_literals_are_mutable_arrays() {
        let js = transpile(
            r#"
            int main() {
                let s = "abc";
                s[0] = 'x';
                return s.len();
            }
        "#,
        );
        assert!(js.contains(r#"Array.from("abc")"#), "{js}");
    }

    #[test]
    fn test_scalar_arrays_are_zero_filled() {
        let js = transpile(
            r#"
            int main() {
                let a = new int[3];
                let b = new real[3];
                let c = new bool[3];
                let d = new char[3];
                return a[0];
            }
        "#,
        );
        assert!(js.contains(".fill(0)"), "{js}");
        assert!(js.contains(".fill(0.0)"), "{js}");
        assert!(js.contains(".fill(false)"), "{js}");
        assert!(js.contains(".fill(\"\\0\")"), "{js}");
    }

    #[test]
    fn transpiles_ui_examples() {
        let examples: &[(&str, &str)] = &[
            ("mandelbrot", include_str!("../../res/mandelbrot.kora")),
            ("sudoku", include_str!("../../res/sudoku.kora")),
            ("chess", include_str!("../../res/chess.kora")),
            ("snake", include_str!("../../res/snake.kora")),
            ("tetris", include_str!("../../res/tetris.kora")),
            ("pong", include_str!("../../res/pong.kora")),
            ("doom", include_str!("../../res/doom.kora")),
            ("pacman", include_str!("../../res/pacman.kora")),
        ];
        for (name, source) in examples {
            println!("transpiling {name}.kora");
            transpile(source);
        }
    }
}
