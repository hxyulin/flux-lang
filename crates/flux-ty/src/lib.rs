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
use flux_hir::{Diagnostic, FileHir, Function, FunctionId, ItemTree, Struct, StructId, TypeRef};
use flux_intern::{Symbol, sym};
use flux_syntax::TextRange;

pub use infer::{InferenceResult, c_char_ptr, c_str_ref, infer_body};
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

/// Resolves a type as written: a primitive type or one of `items`'
/// structs, which shadow primitive types of the same name.
pub fn resolve_type(items: &ItemTree, ty: &TypeRef) -> Result<Ty, String> {
    match ty {
        TypeRef::Path(name) => {
            if let Some((id, s)) = items.struct_by_name(*name) {
                return Ok(Ty::adt(id, s.name));
            }
            if *name == sym::bool {
                return Ok(Ty::BOOL);
            }
            match IntTy::from_name(*name) {
                Some(int) => Ok(Ty::int(int)),
                None if [sym::char, sym::str, sym::f32, sym::f64].contains(name) => {
                    Err(format!("`{name}` is not supported yet"))
                }
                None => Err(format!("cannot find type `{name}` in this scope")),
            }
        }
        TypeRef::Tuple(fields) => {
            let fields = fields
                .iter()
                .map(|f| resolve_type(items, f))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Ty::tuple(&fields))
        }
        TypeRef::Never => Ok(Ty::NEVER),
        TypeRef::Ptr { mutable, pointee } => {
            let mutability = if *mutable {
                Mutability::Mut
            } else {
                Mutability::Not
            };
            Ok(Ty::raw_ptr(mutability, resolve_type(items, pointee)?))
        }
        TypeRef::Error => Ok(Ty::ERROR),
    }
}

fn resolve_or_report(
    items: &ItemTree,
    ty: &TypeRef,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    resolve_type(items, ty).unwrap_or_else(|message| {
        diagnostics.push(Diagnostic { range, message });
        Ty::ERROR
    })
}

/// The signature of every function and the field types of every struct
/// in `items`.
pub fn signatures(items: &ItemTree) -> (Signatures, Vec<Diagnostic>) {
    let mut sigs = Signatures::default();
    let mut diagnostics = Vec::new();
    let mut resolve = |ty, range| resolve_or_report(items, ty, range, &mut diagnostics);
    for (id, s) in items.structs.iter() {
        let fields = s.fields.iter().map(|f| resolve(&f.ty, f.range)).collect();
        sigs.structs.insert(id, fields);
    }
    for (id, function) in items.functions.iter() {
        let params = function
            .params
            .iter()
            .map(|p| resolve(&p.ty, p.range))
            .collect();
        let ret = resolve(&function.ret, function.ret_range);
        sigs.fns.insert(id, FnSig { params, ret });
    }
    for (id, s) in items.structs.iter() {
        if contains(&sigs, Ty::adt(id, s.name), id, &mut Vec::new()) {
            diagnostics.push(Diagnostic {
                range: s.def_range,
                message: format!("recursive type `{}` has infinite size", s.name),
            });
        }
    }
    for function in items.functions.values() {
        if function.is_extern {
            let types = function
                .params
                .iter()
                .map(|p| (&p.ty, p.range))
                .chain([(&function.ret, function.ret_range)]);
            for (ty, range) in types {
                let ty = resolve_type(items, ty).unwrap_or(Ty::ERROR);
                if !matches!(ty.kind(), TyKind::Tuple([])) && sigs.fields(ty).is_some() {
                    diagnostics.push(Diagnostic {
                        range,
                        message: "passing structs and tuples to C is not supported yet".to_owned(),
                    });
                }
            }
        }
    }
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
    pub sigs: Signatures,
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
        for (id, s) in hir.items.structs.iter() {
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
