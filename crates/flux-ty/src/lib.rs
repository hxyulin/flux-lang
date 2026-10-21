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
    Const, Diagnostic, Function, FunctionId, Hir, ImplId, ImplTarget, ItemLoc, ItemTree, ModuleId,
    Struct, StructId, TypeRef,
};
use flux_intern::{Symbol, sym};
use flux_syntax::TextRange;

pub use infer::{InferenceResult, Method, c_char_ptr, c_str_ref, infer_body, infer_const};
pub use ty::{AdtId, InferVar, IntTy, Mutability, Ty, TyKind};

/// A function's resolved signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub params: Box<[Ty]>,
    pub ret: Ty,
}

/// Everything about a file's items that bodies depend on: each
/// function's signature and each struct's field types.
#[derive(Debug, Default)]
pub struct Signatures {
    pub fns: ArenaMap<Function, FnSig>,
    pub structs: ArenaMap<Struct, Box<[Ty]>>,
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
    pub fn fields(&self, ty: Ty) -> Option<&[Ty]> {
        match ty.kind() {
            TyKind::Tuple(elements) => Some(elements),
            TyKind::Adt(adt) => Some(&self.structs[adt.strukt]),
            _ => None,
        }
    }
}

/// Resolves a type as written in `module`: a struct, a primitive type,
/// or `Self`, which means `self_ty` here, if anything. Errors go to
/// `errors`, at the part of the type they are about within `range`, and
/// make the type an error unless they are about privacy.
pub fn resolve_type(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    self_ty: Option<Ty>,
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
            let last = path.range(path.segments.len().saturating_sub(1), range);
            match resolved {
                Ok(Resolved::SelfType) => match self_ty {
                    Some(ty) => ty,
                    None => fail(
                        errors,
                        range,
                        "cannot find type `Self` in this scope".to_owned(),
                    ),
                },
                Ok(Resolved::Def(Def::Struct(id))) => Ty::adt(id, items.structs[id].name),
                Ok(Resolved::Primitive(name)) => match primitive(name) {
                    Ok(ty) => ty,
                    Err(message) => fail(errors, range, message),
                },
                Ok(Resolved::Def(Def::Module(m))) => fail(
                    errors,
                    last,
                    format!("expected type, found module `{}`", items.modules[m].name),
                ),
                Ok(_) => fail(errors, last, format!("expected type, found `{path}`")),
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
                .map(|f| resolve_type(items, module, f, self_ty, range, errors))
                .collect();
            if fields.contains(&Ty::ERROR) {
                return Ty::ERROR;
            }
            Ty::tuple(&fields)
        }
        TypeRef::Never => Ty::NEVER,
        TypeRef::Ptr { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, self_ty, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::raw_ptr(mutability(*mutable), pointee)
        }
        TypeRef::Ref { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, self_ty, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::new(TyKind::Ref(mutability(*mutable), pointee))
        }
        TypeRef::Error => Ty::ERROR,
    }
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

/// The type of `target`, an `impl` block's type.
pub fn target_ty(items: &ItemTree, target: ImplTarget) -> Ty {
    match target {
        ImplTarget::Struct(s) => Ty::adt(s, items.structs[s].name),
        ImplTarget::Primitive(name) => primitive(name).unwrap_or(Ty::ERROR),
    }
}

/// The `impl` blocks' type for methods on `ty`, if it has any.
pub fn impl_target(ty: Ty) -> Option<ImplTarget> {
    match ty.kind() {
        TyKind::Adt(adt) => Some(ImplTarget::Struct(adt.strukt)),
        TyKind::Int(int) => Some(ImplTarget::Primitive(int.name())),
        TyKind::Bool => Some(ImplTarget::Primitive(sym::bool)),
        _ => None,
    }
}

/// What `Self` means in an item in the `impl` block `owner`, if any.
pub fn self_ty(items: &ItemTree, owner: Option<ImplId>) -> Option<Ty> {
    let target = owner.map(|owner| items.impls[owner].target)?;
    Some(target.map_or(Ty::ERROR, |t| target_ty(items, t)))
}

fn resolve_or_report(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    self_ty: Option<Ty>,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let mut errors = Vec::new();
    let ty = resolve_type(items, module, ty, self_ty, range, &mut errors);
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
        TyKind::Tuple(elements) => elements.iter().any(|&e| contains_ref(e)),
        _ => false,
    }
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
    let mut resolve = |loc: ItemLoc, ty, self_ty, range| {
        let start = reported.len();
        let ty = resolve_or_report(items, loc.module, ty, self_ty, range, &mut reported);
        for d in &mut reported[start..] {
            d.file = loc.file;
        }
        ty
    };
    for (id, s) in items.structs.iter() {
        let self_ty = Some(Ty::adt(id, s.name));
        let fields = s
            .fields
            .iter()
            .map(|f| resolve(s.loc, &f.ty, self_ty, f.range))
            .collect();
        sigs.structs.insert(id, fields);
    }
    for (id, konst) in items.consts.iter() {
        let self_ty = self_ty(items, konst.owner);
        let ty = resolve(konst.loc, &konst.ty, self_ty, konst.ty_range);
        sigs.consts.insert(id, ty);
    }
    for (id, function) in items.functions.iter() {
        let self_ty = self_ty(items, function.owner);
        let params = function
            .params
            .iter()
            .map(|p| resolve(function.loc, &p.ty, self_ty, p.range))
            .collect();
        let ret = resolve(function.loc, &function.ret, self_ty, function.ret_range);
        sigs.fns.insert(id, FnSig { params, ret });
    }
    for (id, s) in items.structs.iter() {
        if contains(&sigs, Ty::adt(id, s.name), id, &mut Vec::new()) {
            error(
                s.loc.file,
                s.def_range,
                &format!("recursive type `{}` has infinite size", s.name),
            );
        }
        for (field, &ty) in s.fields.iter().zip(&sigs.structs[id]) {
            if contains_ref(ty) {
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
    if let TyKind::Adt(adt) = ty.kind() {
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
    let message = if function.is_unsafe {
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
                .zip(&self.sigs.structs[id])
                .map(|(f, ty)| format!("{}: {ty}", f.name))
                .collect();
            match fields.as_slice() {
                [] => writeln!(out, "struct {} {{}}", s.name),
                _ => writeln!(out, "struct {} {{ {} }}", s.name, fields.join(", ")),
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
                "fn {}({}) -> {}",
                function.name,
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
