//! Lifetimes in signatures, for the borrow checker.
//!
//! Types are checked without lifetimes: a [`Ty`] has none, and bodies
//! are inferred as if every reference had the same one, as rustc's type
//! checking erases them. What the borrow checker needs of lifetimes is
//! here, beside the types of signatures: the lifetime of each *region
//! slot* of each type in each function's, struct's and `impl` block's
//! signature. A type's slots are, in order: a reference's own, then its
//! pointee's; a struct's or enum's lifetime arguments, then its type
//! arguments' slots; a tuple's, array's or raw pointer's parts'; and a
//! closure's captures'. A generic parameter, an associated type, and a
//! function item or pointer have none: an instance of a signature has
//! the slots of its types with the arguments substituted, each
//! parameter's at its place ([`instantiate`]).
//!
//! A signature's lifetimes are its parameters (its owner's, then its
//! own, then one for each lifetime elided in a function's inputs or an
//! `impl` block's header), `'static`, or, in a body, elided ones the
//! borrow checker infers. Elision follows rustc: in a function's return
//! type, an elided lifetime is that of `&self` or `&mut self`, or the
//! only one in the inputs (E0106 otherwise); in a constant's or
//! static's type it is `'static`.

use flux_arena::ArenaMap;
use flux_hir::def_map::{Def, Ns, Resolved};
use flux_hir::{
    Const, Function, GenericArgs, Generics, Impl, ItemTree, LifetimeRef, ModuleId, Path, Struct,
    TypeAlias, TypeAliasId, TypeRef,
};
use flux_intern::{Symbol, kw};
use flux_syntax::TextRange;

use crate::{ClosureArgs, Signatures, Ty, TyKind, TypeScope};

mod resolve;

pub(crate) use resolve::resolve_regions;

/// A lifetime in a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Static,
    /// The item's lifetime parameter with this index: its owner's
    /// first, then its own, then those its elided lifetimes stand for.
    Param(u32),
    /// Elided in a body, or erroneous (reported): the borrow checker
    /// infers it.
    Erased,
}

/// How a type's subtyping depends on one of its parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variance {
    /// Not used: anything goes (and an error for a struct's parameter).
    Bivariant,
    Covariant,
    Contravariant,
    Invariant,
}

impl Variance {
    /// The variance of a use with variance `inner` inside a position
    /// with variance `self`.
    pub fn xform(self, inner: Variance) -> Variance {
        match (self, inner) {
            (Variance::Covariant, v) => v,
            (Variance::Contravariant, v) => v.flip(),
            (Variance::Invariant, Variance::Bivariant) => Variance::Bivariant,
            (Variance::Invariant, _) => Variance::Invariant,
            (Variance::Bivariant, _) => Variance::Bivariant,
        }
    }

    pub fn flip(self) -> Variance {
        match self {
            Variance::Covariant => Variance::Contravariant,
            Variance::Contravariant => Variance::Covariant,
            v => v,
        }
    }

    /// The variance of a parameter used both ways.
    pub fn join(self, other: Variance) -> Variance {
        match (self, other) {
            (Variance::Bivariant, v) | (v, Variance::Bivariant) => v,
            (a, b) if a == b => a,
            _ => Variance::Invariant,
        }
    }
}

/// `'a: 'b`, or `T: 'a`: what one lifetime or type outlives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outlives<R> {
    Region(R, R),
    /// A type without slots of its own (a parameter or an associated
    /// type) outlives a lifetime.
    Type(Ty, R),
}

/// A lifetime parameter's name, `None` for one an elided lifetime
/// stands for, and where it is declared (or elided).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifetimeName {
    pub name: Option<Symbol>,
    pub range: TextRange,
}

/// The lifetimes of a function's signature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FnRegions {
    /// Its lifetime parameters, its owner's first.
    pub names: Box<[LifetimeName]>,
    /// How many of `names` are its owner's.
    pub owner: u32,
    pub params: Box<[Box<[Region]>]>,
    pub ret: Box<[Region]>,
    /// What its lifetimes outlive: its bounds and its owner's, and what
    /// its parameter and return types imply (`&'a T` that `T: 'a`).
    /// The body may assume them, and a call must meet them.
    pub outlives: Box<[Outlives<Region>]>,
    /// Has bounds on lifetimes of its own: `where 'a: 'b`.
    pub has_bounds: bool,
    /// The slots of the types its own bounds are on (`&'a T` in `where
    /// &'a T: Tr`), as [`Signatures::fn_predicates`] lists them.
    pub bound_selfs: Box<[Box<[Region]>]>,
    /// The slots of what its own bounds say associated types are
    /// (`I: Iterator<Item = &'a T>`), as [`Signatures::fn_projections`]
    /// lists them.
    pub projections: Box<[Box<[Region]>]>,
    /// Its opaque types', by index.
    pub opaques: Box<[OpaqueRegions]>,
}

/// The lifetimes of an opaque type (an `impl Trait` in a function's
/// return type), in terms of its function's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpaqueRegions {
    /// Those it captures, which are its region slots: in edition 2024,
    /// all of its function's, or those `use<..>` lists.
    pub captures: Box<[Region]>,
    /// `'a` in `impl Tr + 'a`: what its hidden type outlives.
    pub outlives: Box<[Region]>,
    /// The slots of what its bounds say associated types are
    /// (`impl Iterator<Item = &'a T>`), as [`crate::OpaqueSig`] lists
    /// them; none where they cannot be matched, which only errors cause.
    pub projections: Box<[Box<[Region]>]>,
}

/// The lifetimes of a struct's or enum's fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructRegions {
    /// How many lifetime parameters it has.
    pub lifetimes: u32,
    /// The slots of each field's type, as [`Signatures::structs`] lists
    /// them.
    pub fields: Box<[Box<[Region]>]>,
    /// Its lifetime parameters', then its type parameters'.
    pub variances: Box<[Variance]>,
    /// What its parameters outlive, as its bounds say and its fields'
    /// types imply (rustc's inferred outlives requirements), in terms of
    /// its parameters: what a well-formed instance of it implies.
    pub outlives: Box<[Outlives<Region>]>,
}

/// The lifetimes of an `impl` block's header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImplRegions {
    pub names: Box<[LifetimeName]>,
    pub self_ty: Box<[Region]>,
    /// The slots of each of its trait's arguments, after `Self`.
    pub trait_args: Box<[Box<[Region]>]>,
    /// Each associated type's.
    pub types: Box<[(Symbol, Box<[Region]>)]>,
    /// Its bounds, and what its header's types imply: what its items
    /// may assume.
    pub outlives: Box<[Outlives<Region>]>,
    /// Its bounds alone (`'b: 'a`, `T: 'a`): what a use of it must
    /// meet.
    pub bounds: Box<[Outlives<Region>]>,
    /// The slots of the types its bounds are on, as
    /// [`Signatures::impl_predicates`] lists them.
    pub bound_selfs: Box<[Box<[Region]>]>,
    /// The slots of what its bounds say associated types are, as
    /// [`Signatures::impl_projections`] lists them.
    pub projections: Box<[Box<[Region]>]>,
}

/// A type alias as a type and its slots, in terms of its parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AliasRegions {
    pub ty: Option<Ty>,
    pub regions: Box<[Region]>,
}

/// The lifetimes of every signature.
#[derive(Debug, Clone, Default)]
pub struct RegionSigs {
    pub fns: ArenaMap<Function, FnRegions>,
    pub structs: ArenaMap<Struct, StructRegions>,
    pub impls: ArenaMap<Impl, ImplRegions>,
    pub consts: ArenaMap<Const, Box<[Region]>>,
}

impl RegionSigs {
    /// Struct `strukt`'s lifetimes, if known.
    pub fn strukt(&self, strukt: flux_hir::StructId) -> Option<&StructRegions> {
        self.structs.get(strukt)
    }
}

/// How many region slots `ty` has.
pub fn slots(items: &ItemTree, ty: Ty) -> usize {
    match ty.kind() {
        TyKind::Ref(_, t) => 1 + slots(items, t),
        TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => slots(items, t),
        TyKind::Tuple(list) => list.iter().map(|&t| slots(items, t)).sum(),
        TyKind::Adt(adt, args) => {
            items.structs[adt.strukt].generics.lifetimes.len()
                + args.iter().map(|&t| slots(items, t)).sum::<usize>()
        }
        TyKind::Closure(_, args) => slots(items, ClosureArgs::split(args).upvars),
        TyKind::Opaque(opaque) => {
            opaque.lifetimes as usize + opaque.args.iter().map(|&t| slots(items, t)).sum::<usize>()
        }
        _ => 0,
    }
}

/// The slots of an instance of `ty`, a type in some item's terms whose
/// own slots are `own`: `region` maps each of them, and type parameter
/// `j`'s slots are `args(j)`'s. Into `out`.
pub fn instantiate<R: Copy>(
    items: &ItemTree,
    ty: Ty,
    own: &[Region],
    region: &mut dyn FnMut(Region) -> R,
    args: &dyn Fn(u32) -> Vec<R>,
    out: &mut Vec<R>,
) {
    let mut next = 0;
    instantiate_at(items, ty, own, &mut next, region, args, out);
}

fn instantiate_at<R: Copy>(
    items: &ItemTree,
    ty: Ty,
    own: &[Region],
    next: &mut usize,
    region: &mut dyn FnMut(Region) -> R,
    args: &dyn Fn(u32) -> Vec<R>,
    out: &mut Vec<R>,
) {
    let mut own_slot = |next: &mut usize, out: &mut Vec<R>| {
        let r = own.get(*next).copied().unwrap_or(Region::Erased);
        *next += 1;
        out.push(region(r));
    };
    match ty.kind() {
        TyKind::Param(p) => out.extend(args(p.index)),
        TyKind::Ref(_, t) => {
            own_slot(next, out);
            instantiate_at(items, t, own, next, region, args, out);
        }
        TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            instantiate_at(items, t, own, next, region, args, out)
        }
        TyKind::Tuple(list) => {
            for &t in list {
                instantiate_at(items, t, own, next, region, args, out);
            }
        }
        TyKind::Adt(adt, list) => {
            for _ in 0..items.structs[adt.strukt].generics.lifetimes.len() {
                own_slot(next, out);
            }
            for &t in list {
                instantiate_at(items, t, own, next, region, args, out);
            }
        }
        TyKind::Closure(_, list) => {
            let upvars = ClosureArgs::split(list).upvars;
            instantiate_at(items, upvars, own, next, region, args, out);
        }
        TyKind::Opaque(opaque) => {
            for _ in 0..opaque.lifetimes {
                own_slot(next, out);
            }
            for &t in opaque.args {
                instantiate_at(items, t, own, next, region, args, out);
            }
        }
        _ => {}
    }
}

/// What an elided lifetime (`&T`, `'_`, or a struct's lifetime
/// arguments left out) is where a type is written.
#[derive(Debug, Clone)]
pub(crate) enum Elided {
    /// A new lifetime parameter: in a function's inputs and an `impl`
    /// block's header.
    Fresh,
    /// This one: a function's return type's, a constant's (`'static`),
    /// or one to infer in a body.
    Is(Region),
    /// Missing (E0106), with rustc's help.
    Missing(Option<String>),
    /// Not allowed (E0637): in a bound's generic arguments.
    NotAllowed,
}

/// Finds the regions of types as written, beside their resolved types.
pub(crate) struct Walker<'a> {
    pub items: &'a ItemTree,
    pub module: ModuleId,
    /// The lifetime parameters in scope, by index; `None` for one an
    /// elided lifetime stands for, which cannot be named.
    pub names: Vec<Option<Symbol>>,
    /// What `Self` is, if it has slots.
    pub self_regions: Option<&'a [Region]>,
    /// In a trait's `impl` block's methods, the slots of its associated
    /// types, which `Self::Out` resolves to.
    pub self_types: &'a [(Symbol, Box<[Region]>)],
    /// For resolving the types an alias is given.
    pub scope: TypeScope<'a>,
    pub elided: Elided,
    /// The lifetime parameters elided lifetimes made, after `names`.
    pub fresh: Vec<TextRange>,
    /// In a function's return type, the lifetimes each of its opaque
    /// types captures, which are their slots.
    pub opaque_captures: Vec<Box<[Region]>>,
    pub errors: Vec<(TextRange, String)>,
    /// The region of the reference whose pointee is being walked, if
    /// that is what is: a trait object's lifetime bound there.
    ref_region: Option<Region>,
}

impl<'a> Walker<'a> {
    pub fn new(items: &'a ItemTree, module: ModuleId, names: Vec<Option<Symbol>>) -> Self {
        Walker {
            items,
            module,
            names,
            self_regions: None,
            self_types: &[],
            scope: TypeScope::default(),
            elided: Elided::Is(Region::Erased),
            fresh: Vec::new(),
            opaque_captures: Vec::new(),
            errors: Vec::new(),
            ref_region: None,
        }
    }

    /// The slots of `ty`, written as `written`.
    pub fn regions(&mut self, written: &TypeRef, ty: Ty) -> Box<[Region]> {
        let mut out = Vec::new();
        self.walk(written, ty, &mut out);
        // A type that did not resolve as written (an error, reported)
        // has its slots inferred.
        let want = slots(self.items, ty);
        out.resize(want, Region::Erased);
        out.into()
    }

    /// The region a lifetime written as `lifetime` is.
    pub fn named(&mut self, lifetime: LifetimeRef) -> Region {
        if lifetime.is_static() {
            return Region::Static;
        }
        if lifetime.is_anonymous() {
            return self.elide(lifetime.range);
        }
        if let Some(i) = self.names.iter().rposition(|&n| n == Some(lifetime.name)) {
            return Region::Param(i as u32);
        }
        let message = if outer_lifetime(self.items, self.module, lifetime.name) {
            "can't use generic parameters from outer item".to_owned()
        } else {
            format!("use of undeclared lifetime name `{}`", lifetime.name)
        };
        self.errors.push((lifetime.range, message));
        Region::Erased
    }

    /// The region an elided lifetime at `range` is.
    fn elide(&mut self, range: TextRange) -> Region {
        match &self.elided {
            Elided::Fresh => {
                let index = self.names.len() + self.fresh.len();
                self.fresh.push(range);
                Region::Param(index as u32)
            }
            Elided::Is(r) => *r,
            Elided::Missing(help) => {
                let mut message = "missing lifetime specifier".to_owned();
                if let Some(help) = help {
                    message = format!("{message}\nhelp: {help}");
                }
                self.errors.push((range, message));
                Region::Erased
            }
            Elided::NotAllowed => {
                self.errors.push((
                    range,
                    "`&` without an explicit lifetime name cannot be used here".to_owned(),
                ));
                Region::Erased
            }
        }
    }

    fn walk(&mut self, written: &TypeRef, ty: Ty, out: &mut Vec<Region>) {
        let enclosing = self.ref_region.take();
        match (written, ty.kind()) {
            (_, TyKind::Error) => {}
            (
                TypeRef::Ref {
                    lifetime,
                    amp,
                    pointee,
                    ..
                },
                TyKind::Ref(_, inner),
            ) => {
                let r = match lifetime {
                    Some(l) if l.is_anonymous() => self.elide(l.range),
                    Some(l) => self.named(*l),
                    None => self.elide(*amp),
                };
                out.push(r);
                self.ref_region = Some(r);
                self.walk(pointee, inner, out);
                self.ref_region = None;
            }
            // A trait object has no slots: its lifetime bound is the
            // reference it is behind (rustc's default), or `'static`
            // behind other pointers, so that is all it may be written
            // as.
            (TypeRef::Dyn(d), _) => {
                if let Some(l) = d.lifetime {
                    let r = self.named(l);
                    let default = enclosing.unwrap_or(Region::Static);
                    if r != default && r != Region::Erased {
                        self.errors.push((
                            l.range,
                            "lifetime bounds on trait objects other than their reference's lifetime are not supported yet"
                                .to_owned(),
                        ));
                    }
                }
            }
            (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(_, inner))
            | (TypeRef::Array { elem: pointee, .. }, TyKind::Array(inner, _))
            | (TypeRef::Slice(pointee), TyKind::Slice(inner)) => self.walk(pointee, inner, out),
            (TypeRef::Tuple(elements), TyKind::Tuple(types)) => {
                for (e, &t) in elements.iter().zip(types) {
                    self.walk(e, t, out);
                }
            }
            (TypeRef::FnPtr { params, ret, .. }, _) => self.fn_ptr(params, ret),
            (TypeRef::Opaque(i), TyKind::Opaque(opaque)) => {
                match self.opaque_captures.get(*i as usize) {
                    Some(captures) => out.extend_from_slice(captures),
                    None => out.extend((0..opaque.lifetimes).map(|_| Region::Erased)),
                }
            }
            (TypeRef::Path(path), _) => self.path(path, ty, out),
            _ => {}
        }
    }

    /// A function pointer's lifetimes are its own (higher-ranked): flux
    /// supports elided ones in its parameters only.
    fn fn_ptr(&mut self, params: &[TypeRef], ret: &TypeRef) {
        let mut named = Vec::new();
        for p in params {
            lifetimes_in(p, &mut named);
        }
        let mut in_ret = Vec::new();
        lifetimes_in(ret, &mut in_ret);
        let mut elided_ret = Vec::new();
        elided_in(ret, &mut elided_ret);
        if let Some(l) = named.iter().chain(&in_ret).find(|l| !l.is_anonymous()) {
            self.errors.push((
                l.range,
                "lifetimes in function pointer types are not supported yet".to_owned(),
            ));
        } else if let Some(&at) = elided_ret.first() {
            self.errors.push((
                at,
                "function pointer types returning references are not supported yet".to_owned(),
            ));
        }
    }

    fn path(&mut self, path: &Path, ty: Ty, out: &mut Vec<Region>) {
        if matches!(ty.kind(), TyKind::Param(_) | TyKind::Alias(_)) {
            self.check_args(path);
            return;
        }
        // `Self::Out` or `<Self as Tr>::Out`, which the type resolved to
        // the block's own (otherwise it is an associated type).
        let is_self = |p: &Path| p.as_name() == Some(kw::SelfUpper);
        let own = match (&path.qself, &*path.segments) {
            (None, [first, name]) if *first == kw::SelfUpper => Some(*name),
            (Some(q), [name]) if matches!(&q.ty, TypeRef::Path(p) if is_self(p)) => Some(*name),
            _ => None,
        };
        if let Some(name) = own
            && let Some((_, regions)) = self.self_types.iter().find(|(n, _)| *n == name)
        {
            self.check_args(path);
            out.extend_from_slice(regions);
            return;
        }
        let resolved = match self.items.resolve(self.module, path, Ns::Types) {
            Ok(r) => r,
            Err(flux_hir::def_map::ResolveError::Private { resolved, .. }) => resolved,
            Err(_) => return,
        };
        let last = path.segments.len().saturating_sub(1);
        let args = path.args(last);
        match resolved {
            Resolved::SelfType => {
                if let Some(regions) = self.self_regions {
                    out.extend_from_slice(regions);
                }
            }
            Resolved::Def(Def::Struct(id)) => {
                let def = &self.items.structs[id];
                let kind = if def.is_enum() { "enum" } else { "struct" };
                let want = def.generics.lifetimes.len();
                self.lifetime_args(path, kind, want, out);
                if let (Some(args), TyKind::Adt(_, types)) = (args, ty.kind()) {
                    for (written, &t) in args.types.iter().zip(types) {
                        self.walk(written, t, out);
                    }
                }
            }
            Resolved::Def(Def::TypeAlias(id)) => self.alias(path, id, out),
            _ => self.check_args(path),
        }
    }

    /// The lifetime arguments of `path`, a `kind` with `want` lifetime
    /// parameters: those written, or elided ones.
    fn lifetime_args(&mut self, path: &Path, kind: &str, want: usize, out: &mut Vec<Region>) {
        let last = path.segments.len().saturating_sub(1);
        let whole = path.ranges.get(last).copied().unwrap_or_default();
        let written: &[LifetimeRef] = path.args(last).map_or(&[], |a| &a.lifetimes);
        if written.is_empty() {
            // rustc points after the name, where they would go.
            let at = match path.args(last) {
                Some(args) => TextRange::new(args.range.start(), args.range.start() + 1),
                None => path.range(last, whole),
            };
            for _ in 0..want {
                let r = self.elide(at);
                out.push(r);
            }
            return;
        }
        if written.len() != want {
            let verb = if written.len() == 1 { "was" } else { "were" };
            let plural = |n: usize| if n == 1 { "" } else { "s" };
            self.errors.push((
                path.range(last, whole),
                format!(
                    "{kind} takes {want} lifetime argument{} but {} lifetime argument{} {verb} supplied",
                    plural(want),
                    written.len(),
                    plural(written.len())
                ),
            ));
            out.extend(std::iter::repeat_n(Region::Erased, want));
            return;
        }
        for &l in written {
            let r = self.named(l);
            out.push(r);
        }
    }

    /// Lifetime arguments where there are no lifetime parameters
    /// (E0107), and those in the type arguments, checked.
    fn check_args(&mut self, path: &Path) {
        for (i, args) in path.args.iter().enumerate() {
            let Some(args) = args else { continue };
            if let Some(&first) = args.lifetimes.first() {
                let n = args.lifetimes.len();
                let what = if i + 1 == path.segments.len() {
                    "type"
                } else {
                    "item"
                };
                self.errors.push((
                    first.range,
                    format!(
                        "{what} takes 0 lifetime arguments but {n} lifetime argument{} {} supplied",
                        if n == 1 { "" } else { "s" },
                        if n == 1 { "was" } else { "were" }
                    ),
                ));
            }
            self.check_written(args);
        }
    }

    /// The lifetimes written in `args`' types, without types to match:
    /// each must be in scope.
    fn check_written(&mut self, args: &GenericArgs) {
        let mut found = Vec::new();
        for t in args.types.iter() {
            lifetimes_in(t, &mut found);
        }
        for l in found {
            if !l.is_anonymous() {
                self.named(l);
            }
        }
    }

    /// Type alias `id` named by `path`: its slots with its parameters'
    /// substituted.
    fn alias(&mut self, path: &Path, id: TypeAliasId, out: &mut Vec<Region>) {
        let def = &self.items.type_aliases[id];
        let last = path.segments.len().saturating_sub(1);
        let mut lifetimes = Vec::new();
        self.lifetime_args(
            path,
            "type alias",
            def.generics.lifetimes.len(),
            &mut lifetimes,
        );
        let table = alias_regions(self.items, id, &mut Vec::new());
        let Some(alias_ty) = table.ty else { return };
        let mut types: Vec<Vec<Region>> = Vec::new();
        if let Some(args) = path.args(last) {
            for (written, &range) in args.types.iter().zip(&args.ranges) {
                let mut scratch = Vec::new();
                let t = crate::resolve_type(
                    self.items,
                    self.module,
                    written,
                    self.scope,
                    range,
                    &mut scratch,
                );
                let mut regions = Vec::new();
                self.walk(written, t, &mut regions);
                types.push(regions);
            }
        }
        let mut map = |r: Region| match r {
            Region::Param(i) => lifetimes.get(i as usize).copied().unwrap_or(Region::Erased),
            r => r,
        };
        let args = |j: u32| types.get(j as usize).cloned().unwrap_or_default();
        instantiate(self.items, alias_ty, &table.regions, &mut map, &args, out);
    }
}

/// Type alias `id`'s type and slots in terms of its parameters, with the
/// errors in its lifetimes.
pub(crate) fn alias_regions(
    items: &ItemTree,
    id: TypeAliasId,
    errors: &mut Vec<(TextRange, String)>,
) -> AliasRegions {
    thread_local! {
        static ACTIVE: std::cell::RefCell<Vec<TypeAliasId>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    if ACTIVE.with(|a| a.borrow().contains(&id)) {
        return AliasRegions::default();
    }
    ACTIVE.with(|a| a.borrow_mut().push(id));
    let def: &TypeAlias = &items.type_aliases[id];
    let names: Vec<Option<Symbol>> = def
        .generics
        .lifetimes
        .iter()
        .map(|l| Some(l.name))
        .collect();
    let generics: Vec<Symbol> = def.generics.params.iter().map(|p| p.name).collect();
    let scope = TypeScope {
        generics: &generics,
        signature_of: Some("type aliases"),
        ..TypeScope::default()
    };
    let mut scratch = Vec::new();
    let ty = crate::resolve_type(
        items,
        def.loc.module,
        &def.ty,
        scope,
        def.ty_range,
        &mut scratch,
    );
    let mut walker = Walker::new(items, def.loc.module, names);
    walker.scope = scope;
    walker.elided = Elided::Missing(None);
    let regions = walker.regions(&def.ty, ty);
    errors.append(&mut walker.errors);
    ACTIVE.with(|a| a.borrow_mut().pop());
    AliasRegions {
        ty: Some(ty),
        regions,
    }
}

/// Whether `name` is a lifetime parameter of a function whose body has
/// the block `module` is, or is in: which an item declared there cannot
/// use (E0401).
fn outer_lifetime(items: &ItemTree, module: ModuleId, name: Symbol) -> bool {
    let mut module = Some(module);
    while let Some(m) = module {
        let data = &items.modules[m];
        module = data.parent;
        let Some(f) = data.block.and_then(|b| b.function) else {
            continue;
        };
        let f = &items.functions[f];
        let owner = items.owner_generics(f.owner);
        if owner
            .into_iter()
            .chain([&f.generics])
            .any(|g| g.lifetimes.iter().any(|l| l.name == name))
        {
            return true;
        }
    }
    false
}

/// The lifetimes written in `ty`, into `out`.
pub(crate) fn lifetimes_in(ty: &TypeRef, out: &mut Vec<LifetimeRef>) {
    match ty {
        TypeRef::Ref {
            lifetime, pointee, ..
        } => {
            out.extend(lifetime.iter().copied());
            lifetimes_in(pointee, out);
        }
        TypeRef::Path(path) => {
            for args in path.args.iter().flatten() {
                out.extend(args.lifetimes.iter().copied());
                for t in args.types.iter() {
                    lifetimes_in(t, out);
                }
                for b in args.bindings.iter() {
                    lifetimes_in(&b.ty, out);
                }
            }
            if let Some(qself) = &path.qself {
                lifetimes_in(&qself.ty, out);
            }
        }
        TypeRef::Tuple(list) => list.iter().for_each(|t| lifetimes_in(t, out)),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => lifetimes_in(pointee, out),
        TypeRef::FnPtr { params, ret, .. } => {
            params.iter().for_each(|t| lifetimes_in(t, out));
            lifetimes_in(ret, out);
        }
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter() {
                for args in b.path.args.iter().flatten() {
                    out.extend(args.lifetimes.iter().copied());
                    args.types.iter().for_each(|t| lifetimes_in(t, out));
                    args.bindings.iter().for_each(|b| lifetimes_in(&b.ty, out));
                }
            }
            out.extend(d.lifetime);
        }
        TypeRef::Never
        | TypeRef::Infer
        | TypeRef::Const(_)
        | TypeRef::ImplTrait(_)
        | TypeRef::ImplParam(_)
        | TypeRef::Opaque(_)
        | TypeRef::Error => {}
    }
}

/// Where `ty` elides a reference's lifetime (`&T` or `&'_ T`), into
/// `out`. Structs' elided lifetime arguments are not found: that needs
/// their definitions.
pub(crate) fn elided_in(ty: &TypeRef, out: &mut Vec<TextRange>) {
    match ty {
        TypeRef::Ref {
            lifetime,
            amp,
            pointee,
            ..
        } => {
            match lifetime {
                None => out.push(*amp),
                Some(l) if l.is_anonymous() => out.push(l.range),
                Some(_) => {}
            }
            elided_in(pointee, out);
        }
        TypeRef::Path(path) => {
            for args in path.args.iter().flatten() {
                for l in args.lifetimes.iter().filter(|l| l.is_anonymous()) {
                    out.push(l.range);
                }
                for t in args.types.iter() {
                    elided_in(t, out);
                }
                for b in args.bindings.iter() {
                    elided_in(&b.ty, out);
                }
            }
        }
        TypeRef::Tuple(list) => list.iter().for_each(|t| elided_in(t, out)),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => elided_in(pointee, out),
        // `dyn Tr + '_`; without a bound, its default is not elided.
        TypeRef::Dyn(d) => {
            if let Some(l) = d.lifetime.filter(|l| l.is_anonymous()) {
                out.push(l.range);
            }
        }
        // A function pointer's own.
        TypeRef::FnPtr { .. }
        | TypeRef::Never
        | TypeRef::Infer
        | TypeRef::ImplTrait(_)
        | TypeRef::ImplParam(_)
        | TypeRef::Opaque(_)
        | TypeRef::Const(_)
        | TypeRef::Error => {}
    }
}

/// The help rustc gives for a return type's elided lifetime that
/// elision cannot find (E0106): `params` are the parameters' names (or
/// `None` for a pattern) and how many elided lifetimes each has, and
/// `named` whether the inputs have lifetimes that are not elided.
pub(crate) fn elision_help(params: &[(Option<Symbol>, usize)], named: bool) -> String {
    let start = "this function's return type contains a borrowed value";
    let name = |i: usize, n: Option<Symbol>| match n {
        Some(n) => format!("`{n}`"),
        None => format!("argument {}", i + 1),
    };
    let with: Vec<(usize, Option<Symbol>, usize)> = params
        .iter()
        .enumerate()
        .filter(|(_, (_, count))| *count > 0)
        .map(|(i, &(n, count))| (i, n, count))
        .collect();
    match with.as_slice() {
        [] if named => format!(
            "{start} with an elided lifetime, but the lifetime cannot be derived from the arguments"
        ),
        [] => format!("{start}, but there is no value for it to be borrowed from"),
        [(i, n, count)] => {
            let who = match n {
                Some(n) => format!("`{n}`'s"),
                None => format!("argument {}'s", i + 1),
            };
            format!(
                "{start}, but the signature does not say which one of {who} {count} lifetimes it is borrowed from"
            )
        }
        _ => {
            let names: Vec<String> = with.iter().map(|&(i, n, _)| name(i, n)).collect();
            let list = match names.as_slice() {
                [a, b] => format!("{a} or {b}"),
                [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
                [] => String::new(),
            };
            format!("{start}, but the signature does not say whether it is borrowed from {list}")
        }
    }
}

/// The lifetime parameters of `generics`, named.
pub(crate) fn own_names(generics: &Generics) -> impl Iterator<Item = LifetimeName> + '_ {
    generics.lifetimes.iter().map(|l| LifetimeName {
        name: Some(l.name),
        range: l.range,
    })
}

/// What a well-formed `ty`, with slots `regions`, implies its lifetimes
/// outlive: `&'a T` that `T: 'a`, and a struct's inferred requirements.
pub fn implied_outlives<R: Copy + PartialEq>(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    regions: &[R],
    out: &mut Vec<Outlives<R>>,
) {
    let mut next = 0;
    implied_at(items, sigs, ty, regions, &mut next, out);
}

fn implied_at<R: Copy + PartialEq>(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    regions: &[R],
    next: &mut usize,
    out: &mut Vec<Outlives<R>>,
) {
    match ty.kind() {
        TyKind::Ref(_, inner) => {
            let Some(&r) = regions.get(*next) else { return };
            *next += 1;
            let start = *next;
            implied_at(items, sigs, inner, regions, next, out);
            // Everything in the pointee outlives the reference.
            for &inner_r in &regions[start..*next] {
                if inner_r != r {
                    out.push(Outlives::Region(inner_r, r));
                }
            }
            outlives_params(inner, r, out);
        }
        TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            implied_at(items, sigs, t, regions, next, out)
        }
        TyKind::Tuple(list) => {
            for &t in list {
                implied_at(items, sigs, t, regions, next, out);
            }
        }
        TyKind::Adt(adt, args) => {
            let lifetimes = items.structs[adt.strukt].generics.lifetimes.len();
            let start = *next;
            *next += lifetimes;
            let mut arg_ranges = Vec::new();
            for &t in args {
                let from = *next;
                implied_at(items, sigs, t, regions, next, out);
                arg_ranges.push((from, *next));
            }
            let Some(def) = sigs.regions.structs.get(adt.strukt) else {
                return;
            };
            let lifetime = |r: Region| match r {
                Region::Param(i) => regions.get(start + i as usize).copied(),
                _ => None,
            };
            for p in def.outlives.iter() {
                match *p {
                    Outlives::Region(a, b) => {
                        if let (Some(a), Some(b)) = (lifetime(a), lifetime(b))
                            && a != b
                        {
                            out.push(Outlives::Region(a, b));
                        }
                    }
                    Outlives::Type(t, b) => {
                        let Some(b) = lifetime(b) else { continue };
                        let TyKind::Param(p) = t.kind() else { continue };
                        let Some(&arg) = args.get(p.index as usize) else {
                            continue;
                        };
                        let (from, to) = arg_ranges[p.index as usize];
                        for &a in &regions[from.min(regions.len())..to.min(regions.len())] {
                            if a != b {
                                out.push(Outlives::Region(a, b));
                            }
                        }
                        outlives_params(arg, b, out);
                    }
                }
            }
        }
        TyKind::Closure(_, args) => implied_at(
            items,
            sigs,
            ClosureArgs::split(args).upvars,
            regions,
            next,
            out,
        ),
        TyKind::Opaque(opaque) => {
            *next += opaque.lifetimes as usize;
            for &t in opaque.args {
                implied_at(items, sigs, t, regions, next, out);
            }
        }
        _ => {}
    }
}

/// `T: r` for each parameter and associated type in `ty`.
fn outlives_params<R: Copy>(ty: Ty, r: R, out: &mut Vec<Outlives<R>>) {
    match ty.kind() {
        TyKind::Param(_) | TyKind::Alias(_) => out.push(Outlives::Type(ty, r)),
        TyKind::FnDef(_) | TyKind::FnPtr(_) => {}
        _ => {
            if ty.has_params() || ty.has_aliases() {
                for t in ty.children() {
                    outlives_params(t, r, out);
                }
            }
        }
    }
}
