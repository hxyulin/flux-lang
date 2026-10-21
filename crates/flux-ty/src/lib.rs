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

use flux_arena::ArenaMap;
use flux_hir::{Diagnostic, FileHir, Function, FunctionId, ItemTree, TypeRef};
use flux_intern::{Symbol, sym};
use flux_syntax::TextRange;

pub use infer::{InferenceResult, c_char_ptr, c_str_ref, infer_body};
pub use ty::{InferVar, IntTy, Mutability, Ty, TyKind};

/// A function's resolved signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub params: Box<[Ty]>,
    pub ret: Ty,
}

/// Resolves a type as written. Only primitive types exist so far.
pub fn resolve_type(ty: &TypeRef) -> Result<Ty, String> {
    match ty {
        TypeRef::Path(name) if *name == sym::bool => Ok(Ty::BOOL),
        TypeRef::Path(name) => match IntTy::from_name(*name) {
            Some(int) => Ok(Ty::int(int)),
            None if [sym::char, sym::str, sym::f32, sym::f64].contains(name) => {
                Err(format!("`{name}` is not supported yet"))
            }
            None => Err(format!("cannot find type `{name}` in this scope")),
        },
        TypeRef::Tuple(fields) if fields.is_empty() => Ok(Ty::UNIT),
        TypeRef::Tuple(_) => Err("tuples are not supported yet".to_owned()),
        TypeRef::Never => Ok(Ty::NEVER),
        TypeRef::Ptr { mutable, pointee } => {
            let mutability = if *mutable {
                Mutability::Mut
            } else {
                Mutability::Not
            };
            Ok(Ty::raw_ptr(mutability, resolve_type(pointee)?))
        }
        TypeRef::Error => Ok(Ty::ERROR),
    }
}

fn resolve_or_report(ty: &TypeRef, range: TextRange, diagnostics: &mut Vec<Diagnostic>) -> Ty {
    resolve_type(ty).unwrap_or_else(|message| {
        diagnostics.push(Diagnostic { range, message });
        Ty::ERROR
    })
}

/// The signature of every function in `items`.
pub fn signatures(items: &ItemTree) -> (ArenaMap<Function, FnSig>, Vec<Diagnostic>) {
    let mut sigs = ArenaMap::new();
    let mut diagnostics = Vec::new();
    for (id, function) in items.functions.iter() {
        let params = function
            .params
            .iter()
            .map(|p| resolve_or_report(&p.ty, p.range, &mut diagnostics))
            .collect();
        let ret = resolve_or_report(&function.ret, function.ret_range, &mut diagnostics);
        sigs.insert(id, FnSig { params, ret });
    }
    (sigs, diagnostics)
}

/// Checks that a program's `main` exists and has the right shape.
pub fn check_main(items: &ItemTree, sigs: &ArenaMap<Function, FnSig>) -> Option<Diagnostic> {
    let main = items
        .functions
        .iter()
        .find(|(_, f)| f.name == sym::main && !f.is_extern);
    let Some((id, function)) = main else {
        return Some(Diagnostic {
            range: TextRange::empty(0),
            message: "`main` function not found".to_owned(),
        });
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
    Some(Diagnostic {
        range: function.name_range,
        message,
    })
}

/// A file with every body inferred.
#[derive(Debug)]
pub struct TypedFile {
    pub sigs: ArenaMap<Function, FnSig>,
    pub bodies: ArenaMap<Function, InferenceResult>,
    /// From signatures and every body, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Infers every body of `hir`.
pub fn check_file(hir: &FileHir) -> TypedFile {
    let (sigs, mut diagnostics) = signatures(&hir.items);
    let mut bodies = ArenaMap::new();
    for (id, (body, source_map)) in hir.bodies.iter() {
        let result = infer_body(&hir.items, &sigs, id, body, source_map);
        diagnostics.extend(result.diagnostics.iter().cloned());
        bodies.insert(id, result);
    }
    diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
    TypedFile {
        sigs,
        bodies,
        diagnostics,
    }
}

impl TypedFile {
    /// Each function's signature and the type of each of its bindings,
    /// then the diagnostics. Used by snapshot tests.
    pub fn debug_dump(&self, hir: &FileHir) -> String {
        let mut out = String::new();
        for (id, function) in hir.items.functions.iter() {
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
        for d in &self.diagnostics {
            writeln!(out, "error {:?}: {}", d.range, d.message).unwrap();
        }
        out
    }
}

/// `f` as written in messages.
fn function_name(items: &ItemTree, f: FunctionId) -> Symbol {
    items.functions[f].name
}
