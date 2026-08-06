use std::collections::HashMap;

use super::*;
use crate::frontend::CompiledProgram;
use crate::parser as ast;
use crate::semantic_analyzer::{ArrayMethod, SymbolId};

pub(crate) fn lower(compiled: &CompiledProgram) -> Program {
    let lowering = Lowering {
        compiled,
        types: Types::new(),
        struct_ids: HashMap::new(),
        field_indices: Vec::new(),
        structs: Vec::new(),
        externs: Vec::new(),
        extern_ids: HashMap::new(),
        extern_params: Vec::new(),
        function_ids: HashMap::new(),
        fn_params: Vec::new(),
        fn_rets: Vec::new(),
        entry: None,
        current_ret: Types::VOID,
        locals: Vec::new(),
        local_ids: HashMap::new(),
    };
    lowering.run()
}

struct Lowering<'a> {
    compiled: &'a CompiledProgram,
    types: Types,

    struct_ids: HashMap<ast::NodeId, StructId>,
    field_indices: Vec<HashMap<String, u32>>,
    structs: Vec<StructDef>,

    externs: Vec<ExternDef>,
    extern_ids: HashMap<SymbolId, ExternId>,
    extern_params: Vec<Vec<TypeId>>,

    function_ids: HashMap<SymbolId, FunctionId>,
    fn_params: Vec<Vec<TypeId>>,
    fn_rets: Vec<TypeId>,
    entry: Option<FunctionId>,

    current_ret: TypeId,
    locals: Vec<Local>,
    local_ids: HashMap<SymbolId, LocalId>,
}

impl<'a> Lowering<'a> {
    fn run(mut self) -> Program {
        let modules: Vec<&ast::Module> = self
            .compiled
            .program
            .modules
            .iter()
            .map(|m| &m.module)
            .collect();

        for module in &modules {
            for decl in &module.structs {
                let id = StructId(self.struct_ids.len() as u32);
                self.struct_ids.insert(decl.id, id);
            }
        }
        for module in &modules {
            for decl in &module.structs {
                let fields = decl
                    .node
                    .members
                    .iter()
                    .map(|m| FieldDef {
                        name: m.node.name.clone(),
                        ty: self.lower_type(&m.node.typename),
                    })
                    .collect::<Vec<_>>();
                self.field_indices.push(
                    fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| (f.name.clone(), i as u32))
                        .collect(),
                );
                self.structs.push(StructDef {
                    symbol: self.compiled.emitted[&decl.id].clone(),
                    fields,
                });
            }
        }

        for module in &modules {
            for decl in &module.extern_functions {
                let id = ExternId(self.externs.len() as u32);
                let symbol = self.compiled.symbols.symbol_id_of_declaration(decl.id);
                self.extern_ids.insert(symbol.unwrap(), id);
                let params = decl
                    .node
                    .arguments
                    .iter()
                    .map(|arg| {
                        let projected = arg.node.typename.projection();
                        self.lower_type(&projected)
                    })
                    .collect();
                self.extern_params.push(params);
                self.externs.push(ExternDef {
                    symbol: decl.node.name.clone(),
                    params: decl
                        .node
                        .arguments
                        .iter()
                        .map(|arg| arg.node.typename.clone())
                        .collect(),
                    ret: decl.node.return_type.clone(),
                });
            }
        }

        let mut fn_decls: Vec<&ast::Spanned<ast::Function>> = Vec::new();
        for (m, module) in modules.iter().enumerate() {
            let decls = module
                .functions
                .iter()
                .chain(module.impls.iter().flat_map(|i| i.node.functions.iter()));
            for decl in decls {
                let id = FunctionId(fn_decls.len() as u32);
                let symbol = self.compiled.symbols.symbol_id_of_declaration(decl.id);
                self.function_ids.insert(symbol.unwrap(), id);
                let params = decl
                    .node
                    .arguments
                    .iter()
                    .map(|pair| self.lower_type(&pair.node.typename))
                    .collect();
                self.fn_params.push(params);
                let ret = decl
                    .node
                    .return_type
                    .as_ref()
                    .map(|ty| self.lower_type(ty))
                    .unwrap_or(Types::VOID);
                self.fn_rets.push(ret);
                if m == 0
                    && decl.node.name == "main"
                    && module.functions.iter().any(|f| f.id == decl.id)
                {
                    self.entry = Some(id);
                }
                fn_decls.push(decl);
            }
        }

        let functions = fn_decls
            .iter()
            .enumerate()
            .map(|(i, decl)| self.lower_function(FunctionId(i as u32), decl))
            .collect();

        Program {
            types: self.types,
            structs: self.structs,
            externs: self.externs,
            functions,
            entry: self.entry,
        }
    }

    fn lower_function(
        &mut self,
        id: FunctionId,
        decl: &ast::Spanned<ast::Function>,
    ) -> FunctionDef {
        self.current_ret = self.fn_rets[id.0 as usize];
        self.locals = Vec::new();
        self.local_ids = HashMap::new();
        for pair in &decl.node.arguments {
            self.declare_local(pair.id, &pair.node.name, &pair.node.typename);
        }
        let body = self.lower_block(&decl.node.statement);
        FunctionDef {
            symbol: self.compiled.emitted[&decl.id].clone(),
            params: decl.node.arguments.len(),
            ret: self.fn_rets[id.0 as usize],
            locals: std::mem::take(&mut self.locals),
            body,
        }
    }

    fn declare_local(&mut self, decl: ast::NodeId, name: &str, ty: &ast::Type) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        let symbol = self.compiled.symbols.symbol_id_of_declaration(decl);
        self.local_ids.insert(symbol.unwrap(), id);
        let ty = self.lower_type(ty);
        self.locals.push(Local {
            name: name.to_string(),
            ty,
        });
        id
    }

    fn lower_type(&mut self, ty: &ast::Type) -> TypeId {
        match ty {
            ast::Type::Int => Types::INT,
            ast::Type::Real => Types::REAL,
            ast::Type::Bool => Types::BOOL,
            ast::Type::Char => Types::CHAR,
            ast::Type::Opaque => Types::OPAQUE,
            ast::Type::Array(inner) => {
                let inner = self.lower_type(inner);
                self.types.intern(Type::Array(inner))
            }
            ast::Type::Optional(inner) => {
                let inner = self.lower_type(inner);
                self.types.intern(Type::Optional(inner))
            }
            ast::Type::Struct(sr) => {
                let decl = self.compiled.symbols.struct_decl_of(sr).unwrap();
                self.types.intern(Type::Struct(self.struct_ids[&decl]))
            }
            ast::Type::Function(_, _) => self.types.intern(Type::Fn),
            ast::Type::Generic(_, _) => {
                unreachable!("generics are instantiated before lowering")
            }
        }
    }

    fn ty_of(&mut self, expr: &ast::Spanned<ast::Expression>) -> TypeId {
        let ty = &self.compiled.types[&expr.id];
        self.lower_type(ty)
    }

    fn lower_block(&mut self, stmt: &ast::Spanned<ast::Statement>) -> Block {
        let mut block = Vec::new();
        self.lower_stmt_into(stmt, &mut block);
        block
    }

    fn lower_stmt_into(&mut self, stmt: &ast::Spanned<ast::Statement>, out: &mut Block) {
        match &stmt.node {
            ast::Statement::Empty => {}
            ast::Statement::Compound(stmts) => {
                for stmt in stmts {
                    self.lower_stmt_into(stmt, out);
                }
            }
            ast::Statement::Simple(expr) => {
                let expr = self.lower_expr(expr);
                out.push(Statement::Expression(expr));
            }
            ast::Statement::Return(expr) => {
                let ret = self.current_ret;
                let expr = expr.as_ref().map(|e| self.lower_expecting(e, ret));
                out.push(Statement::Return(expr));
            }
            ast::Statement::Let(name, annotation, init) => {
                let lowered = match annotation {
                    Some(ty) => {
                        let expected = self.lower_type(ty);
                        self.lower_expecting(init, expected)
                    }
                    None => self.lower_expr(init),
                };
                let declared = match annotation {
                    Some(ty) => ty,
                    None => &self.compiled.types[&init.id],
                };
                let local = self.declare_local(name.id, &name.node, declared);
                out.push(Statement::Let(local, lowered));
            }
            ast::Statement::While(cond, body) => {
                let cond = self.lower_expr(cond);
                let body = self.lower_block(body);
                out.push(Statement::While { cond, body });
            }
            ast::Statement::For(init, cond, step, body) => {
                let init = self.lower_block(init);
                let cond = self.lower_expr(cond);
                let step = self.lower_expr(step);
                let body = self.lower_block(body);
                out.push(Statement::For {
                    init,
                    cond,
                    step,
                    body,
                });
            }
            ast::Statement::If(cond, then, otherwise) => {
                let cond = self.lower_expr(cond);
                let then = self.lower_block(then);
                let otherwise = otherwise.as_ref().map(|s| self.lower_block(s));
                out.push(Statement::If {
                    cond,
                    then,
                    otherwise,
                });
            }
            ast::Statement::Break => out.push(Statement::Break),
            ast::Statement::Continue => out.push(Statement::Continue),
        }
    }

    fn lower_expecting(
        &mut self,
        expr: &ast::Spanned<ast::Expression>,
        expected: TypeId,
    ) -> Expression {
        // A bare `none` has no type of its own; it takes the expected one.
        if matches!(expr.node, ast::Expression::NoneLiteral) {
            return Expression::new(expected, expr.span.clone(), ExpressionKind::None);
        }
        let lowered = self.lower_expr(expr);
        if lowered.ty != expected
            && let Type::Optional(inner) = self.types[expected]
            && inner == lowered.ty
        {
            let span = lowered.span.clone();
            return Expression::new(expected, span, ExpressionKind::Wrap(Box::new(lowered)));
        }
        lowered
    }

    fn lower_expr(&mut self, expr: &ast::Spanned<ast::Expression>) -> Expression {
        let span = expr.span.clone();
        match &expr.node {
            ast::Expression::IntegerLiteral(v) => {
                Expression::new(Types::INT, span, ExpressionKind::Int(*v as i64))
            }
            ast::Expression::RealLiteral(v) => {
                Expression::new(Types::REAL, span, ExpressionKind::Real(*v))
            }
            ast::Expression::BoolLiteral(v) => {
                Expression::new(Types::BOOL, span, ExpressionKind::Bool(*v))
            }
            ast::Expression::CharLiteral(v) => {
                Expression::new(Types::CHAR, span, ExpressionKind::Char(*v))
            }
            ast::Expression::StringLiteral(v) => {
                let ty = self.types.intern(Type::Array(Types::CHAR));
                Expression::new(ty, span, ExpressionKind::Str(v.clone()))
            }
            ast::Expression::NoneLiteral => {
                let ty = self.ty_of(expr);
                Expression::new(ty, span, ExpressionKind::None)
            }
            ast::Expression::Identifier(_) => {
                let symbol = self.compiled.symbols.symbol_id_of_use(expr.id).unwrap();
                let ty = self.ty_of(expr);
                if let Some(&function) = self.function_ids.get(&symbol) {
                    Expression::new(ty, span, ExpressionKind::FnRef(function))
                } else {
                    let local = self.local_ids[&symbol];
                    Expression::new(ty, span, ExpressionKind::Local(local))
                }
            }
            ast::Expression::Unwrap(operand) => {
                let operand = self.lower_expr(operand);
                let Type::Optional(inner) = self.types[operand.ty] else {
                    unreachable!("type checker rejects unwrap of non-optionals");
                };
                Expression::new(inner, span, ExpressionKind::Unwrap(Box::new(operand)))
            }
            ast::Expression::Array(elems) => {
                let ty = self.ty_of(expr);
                let Type::Array(elem) = self.types[ty] else {
                    unreachable!("array literals always carry an array type");
                };
                let elems = elems
                    .iter()
                    .map(|e| self.lower_expecting(e, elem))
                    .collect();
                Expression::new(ty, span, ExpressionKind::Array(elems))
            }
            ast::Expression::Binary(left, op, right) => self.lower_binary(expr, left, *op, right),
            ast::Expression::Unary(op, operand) => {
                let operand = self.lower_expr(operand);
                let op = match (op, self.types[operand.ty]) {
                    (ast::UnaryOp::Negate, Type::Int) => UnOp::IntNeg,
                    (ast::UnaryOp::Negate, Type::Real) => UnOp::RealNeg,
                    (ast::UnaryOp::Not, Type::Bool) => UnOp::BoolNot,
                    _ => unreachable!("type checker rejects other unary operands"),
                };
                Expression::new(
                    operand.ty,
                    span,
                    ExpressionKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                )
            }
            ast::Expression::Cast(operand, target) => {
                use ast::Type as T;
                let kind = match (&self.compiled.types[&operand.id], target) {
                    (T::Int, T::Real) => CastKind::IntToReal,
                    (T::Int, T::Char) => CastKind::IntToChar,
                    (T::Real, T::Int) => CastKind::RealToInt,
                    (T::Real, T::Char) => CastKind::RealToChar,
                    (T::Char, T::Int) => CastKind::CharToInt,
                    (T::Char, T::Real) => CastKind::CharToReal,
                    (T::Opaque, T::Struct(_) | T::Array(_) | T::Function(_, _))
                    | (T::Struct(_) | T::Array(_) | T::Function(_, _), T::Opaque) => {
                        CastKind::Reinterpret
                    }
                    _ => unreachable!("type checker rejects other casts"),
                };
                let ty = self.lower_type(target);
                let operand = self.lower_expr(operand);
                Expression::new(
                    ty,
                    span,
                    ExpressionKind::Cast {
                        kind,
                        operand: Box::new(operand),
                    },
                )
            }
            ast::Expression::Call(callee, args) => self.lower_call(expr, callee, args),
            ast::Expression::ArrayIndex(array, index) => {
                let array = self.lower_expr(array);
                let index = self.lower_expr(index);
                let Type::Array(elem) = self.types[array.ty] else {
                    unreachable!("type checker rejects indexing non-arrays");
                };
                Expression::new(
                    elem,
                    span,
                    ExpressionKind::Index {
                        array: Box::new(array),
                        index: Box::new(index),
                    },
                )
            }
            ast::Expression::Access(object, member) => {
                let object = self.lower_expr(object);
                let Type::Struct(struct_) = self.types[object.ty] else {
                    unreachable!("type checker rejects member access on non-structs");
                };
                let index = self.field_indices[struct_.0 as usize][member];
                let ty = self.structs[struct_.0 as usize].fields[index as usize].ty;
                Expression::new(
                    ty,
                    span,
                    ExpressionKind::Field {
                        object: Box::new(object),
                        index,
                    },
                )
            }
            ast::Expression::Construct(_, Some(size)) => {
                let ty = self.ty_of(expr);
                let len = self.lower_expr(size);
                Expression::new(ty, span, ExpressionKind::ArrayNew { len: Box::new(len) })
            }
            ast::Expression::Construct(typename, None) => {
                let ty = self.lower_type(typename);
                let Type::Struct(struct_) = self.types[ty] else {
                    unreachable!("type checker rejects bare `new` on non-structs");
                };
                Expression::new(ty, span, ExpressionKind::DefaultStruct(struct_))
            }
            ast::Expression::StructLiteral(typename, fields) => {
                let ty = self.lower_type(typename);
                let Type::Struct(struct_) = self.types[ty] else {
                    unreachable!("struct literals always carry a struct type");
                };
                let members: Vec<(String, TypeId)> = self.structs[struct_.0 as usize]
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty))
                    .collect();
                let fields = members
                    .iter()
                    .map(|(name, member_ty)| {
                        let (_, value) = fields
                            .iter()
                            .find(|(field, _)| &field.node == name)
                            .expect("type checker requires every member");
                        self.lower_expecting(value, *member_ty)
                    })
                    .collect();
                Expression::new(ty, span, ExpressionKind::StructLit { struct_, fields })
            }
            ast::Expression::TypeApplication(_, _) => {
                unreachable!("generics are instantiated before lowering")
            }
        }
    }

    fn lower_binary(
        &mut self,
        expr: &ast::Spanned<ast::Expression>,
        left: &ast::Spanned<ast::Expression>,
        op: ast::BinaryOp,
        right: &ast::Spanned<ast::Expression>,
    ) -> Expression {
        use ast::BinaryOp as B;
        let span = expr.span.clone();
        match op {
            B::Assign => {
                let place = self.lower_place(left);
                let value = self.lower_expecting(right, place.ty);
                Expression::new(
                    place.ty,
                    span,
                    ExpressionKind::Assign {
                        place,
                        value: Box::new(value),
                    },
                )
            }
            B::And | B::Or => {
                let l = Box::new(self.lower_expr(left));
                let r = Box::new(self.lower_expr(right));
                let kind = match op {
                    B::And => ExpressionKind::And(l, r),
                    _ => ExpressionKind::Or(l, r),
                };
                Expression::new(Types::BOOL, span, kind)
            }
            _ => {
                let equality = matches!(op, B::Equality | B::NotEquality);
                let left_ty = self.operand_ty(left);
                let right_ty = self.operand_ty(right);
                let optional = [left_ty, right_ty]
                    .into_iter()
                    .flatten()
                    .find(|&ty| matches!(self.types[ty], Type::Optional(_)));
                if equality && let Some(target) = optional {
                    let l = self.lower_expecting(left, target);
                    let r = self.lower_expecting(right, target);
                    let op = if op == B::Equality {
                        BinOp::OptionalEq
                    } else {
                        BinOp::OptionalNe
                    };
                    return Expression::new(
                        Types::BOOL,
                        span,
                        ExpressionKind::Binary {
                            op,
                            left: Box::new(l),
                            right: Box::new(r),
                        },
                    );
                }
                let l = self.lower_expr(left);
                let r = self.lower_expr(right);
                let op = resolve_binop(op, self.types[l.ty]);
                let ty = match op {
                    BinOp::ArrayConcat => l.ty,
                    BinOp::IntEq
                    | BinOp::IntNe
                    | BinOp::IntLt
                    | BinOp::IntLe
                    | BinOp::IntGt
                    | BinOp::IntGe
                    | BinOp::RealEq
                    | BinOp::RealNe
                    | BinOp::RealLt
                    | BinOp::RealLe
                    | BinOp::RealGt
                    | BinOp::RealGe
                    | BinOp::CharEq
                    | BinOp::CharNe
                    | BinOp::CharLt
                    | BinOp::CharLe
                    | BinOp::CharGt
                    | BinOp::CharGe
                    | BinOp::BoolEq
                    | BinOp::BoolNe
                    | BinOp::OpaqueEq
                    | BinOp::OpaqueNe
                    | BinOp::ArrayEq
                    | BinOp::ArrayNe
                    | BinOp::OptionalEq
                    | BinOp::OptionalNe => Types::BOOL,
                    _ => l.ty,
                };
                Expression::new(
                    ty,
                    span,
                    ExpressionKind::Binary {
                        op,
                        left: Box::new(l),
                        right: Box::new(r),
                    },
                )
            }
        }
    }

    fn operand_ty(&mut self, expr: &ast::Spanned<ast::Expression>) -> Option<TypeId> {
        if matches!(expr.node, ast::Expression::NoneLiteral) {
            return self
                .compiled
                .types
                .get(&expr.id)
                .cloned()
                .map(|ty| self.lower_type(&ty));
        }
        Some(self.ty_of(expr))
    }

    fn lower_call(
        &mut self,
        expr: &ast::Spanned<ast::Expression>,
        callee: &ast::Spanned<ast::Expression>,
        args: &[ast::Spanned<ast::Expression>],
    ) -> Expression {
        let span = expr.span.clone();

        if let ast::Expression::Identifier(name) = &callee.node
            && name == "copy"
            && self.compiled.symbols.symbol_id_of_use(callee.id).is_none()
        {
            let arg = self.lower_expr(&args[0]);
            return Expression::new(arg.ty, span, ExpressionKind::Copy(Box::new(arg)));
        }

        if let Some(&method) = self.compiled.method_calls.get(&callee.id) {
            let function = self.function_ids[&method];
            let ast::Expression::Access(object, _) = &callee.node else {
                unreachable!("method calls are access expressions");
            };
            let params = self.fn_params[function.0 as usize].clone();
            let mut lowered = vec![self.lower_expr(object)];
            for (arg, &expected) in args.iter().zip(&params[1..]) {
                lowered.push(self.lower_expecting(arg, expected));
            }
            return Expression::new(
                self.fn_rets[function.0 as usize],
                span,
                ExpressionKind::Call {
                    function,
                    args: lowered,
                },
            );
        }

        if let Some(&method) = self.compiled.array_method_calls.get(&callee.id) {
            let ast::Expression::Access(object, _) = &callee.node else {
                unreachable!("array method calls are access expressions");
            };
            let receiver = self.lower_expr(object);
            let Type::Array(elem) = self.types[receiver.ty] else {
                unreachable!("array methods require an array receiver");
            };
            let op = match method {
                ArrayMethod::Len => ArrayOp::Len,
                ArrayMethod::Push => ArrayOp::Push,
                ArrayMethod::Pop => ArrayOp::Pop,
                ArrayMethod::Insert => ArrayOp::Insert,
                ArrayMethod::Remove => ArrayOp::Remove,
                ArrayMethod::Slice => ArrayOp::Slice,
                ArrayMethod::Extend => ArrayOp::Extend,
            };
            let (expected, ret) = match op {
                ArrayOp::Len => (vec![], Types::INT),
                ArrayOp::Push => (vec![elem], Types::VOID),
                ArrayOp::Pop => (vec![], elem),
                ArrayOp::Insert => (vec![Types::INT, elem], Types::VOID),
                ArrayOp::Remove => (vec![Types::INT], elem),
                ArrayOp::Slice => (vec![Types::INT, Types::INT], receiver.ty),
                ArrayOp::Extend => (vec![receiver.ty], Types::VOID),
            };
            let args = args
                .iter()
                .zip(&expected)
                .map(|(arg, &expected)| self.lower_expecting(arg, expected))
                .collect();
            return Expression::new(
                ret,
                span,
                ExpressionKind::ArrayOp {
                    op,
                    receiver: Box::new(receiver),
                    args,
                },
            );
        }

        let symbol = self.compiled.symbols.symbol_id_of_use(callee.id);
        let function_id = symbol.and_then(|s| self.function_ids.get(&s).copied());
        let extern_id = symbol.and_then(|s| self.extern_ids.get(&s).copied());
        if let Some(function) = function_id {
            let params = self.fn_params[function.0 as usize].clone();
            let args = args
                .iter()
                .zip(&params)
                .map(|(arg, &expected)| self.lower_expecting(arg, expected))
                .collect();
            Expression::new(
                self.fn_rets[function.0 as usize],
                span,
                ExpressionKind::Call { function, args },
            )
        } else if let Some(function) = extern_id {
            let params = self.extern_params[function.0 as usize].clone();
            let ret = self.externs[function.0 as usize]
                .ret
                .clone()
                .map(|ty| {
                    let projected = ty.projection();
                    self.lower_type(&projected)
                })
                .unwrap_or(Types::VOID);
            let args = args
                .iter()
                .zip(&params)
                .map(|(arg, &expected)| self.lower_expecting(arg, expected))
                .collect();
            Expression::new(ret, span, ExpressionKind::CallExtern { function, args })
        } else {
            let callee_expr = self.lower_expr(callee);
            let ast::Type::Function(ret_ast, params) = self.compiled.types[&callee.id].clone()
            else {
                unreachable!("an indirect callee has a function type");
            };
            let args = args
                .iter()
                .zip(&params)
                .map(|(arg, param)| {
                    let expected = self.lower_type(param);
                    self.lower_expecting(arg, expected)
                })
                .collect();
            let ret = ret_ast.map(|r| self.lower_type(&r)).unwrap_or(Types::VOID);
            Expression::new(
                ret,
                span,
                ExpressionKind::IndirectCall {
                    callee: Box::new(callee_expr),
                    args,
                },
            )
        }
    }

    fn lower_place(&mut self, expr: &ast::Spanned<ast::Expression>) -> Place {
        let span = expr.span.clone();
        match &expr.node {
            ast::Expression::Identifier(_) => {
                let symbol = self.compiled.symbols.symbol_id_of_use(expr.id).unwrap();
                let local = self.local_ids[&symbol];
                let ty = self.ty_of(expr);
                Place {
                    ty,
                    span,
                    kind: PlaceKind::Local(local),
                }
            }
            ast::Expression::ArrayIndex(array, index) => {
                let array = self.lower_place(array);
                let index = self.lower_expr(index);
                let Type::Array(elem) = self.types[array.ty] else {
                    unreachable!("type checker rejects indexing non-arrays");
                };
                Place {
                    ty: elem,
                    span,
                    kind: PlaceKind::Index {
                        array: Box::new(array),
                        index: Box::new(index),
                    },
                }
            }
            ast::Expression::Access(object, member) => {
                let object = self.lower_place(object);
                let Type::Struct(struct_) = self.types[object.ty] else {
                    unreachable!("type checker rejects member access on non-structs");
                };
                let index = self.field_indices[struct_.0 as usize][member];
                let ty = self.structs[struct_.0 as usize].fields[index as usize].ty;
                Place {
                    ty,
                    span,
                    kind: PlaceKind::Field {
                        object: Box::new(object),
                        index,
                    },
                }
            }
            _ => unreachable!("type checker rejects other assignment targets"),
        }
    }
}

fn resolve_binop(op: ast::BinaryOp, operand: Type) -> BinOp {
    use ast::BinaryOp as B;
    match (operand, op) {
        (Type::Int, B::Add) => BinOp::IntAdd,
        (Type::Int, B::Subtract) => BinOp::IntSub,
        (Type::Int, B::Multiply) => BinOp::IntMul,
        (Type::Int, B::Divide) => BinOp::IntDiv,
        (Type::Int, B::Modulo) => BinOp::IntMod,
        (Type::Int, B::BitAnd) => BinOp::IntBitAnd,
        (Type::Int, B::BitOr) => BinOp::IntBitOr,
        (Type::Int, B::BitXor) => BinOp::IntBitXor,
        (Type::Int, B::ShiftLeft) => BinOp::IntShl,
        (Type::Int, B::ShiftRight) => BinOp::IntShr,
        (Type::Int, B::Equality) => BinOp::IntEq,
        (Type::Int, B::NotEquality) => BinOp::IntNe,
        (Type::Int, B::Less) => BinOp::IntLt,
        (Type::Int, B::LessEqual) => BinOp::IntLe,
        (Type::Int, B::Greater) => BinOp::IntGt,
        (Type::Int, B::GreaterEqual) => BinOp::IntGe,

        (Type::Real, B::Add) => BinOp::RealAdd,
        (Type::Real, B::Subtract) => BinOp::RealSub,
        (Type::Real, B::Multiply) => BinOp::RealMul,
        (Type::Real, B::Divide) => BinOp::RealDiv,
        (Type::Real, B::Equality) => BinOp::RealEq,
        (Type::Real, B::NotEquality) => BinOp::RealNe,
        (Type::Real, B::Less) => BinOp::RealLt,
        (Type::Real, B::LessEqual) => BinOp::RealLe,
        (Type::Real, B::Greater) => BinOp::RealGt,
        (Type::Real, B::GreaterEqual) => BinOp::RealGe,

        (Type::Char, B::Equality) => BinOp::CharEq,
        (Type::Char, B::NotEquality) => BinOp::CharNe,
        (Type::Char, B::Less) => BinOp::CharLt,
        (Type::Char, B::LessEqual) => BinOp::CharLe,
        (Type::Char, B::Greater) => BinOp::CharGt,
        (Type::Char, B::GreaterEqual) => BinOp::CharGe,

        (Type::Bool, B::Equality) => BinOp::BoolEq,
        (Type::Bool, B::NotEquality) => BinOp::BoolNe,

        (Type::Opaque, B::Equality) => BinOp::OpaqueEq,
        (Type::Opaque, B::NotEquality) => BinOp::OpaqueNe,

        (Type::Array(_), B::Equality) => BinOp::ArrayEq,
        (Type::Array(_), B::NotEquality) => BinOp::ArrayNe,
        (Type::Array(_), B::Add) => BinOp::ArrayConcat,

        _ => unreachable!("type checker rejects the operator for the operand type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn lower_source(source: &str) -> Program {
        let map: HashMap<String, String> = [("main.kora".to_string(), source.to_string())].into();
        let compiled = crate::compile("main.kora", move |p: &Path| {
            p.to_str().and_then(|s| map.get(s)).cloned()
        })
        .expect("compile");
        lower(&compiled)
    }

    #[test]
    fn test_lowers_program_shape() {
        let program = lower_source(
            r#"
            struct box<T> { v: T }
            impl box<T> { T get(self) { return self.v; } }
            void log() {}
            int main() {
                log();
                let b = new box<int>{ v: 41 };
                return b.get() + 1;
            }
            "#,
        );
        let entry = program.entry.expect("entry");
        assert_eq!(program[entry].symbol, "__kora_main");
        assert!(program.structs.iter().any(|s| s.symbol == "box$$int"));
        assert!(
            program
                .functions
                .iter()
                .any(|f| f.symbol == "kora$$box$$int$get")
        );
    }

    #[test]
    fn test_optional_coercions_are_explicit_wraps() {
        let program = lower_source(
            r#"
            int main() {
                let x: int? = 5;
                if (x == none) { return 1; }
                return 0;
            }
            "#,
        );
        let main = &program[program.entry.unwrap()];
        let Statement::Let(_, init) = &main.body[0] else {
            panic!("expected let");
        };
        assert!(matches!(init.kind, ExpressionKind::Wrap(_)));
        let Statement::If { cond, .. } = &main.body[1] else {
            panic!("expected if");
        };
        assert!(matches!(
            cond.kind,
            ExpressionKind::Binary {
                op: BinOp::OptionalEq,
                ..
            }
        ));
    }

    #[test]
    fn test_fields_resolve_to_indices_in_declaration_order() {
        let program = lower_source(
            r#"
            struct p { a: int, b: int }
            int main() {
                let v = new p { b: 2, a: 1 };
                return v.b;
            }
            "#,
        );
        let main = &program[program.entry.unwrap()];
        let Statement::Let(_, init) = &main.body[0] else {
            panic!("expected let");
        };
        let ExpressionKind::StructLit { fields, .. } = &init.kind else {
            panic!("expected struct literal");
        };
        assert!(matches!(fields[0].kind, ExpressionKind::Int(1)));
        assert!(matches!(fields[1].kind, ExpressionKind::Int(2)));
        let Statement::Return(Some(ret)) = &main.body[1] else {
            panic!("expected return");
        };
        let ExpressionKind::Field { index, .. } = ret.kind else {
            panic!("expected field access");
        };
        assert_eq!(index, 1);
    }
}
