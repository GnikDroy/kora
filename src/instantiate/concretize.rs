use super::{Chain, Instantiator, TypeSubstitutions};
use crate::parser::{
    Expression, Function, GenericFunction, GenericImpl, GenericStruct, Impl, NodeId, Span, Spanned,
    Statement, Struct, StructRef, Type,
};

/// Scaffolds build concrete copy of generics with fresh NodeIds everywhere.
fn scaffold_struct(decl: &Spanned<GenericStruct>) -> Spanned<Struct> {
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

fn scaffold_function(func: &Spanned<GenericFunction>) -> Spanned<Function> {
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
        Type::Generic(name, args) => {
            name.id = NodeId::new();
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
        instance: &str,
        args: &[Type],
        chain: &mut Chain,
    ) -> Spanned<Struct> {
        let def = &self.generic_structs[generic];
        let params: Vec<String> = def
            .decl
            .node
            .type_params
            .iter()
            .map(|p| p.node.clone())
            .collect();
        let mut decl = scaffold_struct(&def.decl);
        decl.node.name = instance.to_string();
        let subst: TypeSubstitutions = params.into_iter().zip(args.iter().cloned()).collect();
        self.concretize_struct_members(&subst, &mut decl, chain);
        decl
    }

    pub(super) fn instance_impls(
        &mut self,
        generic: &str,
        instance: &str,
        args: &[Type],
        chain: &mut Chain,
    ) -> Vec<(usize, Spanned<Impl>)> {
        let scaffolds: Vec<(usize, Vec<String>, Spanned<Impl>)> = self.generic_structs[generic]
            .impls
            .iter()
            .map(|(module, imp)| {
                let params = imp
                    .node
                    .type_params
                    .iter()
                    .map(|p| p.node.clone())
                    .collect();
                (*module, params, scaffold_impl(imp))
            })
            .collect();
        scaffolds
            .into_iter()
            .map(|(module, params, mut imp)| {
                imp.node.struct_ref.name.node = instance.to_string();
                let subst: TypeSubstitutions =
                    params.into_iter().zip(args.iter().cloned()).collect();
                for method in imp.node.functions.iter_mut() {
                    self.concretize_function(module, &subst, method, chain);
                }
                (module, imp)
            })
            .collect()
    }

    pub(super) fn instance_function(
        &mut self,
        module: usize,
        generic: &str,
        instance: &str,
        args: &[Type],
        chain: &mut Chain,
    ) -> Spanned<Function> {
        let def = &self.generic_fns[&(module, generic.to_string())];
        let params: Vec<String> = def
            .decl
            .node
            .type_params
            .iter()
            .map(|p| p.node.clone())
            .collect();
        let mut decl = scaffold_function(&def.decl);
        decl.node.name = instance.to_string();
        let subst: TypeSubstitutions = params.into_iter().zip(args.iter().cloned()).collect();
        self.concretize_function(module, &subst, &mut decl, chain);
        decl
    }

    pub(super) fn concretize_program(&mut self) {
        let empty = TypeSubstitutions::new();
        for m in 0..self.program.modules.len() {
            let mut chain = Chain::new();

            let mut functions = std::mem::take(&mut self.program.modules[m].module.functions);
            for func in functions.iter_mut() {
                self.concretize_function(m, &empty, func, &mut chain);
            }
            functions.extend(std::mem::take(
                &mut self.program.modules[m].module.functions,
            ));
            self.program.modules[m].module.functions = functions;

            let mut structs = std::mem::take(&mut self.program.modules[m].module.structs);
            for decl in structs.iter_mut() {
                self.concretize_struct_members(&empty, decl, &mut chain);
            }
            structs.extend(std::mem::take(&mut self.program.modules[m].module.structs));
            self.program.modules[m].module.structs = structs;

            let mut impls = std::mem::take(&mut self.program.modules[m].module.impls);
            for imp in impls.iter_mut() {
                self.concretize_impl_methods(m, &empty, imp, &mut chain);
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
        chain: &mut Chain,
    ) {
        let span = func.span.clone();
        if let Some(ty) = func.node.return_type.as_mut() {
            self.concretize_type(subst, ty, &span, chain);
        }
        for pair in func.node.arguments.iter_mut() {
            let span = pair.span.clone();
            self.concretize_type(subst, &mut pair.node.typename, &span, chain);
        }
        self.concretize_statement(module, subst, &mut func.node.statement, chain);
    }

    pub(super) fn concretize_struct_members(
        &mut self,
        subst: &TypeSubstitutions,
        decl: &mut Spanned<Struct>,
        chain: &mut Chain,
    ) {
        for member in decl.node.members.iter_mut() {
            let span = member.span.clone();
            self.concretize_type(subst, &mut member.node.typename, &span, chain);
        }
    }

    pub(super) fn concretize_impl_methods(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        imp: &mut Spanned<Impl>,
        chain: &mut Chain,
    ) {
        for method in imp.node.functions.iter_mut() {
            self.concretize_function(module, subst, method, chain);
        }
    }

    fn concretize_statement(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        stmt: &mut Spanned<Statement>,
        chain: &mut Chain,
    ) {
        let span = stmt.span.clone();
        match &mut stmt.node {
            Statement::Empty | Statement::Break | Statement::Continue => {}
            Statement::Simple(expr) => self.concretize_expression(module, subst, expr, chain),
            Statement::Return(expr) => {
                if let Some(expr) = expr.as_mut() {
                    self.concretize_expression(module, subst, expr, chain);
                }
            }
            Statement::Let(_, ty, expr) => {
                if let Some(ty) = ty.as_mut() {
                    self.concretize_type(subst, ty, &span, chain);
                }
                self.concretize_expression(module, subst, expr, chain);
            }
            Statement::While(cond, body) => {
                self.concretize_expression(module, subst, cond, chain);
                self.concretize_statement(module, subst, body, chain);
            }
            Statement::For(init, cond, step, body) => {
                self.concretize_statement(module, subst, init, chain);
                self.concretize_expression(module, subst, cond, chain);
                self.concretize_expression(module, subst, step, chain);
                self.concretize_statement(module, subst, body, chain);
            }
            Statement::If(cond, if_case, else_case) => {
                self.concretize_expression(module, subst, cond, chain);
                self.concretize_statement(module, subst, if_case, chain);
                if let Some(else_case) = else_case.as_mut() {
                    self.concretize_statement(module, subst, else_case, chain);
                }
            }
            Statement::Compound(stmts) => {
                for stmt in stmts.iter_mut() {
                    self.concretize_statement(module, subst, stmt, chain);
                }
            }
        }
    }

    fn concretize_expression(
        &mut self,
        module: usize,
        subst: &TypeSubstitutions,
        expr: &mut Spanned<Expression>,
        chain: &mut Chain,
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
                self.concretize_expression(module, subst, inner, chain)
            }
            Expression::Array(exprs) => {
                for expr in exprs.iter_mut() {
                    self.concretize_expression(module, subst, expr, chain);
                }
            }
            Expression::Binary(left, _, right) | Expression::ArrayIndex(left, right) => {
                self.concretize_expression(module, subst, left, chain);
                self.concretize_expression(module, subst, right, chain);
            }
            Expression::Call(callee, args) => {
                self.concretize_expression(module, subst, callee, chain);
                for arg in args.iter_mut() {
                    self.concretize_expression(module, subst, arg, chain);
                }
            }
            Expression::Cast(inner, ty) => {
                self.concretize_expression(module, subst, inner, chain);
                self.concretize_type(subst, ty, &span, chain);
            }
            Expression::Access(inner, _) => self.concretize_expression(module, subst, inner, chain),
            Expression::Construct(ty, size) => {
                self.concretize_type(subst, ty, &span, chain);
                if let Some(size) = size.as_mut() {
                    self.concretize_expression(module, subst, size, chain);
                }
            }
            Expression::StructLiteral(ty, fields) => {
                self.concretize_type(subst, ty, &span, chain);
                for (_, value) in fields.iter_mut() {
                    self.concretize_expression(module, subst, value, chain);
                }
            }
            Expression::TypeApplication(callee, args) => {
                for arg in args.iter_mut() {
                    self.concretize_type(subst, arg, &span, chain);
                }
                replacement = self.resolve_type_application(module, callee, args, &span, chain);
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
        chain: &mut Chain,
    ) {
        let mut replacement = None;
        match ty {
            Type::Int | Type::Real | Type::Bool | Type::Char | Type::Opaque => {}
            Type::Struct(sr) => {
                if let Some(concrete) = subst.get(&sr.name.node) {
                    replacement = Some(concrete.clone());
                } else if self.generic_structs.contains_key(&sr.name.node) {
                    let msg = format!(
                        "generic struct `{}` requires type arguments: {}<...>",
                        sr.name.node, sr.name.node
                    );
                    let span = sr.name.span.clone();
                    self.error(msg, &span);
                }
            }
            Type::Generic(name, args) => {
                let generic_name = name.node.clone();
                let name_span = name.span.clone();
                for arg in args.iter_mut() {
                    self.concretize_type(subst, arg, span, chain);
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
                } else if let Some(instance) =
                    self.instantiate_struct(&generic_name, args, &name_span, chain)
                {
                    replacement =
                        Some(Type::Struct(StructRef::unresolved(Spanned::new(
                            instance, name_span,
                        ))));
                }
            }
            Type::Array(inner) => self.concretize_type(subst, inner, span, chain),
            Type::Optional(inner) => {
                self.concretize_type(subst, inner, span, chain);
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
                    self.concretize_type(subst, ret, span, chain);
                }
                for arg in args.iter_mut() {
                    self.concretize_type(subst, arg, span, chain);
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
        callee: &Spanned<Expression>,
        args: &[Type],
        span: &Span,
        chain: &mut Chain,
    ) -> Option<Expression> {
        match &callee.node {
            Expression::Identifier(name) => {
                if !self.generic_fns.contains_key(&(module, name.clone())) {
                    self.error(
                        format!("`{name}` is not a generic function in this module"),
                        span,
                    );
                    return None;
                }
                let instance = self.instantiate_function(module, name, args, span, chain)?;
                Some(Expression::Identifier(instance))
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
                if !self.generic_fns.contains_key(&(target, member.clone())) {
                    self.error(
                        format!("`{member}` is not a generic function in module `{alias}`"),
                        span,
                    );
                    return None;
                }
                let instance = self.instantiate_function(target, member, args, span, chain)?;
                Some(Expression::Access(inner.clone(), instance))
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
