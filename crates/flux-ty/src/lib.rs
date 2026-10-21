//! Type inference and checking.
//!
//! Signatures come first, from the item tree alone ([`signatures`]); then
//! each body is inferred on its own against them ([`infer_body`]), so
//! bodies can be checked in parallel and an edit to one body does not
//! affect another's types.
//!
//! Inference is unification over type variables. An unsuffixed integer
//! literal gets an *integer* variable that only unifies with integer types
//! and defaults to `i32`. `!` coerces to any type instead of unifying, and
//! a variable fed only by `!` falls back to `!`, as in edition 2024.
//! Checks that need final types (literal ranges, operand types, casts)
//! run once the body is inferred.
//!
//! This is also where flux makes the checks rustc makes before borrow
//! checking: calls to `unsafe` functions need an `unsafe` block, and
//! assignments need a `mut` binding.

mod infer;
mod ty;

use std::fmt::Write;
use std::ops::Index;

use flux_arena::ArenaMap;
use flux_hir::def_map::{Def, Ns, ResolveError, Resolved};
use flux_hir::{
    Const, Diagnostic, Function, FunctionId, Generics, Hir, Impl, ImplId, ImplTarget, ItemLoc,
    ItemTree, ModuleId, Path, Struct, StructId, TypeRef,
};
use flux_intern::{Symbol, sym};
use flux_syntax::TextRange;

pub use infer::{InferenceResult, Method, c_char_ptr, c_str_ref, infer_body, infer_const};
pub use ty::{AdtId, GenericArgs, InferVar, IntTy, Mutability, ParamTy, Ty, TyKind, subst_list};

/// A function's resolved signature. A generic function's mentions its
/// parameters, its `impl` block's first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub params: Box<[Ty]>,
    pub ret: Ty,
}

/// A function with generic arguments for its parameters, its `impl`
/// block's first: what a call calls, and, once monomorphized, a function
/// in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instance {
    pub function: FunctionId,
    pub args: GenericArgs,
}

impl FnSig {
    /// The signature of the instance with generic arguments `args`.
    pub fn subst(&self, args: &[Ty]) -> FnSig {
        FnSig {
            params: self.params.iter().map(|p| p.subst(args)).collect(),
            ret: self.ret.subst(args),
        }
    }
}

/// Everything about a file's items that bodies depend on: each
/// function's signature, each struct's field types, and each `impl`
/// block's type, in terms of their generic parameters.
#[derive(Debug, Default)]
pub struct Signatures {
    pub fns: ArenaMap<Function, FnSig>,
    pub structs: ArenaMap<Struct, &'static [Ty]>,
    pub impls: ArenaMap<Impl, Ty>,
    pub consts: ArenaMap<Const, Ty>,
}

impl Index<FunctionId> for Signatures {
    type Output = FnSig;

    fn index(&self, function: FunctionId) -> &FnSig {
        &self.fns[function]
    }
}

impl Signatures {
    /// The types of the fields of `ty`, a tuple or struct.
    pub fn fields(&self, ty: Ty) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Tuple(elements) => Some(elements),
            TyKind::Adt(adt, args) => Some(self.field_types(adt.strukt, args)),
            _ => None,
        }
    }

    /// The types of the fields of `strukt` with generic arguments `args`.
    pub fn field_types(&self, strukt: StructId, args: &[Ty]) -> &'static [Ty] {
        let fields = self.structs[strukt];
        if args.is_empty() {
            return fields;
        }
        Ty::list(&subst_list(fields, args))
    }
}

/// What the names in a type mean besides items: `Self`, if anything,
/// and the generic parameters in scope, by index.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeScope<'a> {
    pub self_ty: Option<Ty>,
    pub generics: &'a [Symbol],
    /// In an item's signature, where `_` is not allowed, what rustc
    /// calls such items ("functions"); `None` in a body, where `_` is a
    /// type to infer.
    pub signature_of: Option<&'static str>,
}

impl TypeScope<'_> {
    /// The generic parameter called `name`, if one is in scope.
    pub fn param(&self, name: Symbol) -> Option<Ty> {
        let index = self.generics.iter().rposition(|&n| n == name)?;
        Some(Ty::param(index as u32, name))
    }
}

/// The names of the generic parameters of an item in `impl` block
/// `owner` with its own `generics`: the block's, then its own.
pub fn generic_names(items: &ItemTree, owner: Option<ImplId>, own: &Generics) -> Vec<Symbol> {
    owner
        .map(|o| &items.impls[o].generics)
        .into_iter()
        .chain([own])
        .flat_map(|g| g.params.iter().map(|p| p.name))
        .collect()
}

/// "1 generic argument", "2 generic arguments".
fn generic_arguments(n: usize) -> String {
    format!("{n} generic argument{}", if n == 1 { "" } else { "s" })
}

/// rustc's error for `got` generic arguments where a `kind` (struct,
/// function, ...) takes `want`.
pub fn wrong_arg_count(kind: &str, want: usize, got: usize) -> String {
    let verb = if got == 1 { "was" } else { "were" };
    format!(
        "{kind} takes {} but {} {verb} supplied",
        generic_arguments(want),
        generic_arguments(got)
    )
}

/// Resolves a type as written in `module`: a struct, a primitive type,
/// a generic parameter, or `Self`. Errors go to `errors`, at the part of
/// the type they are about within `range`, and make the type an error
/// unless they are about privacy.
pub fn resolve_type(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let mutability = |mutable: bool| {
        if mutable {
            Mutability::Mut
        } else {
            Mutability::Not
        }
    };
    let fail = |errors: &mut Vec<(TextRange, String)>, at: TextRange, message: String| {
        errors.push((at, message));
        Ty::ERROR
    };
    match ty {
        TypeRef::Path(path) => {
            let last = path.segments.len().saturating_sub(1);
            // Generic arguments are only for the type itself, not the
            // modules before it.
            for i in 0..last {
                if let Some(args) = path.args(i) {
                    let name = path.segments[i];
                    errors.push((
                        args.ranges.first().copied().unwrap_or(args.range),
                        format!("type arguments are not allowed on module `{name}`"),
                    ));
                }
            }
            let args = path.args(last);
            // Arguments to something that takes none.
            let no_args = |errors: &mut Vec<(TextRange, String)>, what: String| {
                if let Some(args) = args {
                    errors.push((
                        args.ranges.first().copied().unwrap_or(args.range),
                        format!("type arguments are not allowed on {what}"),
                    ));
                }
            };
            if let Some(name) = path.as_name()
                && let Some(param) = scope.param(name)
            {
                no_args(errors, format!("type parameter `{name}`"));
                return param;
            }
            let resolved = match items.resolve(module, path, Ns::Types) {
                Err(e @ ResolveError::Private { .. }) => {
                    let ResolveError::Private {
                        message, resolved, ..
                    } = &e
                    else {
                        unreachable!()
                    };
                    errors.push((e.range(path, range), message.clone()));
                    Ok(*resolved)
                }
                result => result,
            };
            let last_range = path.range(last, range);
            match resolved {
                Ok(Resolved::SelfType) => {
                    no_args(errors, "self type".to_owned());
                    match scope.self_ty {
                        Some(ty) => ty,
                        None => fail(
                            errors,
                            range,
                            "cannot find type `Self` in this scope".to_owned(),
                        ),
                    }
                }
                Ok(Resolved::Def(Def::Struct(id))) => {
                    struct_type(items, module, id, path, scope, range, errors)
                }
                Ok(Resolved::Primitive(name)) => {
                    no_args(errors, format!("builtin type `{name}`"));
                    match primitive(name) {
                        Ok(ty) => ty,
                        Err(message) => fail(errors, range, message),
                    }
                }
                Ok(Resolved::Def(Def::Module(m))) => fail(
                    errors,
                    last_range,
                    format!("expected type, found module `{}`", items.modules[m].name),
                ),
                Ok(_) => fail(errors, last_range, format!("expected type, found `{path}`")),
                Err(e @ ResolveError::NotFound { .. }) => {
                    let ResolveError::NotFound { segment, module } = e else {
                        unreachable!()
                    };
                    let name = path.segments[segment];
                    let place = match module {
                        Some(module) => items.in_module(module),
                        None => "this scope".to_owned(),
                    };
                    fail(
                        errors,
                        e.range(path, range),
                        format!("cannot find type `{name}` in {place}"),
                    )
                }
                Err(ResolveError::Reported) => Ty::ERROR,
                Err(e) => {
                    let at = e.range(path, range);
                    let (ResolveError::Failed { message, .. }
                    | ResolveError::Private { message, .. }) = e
                    else {
                        unreachable!()
                    };
                    fail(errors, at, message)
                }
            }
        }
        TypeRef::Tuple(fields) => {
            let fields: Vec<Ty> = fields
                .iter()
                .map(|f| resolve_type(items, module, f, scope, range, errors))
                .collect();
            if fields.contains(&Ty::ERROR) {
                return Ty::ERROR;
            }
            Ty::tuple(&fields)
        }
        TypeRef::Never => Ty::NEVER,
        TypeRef::Ptr { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::raw_ptr(mutability(*mutable), pointee)
        }
        TypeRef::Ref { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::new(TyKind::Ref(mutability(*mutable), pointee))
        }
        TypeRef::Infer => match scope.signature_of {
            Some(items) => fail(
                errors,
                range,
                format!(
                    "the placeholder `_` is not allowed within types on item signatures for {items}"
                ),
            ),
            None => Ty::infer(ty::InferVar::PLACEHOLDER),
        },
        TypeRef::Error => Ty::ERROR,
    }
}

/// Struct `strukt`, named by `path`, with the generic arguments written
/// after its last segment, which must be one for each of its
/// parameters.
pub fn struct_type(
    items: &ItemTree,
    module: ModuleId,
    strukt: StructId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let def = &items.structs[strukt];
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, range);
    let want = def.generics.len();
    let Some(written) = path.args(last) else {
        if want == 0 {
            return Ty::adt(strukt, def.name, &[]);
        }
        errors.push((at, format!("missing generics for struct `{}`", def.name)));
        return Ty::ERROR;
    };
    let args: Vec<Ty> = written
        .types
        .iter()
        .zip(&written.ranges)
        .map(|(ty, &range)| resolve_type(items, module, ty, scope, range, errors))
        .collect();
    if args.len() != want {
        errors.push((at, wrong_arg_count("struct", want, args.len())));
        return Ty::ERROR;
    }
    if args.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    Ty::adt(strukt, def.name, &args)
}

/// The primitive type called `name`.
pub fn primitive(name: Symbol) -> Result<Ty, String> {
    if name == sym::bool {
        return Ok(Ty::BOOL);
    }
    match IntTy::from_name(name) {
        Some(int) => Ok(Ty::int(int)),
        None => Err(format!("`{name}` is not supported yet")),
    }
}

/// The `impl` blocks' type for methods on `ty`, if it has any.
pub fn impl_target(ty: Ty) -> Option<ImplTarget> {
    match ty.kind() {
        TyKind::Adt(adt, _) => Some(ImplTarget::Struct(adt.strukt)),
        TyKind::Int(int) => Some(ImplTarget::Primitive(int.name())),
        TyKind::Bool => Some(ImplTarget::Primitive(sym::bool)),
        _ => None,
    }
}

/// What `Self` means in an item in the `impl` block `owner`, if any.
pub fn self_ty(sigs: &Signatures, owner: Option<ImplId>) -> Option<Ty> {
    owner.map(|owner| sigs.impls.get(owner).copied().unwrap_or(Ty::ERROR))
}

fn resolve_or_report(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let mut errors = Vec::new();
    let ty = resolve_type(items, module, ty, scope, range, &mut errors);
    diagnostics.extend(
        errors
            .into_iter()
            .map(|(at, message)| Diagnostic::new(at, message)),
    );
    ty
}

/// Whether `ty` contains a reference. References are allowed only as
/// parameters and in local variables so far, where flux can check them
/// without lifetimes.
pub(crate) fn contains_ref(ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Ref(..) => true,
        TyKind::RawPtr(_, pointee) => contains_ref(pointee),
        TyKind::Tuple(elements) | TyKind::Adt(_, elements) => {
            elements.iter().any(|&e| contains_ref(e))
        }
        _ => false,
    }
}

/// Reports names defined twice by blocks for overlapping instances of a
/// generic struct, `impl<T> S<T>` and `impl S<i32>`; the item tree
/// checked other types, and blocks for one generic struct that cannot
/// overlap (`impl S<u8>` and `impl S<i32>`) may share names.
fn duplicate_assoc_items(items: &ItemTree, sigs: &Signatures, diagnostics: &mut Vec<Diagnostic>) {
    let generic: Vec<(ImplId, &Impl)> = items
        .impls
        .iter()
        .filter(|(_, imp)| match imp.target {
            Some(ImplTarget::Struct(s)) => !items.structs[s].generics.is_empty(),
            _ => false,
        })
        .collect();
    let names = |imp: &Impl| -> Vec<(Symbol, TextRange)> {
        let functions = imp.functions.iter().map(|&f| {
            let f = &items.functions[f];
            (f.name, f.def_range)
        });
        let consts = imp.consts.iter().filter_map(|&c| {
            let c = &items.consts[c];
            Some((c.name?, c.def_range))
        });
        functions.chain(consts).collect()
    };
    for (i, &(a, first)) in generic.iter().enumerate() {
        for &(b, second) in &generic[i + 1..] {
            if first.target != second.target || !overlap(sigs.impls[a], sigs.impls[b]) {
                continue;
            }
            let later = names(second);
            // rustc reports the earlier definition.
            for (name, range) in names(first) {
                if later.iter().any(|&(n, _)| n == name) {
                    diagnostics.push(Diagnostic {
                        file: first.file,
                        range,
                        message: format!("duplicate definitions with name `{name}`"),
                    });
                }
            }
        }
    }
}

/// Whether two `impl` blocks' types, each in terms of its own
/// parameters, may be the same type.
fn overlap(a: Ty, b: Ty) -> bool {
    match (a.kind(), b.kind()) {
        (TyKind::Param(_) | TyKind::Error, _) | (_, TyKind::Param(_) | TyKind::Error) => true,
        (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) => {
            x == y && xs.iter().zip(ys).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::Ref(m, x), TyKind::Ref(n, y)) | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
            m == n && overlap(x, y)
        }
        _ => a == b,
    }
}

/// Whether `ty` mentions generic parameter `index`.
fn mentions_param(ty: Ty, index: u32) -> bool {
    if !ty.has_params() {
        return false;
    }
    match ty.kind() {
        TyKind::Param(param) => param.index == index,
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) => mentions_param(t, index),
        TyKind::Tuple(list) | TyKind::Adt(_, list) => {
            list.iter().any(|&t| mentions_param(t, index))
        }
        _ => false,
    }
}

/// The parameters of `generics` as types, numbered from `first`.
pub fn params_of(generics: &Generics, first: usize) -> Vec<Ty> {
    generics
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| Ty::param((first + i) as u32, p.name))
        .collect()
}

/// The signature of every function and the field types of every struct
/// in `items`.
pub fn signatures(items: &ItemTree) -> (Signatures, Vec<Diagnostic>) {
    let mut sigs = Signatures::default();
    let mut diagnostics = Vec::new();
    let mut error = |file, range, message: &str| {
        let mut d = Diagnostic::new(range, message);
        d.file = file;
        diagnostics.push(d);
    };
    let mut reported = Vec::new();
    fn resolve(
        items: &ItemTree,
        loc: ItemLoc,
        ty: &TypeRef,
        scope: TypeScope<'_>,
        range: TextRange,
        reported: &mut Vec<Diagnostic>,
    ) -> Ty {
        let start = reported.len();
        let ty = resolve_or_report(items, loc.module, ty, scope, range, reported);
        for d in &mut reported[start..] {
            d.file = loc.file;
        }
        ty
    }
    for (id, s) in items.structs.iter() {
        let generics: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let scope = TypeScope {
            self_ty: Some(Ty::adt(id, s.name, &params_of(&s.generics, 0))),
            generics: &generics,
            signature_of: Some("structs"),
        };
        let fields: Vec<Ty> = s
            .fields
            .iter()
            .map(|f| resolve(items, s.loc, &f.ty, scope, f.range, &mut reported))
            .collect();
        sigs.structs.insert(id, Ty::list(&fields));
    }
    for (id, imp) in items.impls.iter() {
        let ty = impl_type(items, imp, &mut reported);
        sigs.impls.insert(id, ty);
    }
    for (id, konst) in items.consts.iter() {
        let generics = generic_names(items, konst.owner, &Generics::default());
        let scope = TypeScope {
            self_ty: self_ty(&sigs, konst.owner),
            generics: &generics,
            signature_of: Some("constants"),
        };
        let ty = resolve(
            items,
            konst.loc,
            &konst.ty,
            scope,
            konst.ty_range,
            &mut reported,
        );
        sigs.consts.insert(id, ty);
    }
    for (id, function) in items.functions.iter() {
        let generics = generic_names(items, function.owner, &function.generics);
        let scope = TypeScope {
            self_ty: self_ty(&sigs, function.owner),
            generics: &generics,
            signature_of: Some("functions"),
        };
        let params = function
            .params
            .iter()
            .map(|p| resolve(items, function.loc, &p.ty, scope, p.range, &mut reported))
            .collect();
        let ret = resolve(
            items,
            function.loc,
            &function.ret,
            scope,
            function.ret_range,
            &mut reported,
        );
        sigs.fns.insert(id, FnSig { params, ret });
    }
    for (id, s) in items.structs.iter() {
        // Without `PhantomData`, a parameter must be a field's type.
        for (i, param) in s.generics.params.iter().enumerate() {
            let fields = sigs.structs[id];
            if !fields.iter().any(|&f| mentions_param(f, i as u32)) && !fields.contains(&Ty::ERROR)
            {
                error(
                    s.loc.file,
                    param.range,
                    &format!("type parameter `{}` is never used", param.name),
                );
            }
        }
    }
    duplicate_assoc_items(items, &sigs, &mut reported);
    for (id, imp) in items.impls.iter() {
        let ty = sigs.impls[id];
        for (i, param) in imp.generics.params.iter().enumerate() {
            if ty != Ty::ERROR && !mentions_param(ty, i as u32) {
                error(
                    imp.file,
                    param.range,
                    &format!(
                        "the type parameter `{}` is not constrained by the impl trait, self type, or predicates",
                        param.name
                    ),
                );
            }
        }
    }
    for (id, s) in items.structs.iter() {
        let ty = Ty::adt(id, s.name, &params_of(&s.generics, 0));
        if contains(&sigs, ty, id, &mut Vec::new()) {
            error(
                s.loc.file,
                s.def_range,
                &format!("recursive type `{}` has infinite size", s.name),
            );
        }
        for (field, &ty) in s.fields.iter().zip(sigs.structs[id]) {
            // With lifetime parameters, already reported.
            if contains_ref(ty) && !s.generics.has_lifetimes {
                error(s.loc.file, field.range, "missing lifetime specifier");
            }
        }
    }
    for (id, function) in items.functions.iter() {
        let sig = &sigs.fns[id];
        let params = function.params.iter().zip(&sig.params);
        if function.is_extern {
            for (param, &ty) in params.clone() {
                if contains_ref(ty) {
                    error(
                        function.loc.file,
                        param.range,
                        "passing references to C is not supported yet",
                    );
                }
            }
            let types = params
                .map(|(p, &ty)| (ty, p.range))
                .chain([(sig.ret, function.ret_range)]);
            for (ty, range) in types {
                if !matches!(ty.kind(), TyKind::Tuple([])) && sigs.fields(ty).is_some() {
                    error(
                        function.loc.file,
                        range,
                        "passing structs and tuples to C is not supported yet",
                    );
                }
            }
        } else {
            for (param, &ty) in params {
                if !matches!(ty.kind(), TyKind::Ref(..)) && contains_ref(ty) {
                    error(
                        function.loc.file,
                        param.range,
                        "references inside other types are not supported yet",
                    );
                }
            }
        }
        if contains_ref(sig.ret) {
            if sig.params.iter().any(|&p| contains_ref(p)) {
                error(
                    function.loc.file,
                    function.ret_range,
                    "returning references is not supported yet",
                );
            } else {
                error(
                    function.loc.file,
                    function.ret_range,
                    "missing lifetime specifier",
                );
            }
        }
    }
    diagnostics.extend(reported);
    (sigs, diagnostics)
}

/// Whether a value of `ty`, which is `strukt` itself at the top, contains
/// another `strukt` by value; `seen` guards against cycles that do not
/// involve `strukt`.
fn contains(sigs: &Signatures, ty: Ty, strukt: StructId, seen: &mut Vec<StructId>) -> bool {
    if let TyKind::Adt(adt, _) = ty.kind() {
        if adt.strukt == strukt && !seen.is_empty() {
            return true;
        }
        if seen.contains(&adt.strukt) {
            return false;
        }
        seen.push(adt.strukt);
    }
    let fields = sigs.fields(ty).unwrap_or(&[]);
    fields
        .iter()
        .any(|&field| contains(sigs, field, strukt, seen))
}

/// The type of `impl` block `imp` (`S<T>` in `impl<T> S<T>`), in terms of
/// its parameters; an error if its path did not resolve (reported).
fn impl_type(items: &ItemTree, imp: &Impl, diagnostics: &mut Vec<Diagnostic>) -> Ty {
    let generics: Vec<Symbol> = imp.generics.params.iter().map(|p| p.name).collect();
    let scope = TypeScope {
        self_ty: None,
        generics: &generics,
        signature_of: Some("implementations"),
    };
    let mut errors = Vec::new();
    let ty = match (imp.target, &imp.self_ty) {
        (Some(ImplTarget::Struct(strukt)), TypeRef::Path(path)) => struct_type(
            items,
            imp.module,
            strukt,
            path,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (Some(ImplTarget::Primitive(name)), _) => primitive(name).unwrap_or(Ty::ERROR),
        _ => Ty::ERROR,
    };
    diagnostics.extend(errors.into_iter().map(|(at, message)| Diagnostic {
        file: imp.file,
        range: at,
        message,
    }));
    ty
}

/// Checks that a program's `main` exists and has the right shape.
pub fn check_main(items: &ItemTree, sigs: &Signatures) -> Option<Diagnostic> {
    let krate = items.main_crate();
    let root = items.module_of(krate);
    let main = items.functions.iter().find(|(_, f)| {
        f.name == sym::main && !f.is_extern && f.owner.is_none() && f.loc.module == root
    });
    let Some((id, function)) = main else {
        let name = items.crates[krate.0 as usize].name;
        let mut d = Diagnostic::new(
            TextRange::empty(0),
            format!("`main` function not found in crate `{name}`"),
        );
        d.file = items.modules[root].file;
        return Some(d);
    };
    let sig = &sigs[id];
    let message = if !function.generics.is_empty() {
        let mut d = Diagnostic::new(
            function.generics.range,
            "`main` function is not allowed to have generic parameters",
        );
        d.file = function.loc.file;
        return Some(d);
    } else if function.is_unsafe {
        "`main` function is not allowed to be `unsafe`".to_owned()
    } else if !sig.params.is_empty() {
        "`main` function cannot take parameters".to_owned()
    } else if ![Ty::UNIT, Ty::NEVER, Ty::ERROR].contains(&sig.ret) {
        format!("`main` has invalid return type `{}`", sig.ret)
    } else {
        return None;
    };
    let mut d = Diagnostic::new(function.name_range, message);
    d.file = function.loc.file;
    Some(d)
}

/// A file with every body inferred.
#[derive(Debug)]
pub struct TypedFile {
    pub sigs: Signatures,
    pub bodies: ArenaMap<Function, InferenceResult>,
    pub consts: ArenaMap<Const, InferenceResult>,
    /// From signatures and every body, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Infers every body of `hir`.
pub fn check_file(hir: &Hir) -> TypedFile {
    let (sigs, mut diagnostics) = signatures(&hir.items);
    let mut bodies = ArenaMap::new();
    for (id, (body, source_map)) in hir.bodies.iter() {
        let result = infer_body(&hir.items, &sigs, id, body, source_map);
        let file = hir.items.functions[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
        bodies.insert(id, result);
    }
    let mut consts = ArenaMap::new();
    for (id, (body, source_map)) in hir.const_bodies.iter() {
        let result = infer_const(&hir.items, &sigs, id, body, source_map);
        let file = hir.items.consts[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
        consts.insert(id, result);
    }
    Diagnostic::sort(&mut diagnostics);
    TypedFile {
        sigs,
        bodies,
        consts,
        diagnostics,
    }
}

impl TypedFile {
    /// Each function's signature and the type of each of its bindings,
    /// then the diagnostics. Used by snapshot tests.
    pub fn debug_dump(&self, hir: &Hir) -> String {
        let mut out = String::new();
        let items = &hir.items;
        for (id, s) in items.structs.iter() {
            if !items.in_main_crate(s.loc.module) {
                continue;
            }
            let fields: Vec<String> = s
                .fields
                .iter()
                .zip(self.sigs.structs[id])
                .map(|(f, ty)| format!("{}: {ty}", f.name))
                .collect();
            match fields.as_slice() {
                [] => writeln!(out, "struct {}{} {{}}", s.name, s.generics),
                _ => writeln!(
                    out,
                    "struct {}{} {{ {} }}",
                    s.name,
                    s.generics,
                    fields.join(", ")
                ),
            }
            .unwrap();
        }
        for (id, function) in items.functions.iter() {
            if !hir.is_program(id) {
                continue;
            }
            let sig = &self.sigs[id];
            let params: Vec<String> = sig.params.iter().map(Ty::to_string).collect();
            writeln!(
                out,
                "fn {}{}({}) -> {}",
                function.name,
                function.generics,
                params.join(", "),
                sig.ret
            )
            .unwrap();
            if let (Some((body, _)), Some(result)) = (hir.bodies.get(id), self.bodies.get(id)) {
                for (binding, data) in body.bindings.iter() {
                    let ty = result
                        .binding_types
                        .get(binding)
                        .copied()
                        .unwrap_or(Ty::ERROR);
                    writeln!(out, "    {}@{}: {ty}", data.name, binding.index()).unwrap();
                }
            }
        }
        for (id, konst) in items.consts.iter() {
            if items.in_main_crate(konst.loc.module) {
                let name = konst.name.map_or("_".to_owned(), |n| n.to_string());
                writeln!(out, "const {name}: {}", self.sigs.consts[id]).unwrap();
            }
        }
        hir.write_diagnostics(&mut out, &self.diagnostics);
        out
    }
}

/// `f` as written in messages.
fn function_name(items: &ItemTree, f: FunctionId) -> Symbol {
    items.functions[f].name
}
