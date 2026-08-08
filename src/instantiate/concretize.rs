use std::collections::HashMap;

use super::{InstantiationStack, Instantiator};

use crate::parser::{
    Expression, Function, GenericFunction, GenericImpl, GenericStruct, Impl, NodeId, Span, Spanned,
    Statement, Struct, StructRef, Type,
};

pub(crate) type TypeSubstitutions = HashMap<String, Type>;

/// Scaffolds build concrete copy of generics with fresh NodeIds everywhere.
pub(super) fn scaffold_struct(decl: &Spanned<GenericStruct>) -> Spanned<Struct> {
    let mut decl = Spanned::new(
        Struct {
            name: decl.node.name.clone(),
            members: decl.node.members.clone(),
        },
        decl.span.clone(),
    );
    for member in decl.node.members.iter_mut() {
        member.id = NodeId::new();
        renumber_type(&mut member.node.typename);
    }
    decl
}

fn scaffold_impl(imp: &Spanned<GenericImpl>) -> Spanned<Impl> {
    let mut imp = Spanned::new(
        Impl {
            struct_ref: StructRef::unresolved(Spanned::new(
                imp.node.struct_name.node.clone(),
                imp.node.struct_name.span.clone(),
            )),
            functions: imp.node.functions.clone(),
        },
        imp.span.clone(),
    );
    for method in imp.node.functions.iter_mut() {
        renumber_function(method);
    }
    imp
}

pub(super) fn scaffold_function(func: &Spanned<GenericFunction>) -> Spanned<Function> {
    let mut func = Spanned::new(
        Function {
            return_type: func.node.return_type.clone(),
            name: func.node.name.clone(),
            arguments: func.node.arguments.clone(),
            statement: func.node.statement.clone(),
        },
        func.span.clone(),
    );
    renumber_function(&mut func);
    func
}

// Renumber gives all downstream nodes a new NodeId making the ASTNode unique.
fn renumber_function(func: &mut Spanned<Function>) {
    func.id = NodeId::new();
    if let Some(ty) = func.node.return_type.as_mut() {
        renumber_type(ty);
    }
    for pair in func.node.arguments.iter_mut() {
        pair.id = NodeId::new();
        renumber_type(&mut pair.node.typename);
    }
    renumber_statement(&mut func.node.statement);
}

fn renumber_type(ty: &mut Type) {
    match ty {
        Type::Struct(sr) => sr.name.id = NodeId::new(),
        Type::Generic(sr, args) => {
            sr.name.id = NodeId::new();
            for arg in args.iter_mut() {
                renumber_type(arg);
            }
        }
        Type::Array(inner) | Type::Optional(inner) => renumber_type(inner),
        Type::Function(ret, args) => {
            if let Some(ret) = ret.as_mut() {
                renumber_type(ret);
            }
            for arg in args.iter_mut() {
                renumber_type(arg);
            }
        }
        _ => {}
    }
}

fn renumber_statement(stmt: &mut Spanned<Statement>) {
    stmt.id = NodeId::new();
    match &mut stmt.node {
        Statement::Empty | Statement::Break | Statement::Continue => {}
        Statement::TypeIf(lhs, rhs, then_branch, else_branch) => {
            renumber_type(lhs);
            renumber_type(rhs);
            renumber_statement(then_branch);
            if let Some(else_branch) = else_branch.as_mut() {
                renumber_statement(else_branch);
            }
        }
        Statement::Simple(expr) => renumber_expression(expr),
        Statement::Return(expr) => {
            if let Some(expr) = expr.as_mut() {
                renumber_expression(expr);
            }
        }
        Statement::Let(name, ty, expr) => {
            name.id = NodeId::new();
            if let Some(ty) = ty.as_mut() {
                renumber_type(ty);
            }
            renumber_expression(expr);
        }
        Statement::While(cond, body) => {
            renumber_expression(cond);
            renumber_statement(body);
        }
        Statement::For(init, cond, step, body) => {
            renumber_statement(init);
            renumber_expression(cond);
            renumber_expression(step);
            renumber_statement(body);
        }
        Statement::If(cond, if_case, else_case) => {
            renumber_expression(cond);
            renumber_statement(if_case);
            if let Some(else_case) = else_case.as_mut() {
                renumber_statement(else_case);
            }
        }
        Statement::Compound(stmts) => {
            for stmt in stmts.iter_mut() {
                renumber_statement(stmt);
            }
        }
    }
}

fn renumber_expression(expr: &mut Spanned<Expression>) {
    expr.id = NodeId::new();
    match &mut expr.node {
        Expression::IntegerLiteral(_)
        | Expression::CharLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BoolLiteral(_)
        | Expression::RealLiteral(_)
        | Expression::NoneLiteral
        | Expression::Identifier(_) => {}
        Expression::Unwrap(inner) | Expression::Unary(_, inner) => renumber_expression(inner),
        Expression::Array(exprs) => {
            for expr in exprs.iter_mut() {
                renumber_expression(expr);
            }
        }
        Expression::Binary(left, _, right) | Expression::ArrayIndex(left, right) => {
            renumber_expression(left);
            renumber_expression(right);
        }
        Expression::Call(callee, args) => {
            renumber_expression(callee);
            for arg in args.iter_mut() {
                renumber_expression(arg);
            }
        }
        Expression::Cast(inner, ty) => {
            renumber_expression(inner);
            renumber_type(ty);
        }
        Expression::Access(inner, _) => renumber_expression(inner),
        Expression::Construct(ty, size) => {
            renumber_type(ty);
            if let Some(size) = size.as_mut() {
                renumber_expression(size);
            }
        }
        Expression::StructLiteral(ty, fields) => {
            renumber_type(ty);
            for (name, value) in fields.iter_mut() {
                name.id = NodeId::new();
                renumber_expression(value);
            }
        }
        Expression::TypeApplication(callee, args) => {
            renumber_expression(callee);
            for arg in args.iter_mut() {
                renumber_type(arg);
            }
        }
    }
}

impl Instantiator<'_> {
    pub(super) fn instance_struct(
        &mut self,
        generic: &str,
        decl: &mut Spanned<Struct>,
        args: &[Type],
        stack: &mut InstantiationStack,
    ) {
        let def = &self.generic_structs[generic];
        let params: Vec<String> = def
            .decl
            .node
            .type_params
            .iter()
            .map(|p| p.node.clone())
            .collect();
        let subst: TypeSubstitutions = params.into_iter().zip(args.iter().cloned()).collect();
        self.concretize_struct_members(&subst, decl, stack);
    }

    pub(super) fn instance_impls(
        &mut self,
        generic: &str,
        target: NodeId,
        args: &[Type],
        stack: &mut InstantiationStack,
    ) -> Vec<(usize, Spanned<Impl>)> {
        let scaffolds: Vec<(Vec<String>, Spanned<Impl>)> = self.generic_structs[generic]
            .impls
            .iter()
            .map(|imp| {
                let params = imp
                    .node
                    .type_params
                    .iter()
                    .map(|p| p.node.clone())
                    .collect();
                (params, scaffold_impl(imp))
            })
            .collect();
        scaffolds
            .into_iter()
            .map(|(params, mut imp)| {
                let module = imp.span.source.0 as usize;
                imp.node.struct_ref.target = Some(target);
                let subst: TypeSubstitutions =
                    params.into_iter().zip(args.iter().cloned()).collect();
                for method in imp.node.functions.iter_mut() {
                    self.concretize_function(module, &subst, method, stack);
                }
                (module, imp)
            })
            .collect()
    }

    pub(super) fn instance_function(
        &mut self,
        module: usize,
        generic: &str,
        decl: &mut Spanned<Function>,
        args: &[Type],
        stack: &mut InstantiationStack,
    ) {
        let def = &self.generic_fns[module][generic];
        let params: Vec<String> = def
            .decl
            .node
            .type_params
            .iter()
            .map(|p| p.node.clone())
            .collect();
        let subst: TypeSubstitutions = params.into_iter().zip(args.iter().cloned()).collect();
        self.concretize_function(module, &subst, decl, stack);
    }

    pub(super) fn concretize_program(&mut self) {
        let empty = TypeSubstitutions::new();
        for m in 0..self.program.modules.len() {
            let mut stack = InstantiationStack::new();

            let mut functions = std::mem::take(&mut self.program.modules[m].module.functions);
            for func in functions.iter_mut() {
                self.concretize_function(m, &empty, func, &mut stack);
            }
            functions.extend(std::mem::take(
                &mut self.program.modules[m].module.functions,
            ));
            self.program.modules[m].module.functions = functions;

            let mut structs = std::mem::take(&mut self.program.modules[m].module.structs);
            for decl in structs.iter_mut() {
                self.concretize_struct_members(&empty, decl, &mut stack);
            }
            structs.extend(std::mem::take(&mut self.program.modules[m].module.structs));
            self.program.modules[m].module.structs = structs;

            let mut impls = std::mem::take(&mut self.program.modules[m].module.impls);
            for imp in impls.iter_mut() {
                self.concretize_impl_methods(m, &empty, imp, &mut stack);
            }
            impls.extend(std::mem::take(&mut self.program.modules[m].module.impls));
            self.program.modules[m].module.impls = impls;
        }
    }

    pub(super) fn concretize_function(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        func: &mut Spanned<Function>,
        stack: &mut InstantiationStack,
    ) {
        let span = func.span.clone();
        if let Some(ty) = func.node.return_type.as_mut() {
            self.concretize_type(subst, ty, &span, stack);
        }
        for pair in func.node.arguments.iter_mut() {
            let span = pair.span.clone();
            self.concretize_type(subst, &mut pair.node.typename, &span, stack);
        }
        self.concretize_statement(module, subst, &mut func.node.statement, stack);
    }

    pub(super) fn concretize_struct_members(
        &mut self,
        subst: &TypeSubstitutions,
        decl: &mut Spanned<Struct>,
        stack: &mut InstantiationStack,
    ) {
        for member in decl.node.members.iter_mut() {
            let span = member.span.clone();
            self.concretize_type(subst, &mut member.node.typename, &span, stack);
        }
    }

    pub(super) fn concretize_impl_methods(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        imp: &mut Spanned<Impl>,
        stack: &mut InstantiationStack,
    ) {
        let struct_ref = &mut imp.node.struct_ref;
        if struct_ref.target.is_none() {
            struct_ref.target = self.concrete_structs.get(&struct_ref.name.node).copied();
        }
        for method in imp.node.functions.iter_mut() {
            self.concretize_function(module, subst, method, stack);
        }
    }

    fn concretize_statement(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        stmt: &mut Spanned<Statement>,
        stack: &mut InstantiationStack,
    ) {
        let span = stmt.span.clone();
        match &mut stmt.node {
            Statement::Empty | Statement::Break | Statement::Continue => {}
            Statement::TypeIf(..) => self.concretize_type_if(module, subst, stmt, stack),
            Statement::Simple(expr) => self.concretize_expression(module, subst, expr, stack),
            Statement::Return(expr) => {
                if let Some(expr) = expr.as_mut() {
                    self.concretize_expression(module, subst, expr, stack);
                }
            }
            Statement::Let(_, ty, expr) => {
                if let Some(ty) = ty.as_mut() {
                    self.concretize_type(subst, ty, &span, stack);
                }
                self.concretize_expression(module, subst, expr, stack);
            }
            Statement::While(cond, body) => {
                self.concretize_expression(module, subst, cond, stack);
                self.concretize_statement(module, subst, body, stack);
            }
            Statement::For(init, cond, step, body) => {
                self.concretize_statement(module, subst, init, stack);
                self.concretize_expression(module, subst, cond, stack);
                self.concretize_expression(module, subst, step, stack);
                self.concretize_statement(module, subst, body, stack);
            }
            Statement::If(cond, if_case, else_case) => {
                self.concretize_expression(module, subst, cond, stack);
                self.concretize_statement(module, subst, if_case, stack);
                if let Some(else_case) = else_case.as_mut() {
                    self.concretize_statement(module, subst, else_case, stack);
                }
            }
            Statement::Compound(stmts) => {
                for stmt in stmts.iter_mut() {
                    self.concretize_statement(module, subst, stmt, stack);
                }
            }
        }
    }

    fn concretize_type_if(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        stmt: &mut Spanned<Statement>,
        stack: &mut InstantiationStack,
    ) {
        let span = stmt.span.clone();
        let Statement::TypeIf(mut lhs, mut rhs, then_branch, else_branch) =
            std::mem::replace(&mut stmt.node, Statement::Empty)
        else {
            unreachable!("guarded by the caller");
        };
        self.concretize_type(subst, &mut lhs, &span, stack);
        self.concretize_type(subst, &mut rhs, &span, stack);
        self.check_type_if_operand(&lhs);
        self.check_type_if_operand(&rhs);

        let survivor = if lhs == rhs {
            Some(then_branch)
        } else {
            else_branch
        };
        if let Some(mut branch) = survivor {
            self.concretize_statement(module, subst, &mut branch, stack);
            *stmt = *branch;
        }
    }

    fn check_type_if_operand(&mut self, ty: &Type) {
        match ty {
            Type::Struct(sr) => {
                if sr.target.is_none() {
                    self.error(
                        format!(
                            "unknown type `{}` in a compile-time if; it must be a type \
                             parameter or a defined type",
                            sr.name.node
                        ),
                        &sr.name.span,
                    );
                }
            }
            Type::Array(inner) | Type::Optional(inner) => self.check_type_if_operand(inner),
            Type::Function(ret, args) => {
                if let Some(ret) = ret {
                    self.check_type_if_operand(ret);
                }
                for arg in args {
                    self.check_type_if_operand(arg);
                }
            }
            _ => {}
        }
    }

    fn concretize_expression(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        expr: &mut Spanned<Expression>,
        stack: &mut InstantiationStack,
    ) {
        let span = expr.span.clone();
        let mut replacement = None;
        match &mut expr.node {
            Expression::IntegerLiteral(_)
            | Expression::CharLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BoolLiteral(_)
            | Expression::RealLiteral(_)
            | Expression::NoneLiteral
            | Expression::Identifier(_) => {}
            Expression::Unwrap(inner) | Expression::Unary(_, inner) => {
                self.concretize_expression(module, subst, inner, stack)
            }
            Expression::Array(exprs) => {
                for expr in exprs.iter_mut() {
                    self.concretize_expression(module, subst, expr, stack);
                }
            }
            Expression::Binary(left, _, right) | Expression::ArrayIndex(left, right) => {
                self.concretize_expression(module, subst, left, stack);
                self.concretize_expression(module, subst, right, stack);
            }
            Expression::Call(callee, args) => {
                self.concretize_expression(module, subst, callee, stack);
                for arg in args.iter_mut() {
                    self.concretize_expression(module, subst, arg, stack);
                }
            }
            Expression::Cast(inner, ty) => {
                self.concretize_expression(module, subst, inner, stack);
                self.concretize_type(subst, ty, &span, stack);
            }
            Expression::Access(inner, _) => self.concretize_expression(module, subst, inner, stack),
            Expression::Construct(ty, size) => {
                self.concretize_type(subst, ty, &span, stack);
                if let Some(size) = size.as_mut() {
                    self.concretize_expression(module, subst, size, stack);
                }
            }
            Expression::StructLiteral(ty, fields) => {
                self.concretize_type(subst, ty, &span, stack);
                for (_, value) in fields.iter_mut() {
                    self.concretize_expression(module, subst, value, stack);
                }
            }
            Expression::TypeApplication(callee, args) => {
                for arg in args.iter_mut() {
                    self.concretize_type(subst, arg, &span, stack);
                }
                replacement =
                    self.resolve_type_application(module, expr.id, callee, args, &span, stack);
            }
        }
        if let Some(node) = replacement {
            expr.node = node;
        }
    }

    pub(super) fn concretize_type(
        &mut self,
        subst: &TypeSubstitutions,
        ty: &mut Type,
        span: &Span,
        stack: &mut InstantiationStack,
    ) {
        let mut replacement = None;
        match ty {
            Type::Int | Type::Real | Type::Bool | Type::Char | Type::Opaque => {}
            Type::Struct(sr) => {
                if let Some(concrete) = subst.get(&sr.name.node) {
                    replacement = Some(concrete.clone());
                } else if sr.target.is_none() {
                    if self.generic_structs.contains_key(&sr.name.node) {
                        let msg = format!(
                            "generic struct `{}` requires type arguments: {}<...>",
                            sr.name.node, sr.name.node
                        );
                        let span = sr.name.span.clone();
                        self.error(msg, &span);
                    } else {
                        sr.target = self.concrete_structs.get(&sr.name.node).copied();
                    }
                }
            }
            Type::Generic(sr, args) => {
                let generic_name = sr.name.node.clone();
                let name_span = sr.name.span.clone();
                for arg in args.iter_mut() {
                    self.concretize_type(subst, arg, span, stack);
                }
                if subst.contains_key(&generic_name) {
                    self.error(
                        "a type parameter cannot take type arguments".to_string(),
                        &name_span,
                    );
                } else if !self.generic_structs.contains_key(&generic_name) {
                    self.error(
                        format!("`{generic_name}` is not a generic struct"),
                        &name_span,
                    );
                } else if let Some(decl) =
                    self.instantiate_struct(&generic_name, args, &name_span, stack)
                {
                    replacement = Some(Type::Struct(StructRef {
                        name: Spanned::new(generic_name, name_span),
                        target: Some(decl),
                    }));
                }
            }
            Type::Array(inner) => self.concretize_type(subst, inner, span, stack),
            Type::Optional(inner) => {
                self.concretize_type(subst, inner, span, stack);
                if matches!(**inner, Type::Optional(_)) {
                    self.error(
                        "instantiation produces a nested optional; the argument is already optional"
                            .to_string(),
                        span,
                    );
                }
            }
            Type::Function(ret, args) => {
                if let Some(ret) = ret.as_mut() {
                    self.concretize_type(subst, ret, span, stack);
                }
                for arg in args.iter_mut() {
                    self.concretize_type(subst, arg, span, stack);
                }
            }
        }
        if let Some(concrete) = replacement {
            *ty = concrete;
            renumber_type(ty);
        }
    }

    fn resolve_type_application(
        &mut self,
        module: usize,
        mention: NodeId,
        callee: &Spanned<Expression>,
        args: &[Type],
        span: &Span,
        stack: &mut InstantiationStack,
    ) -> Option<Expression> {
        match &callee.node {
            Expression::Identifier(name) => {
                if !self.generic_fns[module].contains_key(name) {
                    self.error(
                        format!("`{name}` is not a generic function in this module"),
                        span,
                    );
                    return None;
                }
                let decl = self.instantiate_function(module, name, args, span, stack)?;
                self.output.resolutions.insert(mention, decl);
                Some(Expression::Identifier(name.clone()))
            }
            Expression::Access(inner, member) => {
                let Expression::Identifier(alias) = &inner.node else {
                    self.error(
                        "type arguments can only be applied to a named function".to_string(),
                        span,
                    );
                    return None;
                };
                let Some(&target) = self.imports[module].get(alias) else {
                    self.error(format!("`{alias}` is not an imported module"), span);
                    return None;
                };
                if !self.generic_fns[target].contains_key(member) {
                    self.error(
                        format!("`{member}` is not a generic function in module `{alias}`"),
                        span,
                    );
                    return None;
                }
                let decl = self.instantiate_function(target, member, args, span, stack)?;
                self.output.resolutions.insert(mention, decl);
                Some(Expression::Access(inner.clone(), member.clone()))
            }
            _ => {
                self.error(
                    "type arguments can only be applied to a named function".to_string(),
                    span,
                );
                None
            }
        }
    }
}
