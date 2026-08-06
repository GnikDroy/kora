use std::collections::HashSet;

use crate::ir::{
    ArrayOp, BinOp, Block, CastKind, Expression, ExpressionKind, ExternDef, FunctionDef, LocalId,
    Place, PlaceKind, Program, Statement, StructId, Type, TypeId, UnOp,
};

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

/// JavaScript Number holds every number upto 2^-53
const MAX_SAFE_INT: u64 = (1 << 53) - 1;

pub(crate) fn emit(program: &Program, async_fns: HashSet<String>) -> Result<String, String> {
    let mut emitter = Emitter {
        program,
        async_fns,
        func: None,
        out: String::new(),
        error: None,
    };
    emitter.program();
    match emitter.error {
        Some(error) => Err(error),
        None => Ok(emitter.out),
    }
}

struct Emitter<'a> {
    program: &'a Program,
    async_fns: HashSet<String>,
    func: Option<&'a FunctionDef>,
    out: String,
    error: Option<String>,
}

impl<'a> Emitter<'a> {
    fn program(&mut self) {
        let program = self.program;
        for ext in &program.externs {
            self.extern_guard(ext);
        }
        for func in &program.functions {
            self.function(func);
        }
        self.out.push('\n');
        self.out.push_str(INTRINSICS);
    }

    fn extern_guard(&mut self, ext: &ExternDef) {
        let name = &ext.symbol;
        self.out.push_str(&format!(
            "var {name} = typeof {name} === \"function\" ? {name} : __kora_missing_extern(\"{name}\");"
        ));
    }

    fn local_name(&self, id: LocalId) -> &'a str {
        &self.func.expect("emitting outside a function")[id].name
    }

    fn function(&mut self, func: &'a FunctionDef) {
        self.func = Some(func);
        let async_prefix = if self.async_fns.contains(&func.symbol) {
            "async "
        } else {
            ""
        };
        let args: String = func.locals[..func.params]
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        self.out.push_str(&format!(
            "{}function {}({})",
            async_prefix, func.symbol, args
        ));
        self.out.push('{');
        self.block(&func.body);
        self.out.push('}');
    }

    fn braces(&mut self, block: &Block) {
        self.out.push('{');
        self.block(block);
        self.out.push('}');
    }

    fn block(&mut self, block: &Block) {
        for stmt in block {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Let(local, expr) => {
                self.out
                    .push_str(&format!("let {} = ", self.local_name(*local)));
                self.expr(expr);
                self.out.push(';');
            }
            Statement::Expression(expr) => {
                self.expr(expr);
                self.out.push(';');
            }
            Statement::Return(expr) => {
                self.out.push_str("return");
                if let Some(expr) = expr {
                    self.out.push(' ');
                    self.expr(expr);
                }
                self.out.push(';');
            }
            Statement::Break => self.out.push_str("break;"),
            Statement::Continue => self.out.push_str("continue;"),
            Statement::While { cond, body } => {
                self.out.push_str("while (");
                self.expr(cond);
                self.out.push(')');
                self.braces(body);
            }
            Statement::For {
                init,
                cond,
                step,
                body,
            } => {
                self.out.push_str("for (");
                self.for_init(init);
                self.out.push(';');
                self.expr(cond);
                self.out.push(';');
                self.expr(step);
                self.out.push(')');
                self.braces(body);
            }
            Statement::If {
                cond,
                then,
                otherwise,
            } => {
                self.out.push_str("if (");
                self.expr(cond);
                self.out.push(')');
                self.braces(then);
                if let Some(otherwise) = otherwise {
                    self.out.push_str("else ");
                    self.braces(otherwise);
                }
            }
        }
    }

    fn for_init(&mut self, init: &Block) {
        for (i, stmt) in init.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            match stmt {
                Statement::Let(local, expr) => {
                    self.out
                        .push_str(&format!("let {} = ", self.local_name(*local)));
                    self.expr(expr);
                }
                Statement::Expression(expr) => self.expr(expr),
                _ => unreachable!("a for-init lowers to declarations and expressions"),
            }
        }
    }

    fn operand(&mut self, expr: &Expression) {
        let compound = matches!(
            expr.kind,
            ExpressionKind::Binary { .. }
                | ExpressionKind::And(..)
                | ExpressionKind::Or(..)
                | ExpressionKind::Unary { .. }
                | ExpressionKind::Assign { .. }
        );
        if compound {
            self.out.push('(');
            self.expr(expr);
            self.out.push(')');
        } else {
            self.expr(expr);
        }
    }

    fn expr(&mut self, expr: &Expression) {
        match &expr.kind {
            ExpressionKind::Int(v) => {
                if v.unsigned_abs() > MAX_SAFE_INT && self.error.is_none() {
                    self.error = Some(format!(
                        "integer literal {v} exceeds the 53-bit range of int on the JavaScript backend ({}:{})",
                        expr.span.start.row, expr.span.start.col
                    ));
                }
                self.out.push_str(&v.to_string());
            }
            ExpressionKind::Real(v) => self.out.push_str(&v.to_string()),
            ExpressionKind::Bool(b) => self.out.push_str(if *b { "true" } else { "false" }),
            ExpressionKind::Char(c) => self.char_literal(*c),
            ExpressionKind::Str(s) => self.string_literal(s),
            ExpressionKind::None => self.out.push_str("null"),
            ExpressionKind::Array(items) => {
                self.out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.push(',');
                    }
                    self.expr(item);
                }
                self.out.push(']');
            }
            ExpressionKind::Local(local) => {
                self.out.push_str(self.local_name(*local));
            }
            ExpressionKind::Field { object, index } => {
                let name = self.field_name(object.ty, *index);
                self.operand(object);
                self.out.push('.');
                self.out.push_str(&name);
            }
            ExpressionKind::Index { array, index } => {
                self.out.push_str("__kora_runtime_index(");
                self.expr(array);
                self.out.push(',');
                self.expr(index);
                self.out.push(')');
            }
            ExpressionKind::Assign { place, value } => self.assign(place, value),
            ExpressionKind::Binary { op, left, right } => self.binary(*op, left, right),
            ExpressionKind::And(left, right) => {
                self.operand(left);
                self.out.push_str("&&");
                self.operand(right);
            }
            ExpressionKind::Or(left, right) => {
                self.operand(left);
                self.out.push_str("||");
                self.operand(right);
            }
            ExpressionKind::Unary { op, operand } => {
                self.out.push_str(match op {
                    UnOp::IntNeg | UnOp::RealNeg => "-",
                    UnOp::BoolNot => "!",
                });
                self.operand(operand);
            }
            ExpressionKind::Cast { kind, operand } => self.cast(*kind, operand),
            ExpressionKind::Call { function, args } => {
                let symbol = self.program[*function].symbol.clone();
                self.call(&symbol, args);
            }
            ExpressionKind::CallExtern { function, args } => {
                let symbol = self.program[*function].symbol.clone();
                self.call(&symbol, args);
            }
            ExpressionKind::ArrayOp { op, receiver, args } => self.array_op(*op, receiver, args),
            ExpressionKind::FnRef(function) => {
                let symbol = self.program[*function].symbol.clone();
                self.out.push_str(&symbol);
            }
            ExpressionKind::IndirectCall { callee, args } => self.call_value(callee, args),
            ExpressionKind::Copy(inner) => {
                if matches!(self.program.types[inner.ty], Type::Struct(_)) {
                    self.out.push_str("({...");
                    self.expr(inner);
                    self.out.push_str("})");
                } else {
                    self.out.push_str("Array.from(");
                    self.expr(inner);
                    self.out.push(')');
                }
            }
            ExpressionKind::StructLit { struct_, fields } => self.struct_literal(*struct_, fields),
            ExpressionKind::DefaultStruct(struct_) => self.struct_zero(*struct_),
            ExpressionKind::ArrayNew { len } => self.array_new(expr.ty, len),
            ExpressionKind::Wrap(inner) => self.expr(inner),
            ExpressionKind::Unwrap(inner) => {
                self.out.push_str("__kora_runtime_unwrap(");
                self.expr(inner);
                self.out.push(')');
            }
        }
    }

    fn call(&mut self, symbol: &str, args: &[Expression]) {
        let is_async = self.async_fns.contains(symbol);
        if is_async {
            self.out.push_str("(await ");
        }
        self.out.push_str(symbol);
        self.out.push('(');
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.expr(arg);
        }
        self.out.push(')');
        if is_async {
            self.out.push(')');
        }
    }

    /// We cannot callee is sync, so we await it whenever the surrounding function is async.
    ///
    /// NOTE: The surrounding function of an indirect call is guaranteed to be async
    /// because of the async coloring pass.
    ///
    /// We double check here just in case the two drift in the future.
    fn call_value(&mut self, callee: &Expression, args: &[Expression]) {
        let is_async = self
            .func
            .is_some_and(|f| self.async_fns.contains(&f.symbol));
        if is_async {
            self.out.push_str("(await ");
        }
        self.operand(callee);
        self.out.push('(');
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.expr(arg);
        }
        self.out.push(')');
        if is_async {
            self.out.push(')');
        }
    }

    fn array_op(&mut self, op: ArrayOp, receiver: &Expression, args: &[Expression]) {
        match op {
            ArrayOp::Pop => {
                self.out.push_str("__kora_runtime_pop(");
                self.expr(receiver);
                self.out.push(')');
            }
            ArrayOp::Insert => {
                self.out.push_str("__kora_runtime_insert(");
                self.expr(receiver);
                self.out.push(',');
                self.expr(&args[0]);
                self.out.push(',');
                self.expr(&args[1]);
                self.out.push(')');
            }
            ArrayOp::Remove => {
                self.out.push_str("__kora_runtime_remove(");
                self.expr(receiver);
                self.out.push(',');
                self.expr(&args[0]);
                self.out.push(')');
            }
            ArrayOp::Len => {
                self.operand(receiver);
                self.out.push_str(".length");
            }
            ArrayOp::Push => {
                self.operand(receiver);
                self.out.push_str(".push(");
                self.expr(&args[0]);
                self.out.push(')');
            }
            ArrayOp::Slice => {
                self.operand(receiver);
                self.out.push_str(".slice(");
                self.expr(&args[0]);
                self.out.push(',');
                self.expr(&args[1]);
                self.out.push(')');
            }
            // JS .concat is pure, so spread-push.
            ArrayOp::Extend => {
                self.operand(receiver);
                self.out.push_str(".push(...");
                self.expr(&args[0]);
                self.out.push(')');
            }
        }
    }

    fn binary(&mut self, op: BinOp, left: &Expression, right: &Expression) {
        if op == BinOp::ArrayConcat {
            self.operand(left);
            self.out.push_str(".concat(");
            self.expr(right);
            self.out.push(')');
            return;
        }

        if matches!(op, BinOp::ArrayEq | BinOp::ArrayNe) {
            if op == BinOp::ArrayNe {
                self.out.push('!');
            }
            self.out.push_str("__kora_runtime_equality_intrinsic(");
            self.expr(left);
            self.out.push(',');
            self.expr(right);
            self.out.push(')');
            return;
        }

        if matches!(op, BinOp::OptionalEq | BinOp::OptionalNe) {
            return self.optional_compare(op, left, right);
        }

        if let Some(bitwise) = bitwise_operator(op) {
            self.out.push_str("Number(BigInt.asIntN(64,BigInt(");
            self.expr(left);
            self.out.push(')');
            self.out.push_str(bitwise);
            self.out.push_str("BigInt(");
            self.expr(right);
            self.out.push_str(")))");
            return;
        }

        if op == BinOp::IntDiv || op == BinOp::IntMod {
            self.out.push_str(if op == BinOp::IntDiv {
                "__kora_runtime_div("
            } else {
                "__kora_runtime_mod("
            });
            self.expr(left);
            self.out.push(',');
            self.expr(right);
            self.out.push(')');
            return;
        }

        self.operand(left);
        self.out.push_str(infix_operator(op));
        self.operand(right);
    }

    fn optional_compare(&mut self, op: BinOp, left: &Expression, right: &Expression) {
        let has_none =
            matches!(left.kind, ExpressionKind::None) || matches!(right.kind, ExpressionKind::None);
        let arrays = self.optional_inner_is_array(left) || self.optional_inner_is_array(right);
        if !has_none && arrays {
            if op == BinOp::OptionalNe {
                self.out.push('!');
            }
            self.out.push_str("__kora_runtime_equality_intrinsic(");
            self.expr(left);
            self.out.push(',');
            self.expr(right);
            self.out.push(')');
            return;
        }
        self.operand(left);
        self.out
            .push_str(if op == BinOp::OptionalEq { "==" } else { "!=" });
        self.operand(right);
    }

    fn optional_inner_is_array(&self, expr: &Expression) -> bool {
        match self.program.types[expr.ty] {
            Type::Optional(inner) => matches!(self.program.types[inner], Type::Array(_)),
            _ => false,
        }
    }

    fn cast(&mut self, kind: CastKind, operand: &Expression) {
        let (before, after) = match kind {
            CastKind::RealToInt => ("Math.trunc(", ")"),
            CastKind::CharToInt
            | CastKind::CharToReal
            | CastKind::IntToReal
            | CastKind::Reinterpret => ("(", ")"),
            CastKind::IntToChar => ("((", ") & 255)"),
            CastKind::RealToChar => ("(Math.trunc(", ") & 255)"),
        };
        self.out.push_str(before);
        self.expr(operand);
        self.out.push_str(after);
    }

    fn assign(&mut self, place: &Place, value: &Expression) {
        match &place.kind {
            PlaceKind::Index { array, index } => {
                self.out.push_str("__kora_runtime_index_set(");
                self.place_read(array);
                self.out.push(',');
                self.expr(index);
                self.out.push(',');
                self.expr(value);
                self.out.push(')');
            }
            PlaceKind::Local(local) => {
                self.out.push_str(self.local_name(*local));
                self.out.push('=');
                self.operand(value);
            }
            PlaceKind::Field { object, index } => {
                let name = self.field_name(object.ty, *index);
                self.place_read(object);
                self.out.push('.');
                self.out.push_str(&name);
                self.out.push('=');
                self.operand(value);
            }
        }
    }

    fn place_read(&mut self, place: &Place) {
        match &place.kind {
            PlaceKind::Local(local) => {
                self.out.push_str(self.local_name(*local));
            }
            PlaceKind::Field { object, index } => {
                let name = self.field_name(object.ty, *index);
                self.place_read(object);
                self.out.push('.');
                self.out.push_str(&name);
            }
            PlaceKind::Index { array, index } => {
                self.out.push_str("__kora_runtime_index(");
                self.place_read(array);
                self.out.push(',');
                self.expr(index);
                self.out.push(')');
            }
        }
    }

    fn struct_literal(&mut self, struct_: StructId, fields: &[Expression]) {
        self.out.push_str("({");
        for (i, (field, value)) in self.program[struct_]
            .fields
            .iter()
            .map(|f| f.name.clone())
            .zip(fields)
            .enumerate()
        {
            if i > 0 {
                self.out.push(',');
            }
            self.out.push_str(&field);
            self.out.push(':');
            self.expr(value);
        }
        self.out.push_str("})");
    }

    /// NOTE: Type checker rejects struct cycles, so this terminates.
    fn struct_zero(&mut self, struct_: StructId) {
        let fields: Vec<(String, TypeId)> = self.program[struct_]
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.ty))
            .collect();
        self.out.push_str("({");
        for (i, (field, ty)) in fields.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.out.push_str(field);
            self.out.push(':');
            self.default(*ty);
        }
        self.out.push_str("})");
    }

    fn default(&mut self, ty: TypeId) {
        match self.program.types[ty] {
            Type::Struct(s) => self.struct_zero(s),
            Type::Array(_) => self.out.push_str("[]"),
            Type::Optional(_) | Type::Opaque | Type::Fn => self.out.push_str("null"),
            Type::Real => self.out.push_str("0.0"),
            Type::Bool => self.out.push_str("false"),
            Type::Char => self.out.push('0'),
            Type::Int | Type::Void => self.out.push('0'),
        }
    }

    fn array_new(&mut self, ty: TypeId, len: &Expression) {
        let Type::Array(elem) = self.program.types[ty] else {
            unreachable!("array construction always has an array type");
        };
        if let Type::Struct(s) = self.program.types[elem] {
            self.out
                .push_str("Array.from({length:__kora_runtime_check_len(");
            self.expr(len);
            self.out.push_str(")},()=>");
            self.struct_zero(s);
            self.out.push(')');
        } else {
            self.out.push_str("new Array(__kora_runtime_check_len(");
            self.expr(len);
            self.out.push_str(")).fill(");
            self.default(elem);
            self.out.push(')');
        }
    }

    fn field_name(&self, struct_ty: TypeId, index: u32) -> String {
        let Type::Struct(s) = self.program.types[struct_ty] else {
            unreachable!("field access is only lowered on struct receivers");
        };
        self.program[s].fields[index as usize].name.clone()
    }

    fn char_literal(&mut self, c: u8) {
        self.out.push_str(&c.to_string());
    }

    fn string_literal(&mut self, s: &str) {
        self.out.push('[');
        for (i, b) in s.as_bytes().iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.out.push_str(&b.to_string());
        }
        self.out.push(']');
    }
}

fn bitwise_operator(op: BinOp) -> Option<&'static str> {
    match op {
        BinOp::IntBitAnd => Some("&"),
        BinOp::IntBitOr => Some("|"),
        BinOp::IntBitXor => Some("^"),
        BinOp::IntShl => Some("<<"),
        BinOp::IntShr => Some(">>"),
        _ => None,
    }
}

fn infix_operator(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        IntAdd | RealAdd => "+",
        IntSub | RealSub => "-",
        IntMul | RealMul => "*",
        RealDiv => "/",
        IntEq | RealEq | CharEq | BoolEq | OpaqueEq => "===",
        IntNe | RealNe | CharNe | BoolNe | OpaqueNe => "!==",
        IntLt | RealLt | CharLt => "<",
        IntLe | RealLe | CharLe => "<=",
        IntGt | RealGt | CharGt => ">",
        IntGe | RealGe | CharGe => ">=",
        IntDiv | IntMod | IntBitAnd | IntBitOr | IntBitXor | IntShl | IntShr | ArrayConcat
        | ArrayEq | ArrayNe | OptionalEq | OptionalNe => {
            unreachable!("handled before the plain-infix path")
        }
    }
}
