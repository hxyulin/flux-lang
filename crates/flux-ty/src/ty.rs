//! Types, interned.
//!
//! A [`Ty`] is a 4-byte handle into one process-wide table of [`TyKind`]s,
//! so types compare and hash as integers, and a type that contains types
//! (`*const T`, `(A, B)`) stays `Copy`. The primitive types are interned
//! first, so [`Ty::BOOL`] and [`Ty::int`] are constants. Lists of types (a
//! tuple's elements, a struct's generic arguments) are interned too, and
//! live for the rest of the process, like the table.
//!
//! A generic item's types mention its parameters ([`TyKind::Param`]);
//! [`Ty::subst`] replaces them with the arguments of one use of it.
//!
//! Reading a type's kind takes no lock: kinds live in an append-only
//! table of write-once slots, as the symbol interner's strings do, since
//! type checking reads them constantly. Interning looks in a per-thread
//! cache first and then in one of many shards, each with its own lock, so
//! threads checking bodies in parallel rarely wait for each other.

use std::cell::Cell;
use std::fmt;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, OnceLock, RwLock};

use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;

use flux_hir::{ExprId, FunctionId, StructId, TraitId};
use flux_intern::{Symbol, sym};

/// A type: a handle to its [`TyKind`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ty(u32);

/// What a type is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TyKind {
    Int(IntTy),
    Bool,
    /// `char`: a Unicode scalar value, held as a `u32`.
    Char,
    /// `str`: UTF-8 text, unsized like a slice of bytes, so only behind
    /// a pointer.
    Str,
    /// `(A, B)`; `()` is the empty tuple, [`Ty::UNIT`].
    Tuple(&'static [Ty]),
    /// A struct, with its generic arguments.
    Adt(AdtId, GenericArgs),
    /// A generic parameter of the item whose types these are.
    Param(ParamTy),
    /// `<T as Tr>::Out`: a trait's associated type for a type, where it
    /// cannot be replaced by what it is (yet).
    Alias(AliasTy),
    /// The `impl Trait` of a function's return type: a type of its own,
    /// known only by its bounds, but for code generation, which reveals
    /// the type the function's body returns.
    Opaque(OpaqueTy),
    /// `!`
    Never,
    /// `*const T` and `*mut T`.
    RawPtr(Mutability, Ty),
    /// `&T` and `&mut T`.
    Ref(Mutability, Ty),
    /// `[T; N]`: its element type and its length, a constant of type
    /// `usize` ([`TyKind::Value`], a const parameter, or a variable).
    Array(Ty, Ty),
    /// `[T]`: unsized, so only behind a pointer.
    Slice(Ty),
    /// `core::ffi::CStr`: a NUL-terminated string, only behind a reference.
    CStr,
    /// The type of a function item, `fn(i32) -> i32 {double}`: one per
    /// function and generic arguments, with no value to hold, which
    /// coerces to a function pointer of its signature.
    FnDef(FnDefTy),
    /// A function pointer, `fn(i32) -> i32`.
    FnPtr(FnSigTy),
    /// The type of a closure expression: unique to it, with its owner's
    /// generic arguments followed by [`ClosureArgs`]' three.
    Closure(ClosureId, GenericArgs),
    /// An inference variable; never present after inference.
    Infer(InferVar),
    /// A constant as a generic argument or array length (`3` in `[T;
    /// 3]` and `Stack<i32, 3>`): not a type, but held as one, so that
    /// generic arguments, substitution and inference treat types and
    /// constants alike, as const parameters are generic parameters
    /// ([`TyKind::Param`]) and inferred ones variables.
    Value(ConstVal),
    /// The type of an erroneous expression, already reported. It unifies
    /// with everything, so one error does not cause others.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mutability {
    Not,
    Mut,
}

/// An integer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

/// The value of a constant of type `ty` (an integer type, `bool` or
/// `char`), as bits, an integer's truncated to its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstVal {
    pub ty: Ty,
    lo: u64,
    hi: u64,
}

impl ConstVal {
    pub fn new(ty: Ty, bits: u128) -> ConstVal {
        ConstVal {
            ty,
            lo: bits as u64,
            hi: (bits >> 64) as u64,
        }
    }

    pub fn bits(self) -> u128 {
        (u128::from(self.hi) << 64) | u128::from(self.lo)
    }
}

impl fmt::Display for ConstVal {
    /// As rustc writes const arguments: `3`, `-1`, `true`, `'a'`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bits = self.bits();
        match self.ty.kind() {
            TyKind::Bool => write!(f, "{}", bits != 0),
            TyKind::Char => match char::from_u32(bits as u32) {
                Some(c) => write!(f, "{c:?}"),
                None => write!(f, "{bits}"),
            },
            TyKind::Int(int) if int.is_signed() => {
                let shift = 128 - int.bits();
                write!(f, "{}", ((bits as i128) << shift) >> shift)
            }
            _ => write!(f, "{bits}"),
        }
    }
}

/// A struct, and its name for messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AdtId {
    pub strukt: StructId,
    pub name: Symbol,
}

/// `<T as Tr<A>>::Out`: associated type `name` of trait `trait_`, with
/// `args` its `Self` then the trait's own arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AliasTy {
    pub trait_: TraitId,
    pub trait_name: Symbol,
    pub name: Symbol,
    pub args: GenericArgs,
}

impl AliasTy {
    pub fn self_ty(self) -> Ty {
        self.args[0]
    }
}

/// The `impl Trait` with index `index` in `function`'s return type, with
/// generic arguments for all of the function's parameters (its owner's
/// first), which it captures, as edition 2024 does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpaqueTy {
    pub function: FunctionId,
    pub index: u32,
    /// As written, which is how rustc names it: `impl Iterator<Item = u32>`.
    pub name: Symbol,
    pub args: GenericArgs,
    /// How many lifetimes it captures: its region slots, before its
    /// arguments'.
    pub lifetimes: u32,
}

/// A function's signature as a type: its parameters' types then its
/// return type, and whether calling it is `unsafe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnSigTy {
    pub inputs_output: &'static [Ty],
    pub is_unsafe: bool,
}

impl FnSigTy {
    pub fn new(params: &[Ty], ret: Ty, is_unsafe: bool) -> FnSigTy {
        let mut all = params.to_vec();
        all.push(ret);
        FnSigTy {
            inputs_output: Ty::list(&all),
            is_unsafe,
        }
    }

    pub fn params(self) -> &'static [Ty] {
        &self.inputs_output[..self.inputs_output.len() - 1]
    }

    pub fn ret(self) -> Ty {
        self.inputs_output[self.inputs_output.len() - 1]
    }

    /// The signature with each type replaced by `f`'s.
    pub fn map(self, f: impl FnMut(Ty) -> Ty) -> FnSigTy {
        let list: Vec<Ty> = self.inputs_output.iter().copied().map(f).collect();
        FnSigTy {
            inputs_output: Ty::list(&list),
            ..self
        }
    }
}

/// A closure: the function whose body has it, and its expression there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClosureId {
    pub owner: FunctionId,
    pub expr: ExprId,
}

/// Which of the `Fn`-family traits a closure implements: all three for
/// `Fn`, `FnMut` and `FnOnce` for `FnMut`, `FnOnce` alone for `FnOnce`.
/// As in rustc, a closure's type holds it as a type, `i8`, `i16` or
/// `i32`, so that it can be inferred like its other parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClosureKind {
    Fn,
    FnMut,
    FnOnce,
}

impl ClosureKind {
    pub fn as_ty(self) -> Ty {
        Ty::int(match self {
            ClosureKind::Fn => IntTy::I8,
            ClosureKind::FnMut => IntTy::I16,
            ClosureKind::FnOnce => IntTy::I32,
        })
    }

    pub fn from_ty(ty: Ty) -> Option<ClosureKind> {
        match ty.kind() {
            TyKind::Int(IntTy::I8) => Some(ClosureKind::Fn),
            TyKind::Int(IntTy::I16) => Some(ClosureKind::FnMut),
            TyKind::Int(IntTy::I32) => Some(ClosureKind::FnOnce),
            _ => None,
        }
    }

    /// The trait's name.
    pub fn name(self) -> &'static str {
        match self {
            ClosureKind::Fn => "Fn",
            ClosureKind::FnMut => "FnMut",
            ClosureKind::FnOnce => "FnOnce",
        }
    }
}

/// A function item's type: the function, its name for messages, its
/// generic arguments, and its signature with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnDefTy {
    pub function: FunctionId,
    pub name: Symbol,
    pub args: GenericArgs,
    pub sig: FnSigTy,
}

/// The last three generic arguments of a closure's type: its kind
/// ([`ClosureKind::as_ty`]), its signature as the tuple `(params, ret)`
/// with `params` a tuple, and the tuple of the types of what it captures
/// (its fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureArgs {
    pub parent: GenericArgs,
    pub kind: Ty,
    pub sig: Ty,
    pub upvars: Ty,
}

impl ClosureArgs {
    pub fn split(args: GenericArgs) -> ClosureArgs {
        let n = args.len();
        assert!(n >= 3, "a closure's type has its three parts");
        ClosureArgs {
            parent: &args[..n - 3],
            kind: args[n - 3],
            sig: args[n - 2],
            upvars: args[n - 1],
        }
    }

    /// The tuple of the parameters' types.
    pub fn params(&self) -> Ty {
        match self.sig.kind() {
            TyKind::Tuple([params, _]) => *params,
            _ => Ty::ERROR,
        }
    }

    pub fn ret(&self) -> Ty {
        match self.sig.kind() {
            TyKind::Tuple([_, ret]) => *ret,
            _ => Ty::ERROR,
        }
    }

    /// The captured values' types, once known.
    pub fn upvar_types(&self) -> &'static [Ty] {
        match self.upvars.kind() {
            TyKind::Tuple(list) => list,
            _ => &[],
        }
    }

    pub fn closure_kind(&self) -> Option<ClosureKind> {
        ClosureKind::from_ty(self.kind)
    }
}

/// The generic arguments of a use of a generic item: one type per
/// parameter, in order, the `impl` block's first for an associated item.
pub type GenericArgs = &'static [Ty];

/// A generic parameter: the `index`th of its item's, counting the `impl`
/// block's first, as in [`GenericArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamTy {
    pub index: u32,
    pub name: Symbol,
}

/// A type variable of one body's inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InferVar(pub(crate) u32);

impl InferVar {
    /// `_` written in a type in a body, before inference gives it a
    /// variable of its own.
    pub(crate) const PLACEHOLDER: InferVar = InferVar(u32::MAX);
}

/// The interning side: what each kind's and list's index is, split into
/// shards by hash, each on its own cache line, so threads interning
/// different types seldom touch the same lock (as the symbol interner's
/// shards are). Indices come from one counter, so they stay dense.
struct Interner {
    kinds: [Shard<u32>; SHARDS],
    lists: [Shard<&'static [Ty]>; SHARDS],
    next: AtomicU32,
}

/// `(hash, value)`; the hash is kept to make resizing cheap. Readers share
/// the lock; inserting takes it exclusively.
type Shard<T> = CachePadded<RwLock<HashTable<(u64, T)>>>;

const SHARDS: usize = 64;

/// Keeps each shard's lock on its own cache line (128 bytes on Apple
/// silicon), so threads locking neighboring shards do not slow each other.
#[repr(align(128))]
struct CachePadded<T>(T);

/// Entries in each thread's caches of interned kinds and lists: 4096 × 16
/// and 4096 × 24 bytes, 160 KiB per thread.
const CACHE_SLOTS: usize = 4096;
/// Marks an empty cache slot; never a real type.
const EMPTY: Ty = Ty(u32::MAX);

thread_local! {
    /// A direct-mapped cache of recently interned kinds, `(hash, type)`:
    /// most kinds a body interns it has interned before, and looking them
    /// up here takes no lock at all.
    static CACHE: [Cell<(u64, Ty)>; CACHE_SLOTS] =
        const { [const { Cell::new((0, EMPTY)) }; CACHE_SLOTS] };
    /// The same for lists, `(hash, list)`; an empty slot holds the empty
    /// list, which is never interned.
    static LIST_CACHE: [Cell<(u64, &'static [Ty])>; CACHE_SLOTS] =
        const { [const { Cell::new((0, &[])) }; CACHE_SLOTS] };
}

/// The shard of a value whose hash is `hash`. hashbrown uses the low bits
/// of the hash for the bucket and the top seven for its tag, so take the
/// shard from the middle.
fn shard(hash: u64) -> usize {
    (hash >> 40) as usize % SHARDS
}

/// Each type's kind and flags ([`HAS_PARAM`], [`HAS_INFER`]), by index,
/// written once when it is interned. Bucket `b` holds `FIRST << b`
/// slots, so the table grows without moving what it holds.
struct Kinds {
    buckets: [OnceLock<Box<[Slot]>>; BUCKETS],
}

type Slot = OnceLock<(TyKind, u8)>;

const FIRST: u32 = 64;
const BUCKETS: usize = 26;

impl Kinds {
    /// The bucket and slot of index `i`.
    fn locate(i: u32) -> (usize, usize) {
        let n = i / FIRST + 1;
        let bucket = (31 - n.leading_zeros()) as usize;
        let start = FIRST * ((1 << bucket) - 1);
        (bucket, (i - start) as usize)
    }

    fn get(&self, i: u32) -> Option<(TyKind, u8)> {
        let (bucket, slot) = Kinds::locate(i);
        self.buckets[bucket].get()?[slot].get().copied()
    }

    fn set(&self, i: u32, value: (TyKind, u8)) {
        let (bucket, slot) = Kinds::locate(i);
        let slots = self.buckets[bucket]
            .get_or_init(|| (0..FIRST << bucket).map(|_| OnceLock::new()).collect());
        slots[slot].set(value).expect("each type is written once");
    }
}

static KINDS: Kinds = Kinds {
    buckets: [const { OnceLock::new() }; BUCKETS],
};

/// The type contains a generic parameter.
const HAS_PARAM: u8 = 1;
/// The type contains an inference variable.
const HAS_INFER: u8 = 2;
/// The type contains an associated type.
const HAS_ALIAS: u8 = 4;
/// The type contains an opaque type.
const HAS_OPAQUE: u8 = 8;
/// The type contains [`Ty::ERROR`].
const HAS_ERROR: u8 = 16;

/// Interned in this order, so the constants below are their indices.
const PREDEFINED: [TyKind; 6] = [
    TyKind::Error,
    TyKind::Tuple(&[]),
    TyKind::Bool,
    TyKind::Never,
    TyKind::Char,
    TyKind::Str,
];

static INTERNER: LazyLock<Interner> = LazyLock::new(|| {
    let interner = Interner {
        kinds: std::array::from_fn(|_| CachePadded(RwLock::default())),
        lists: std::array::from_fn(|_| CachePadded(RwLock::default())),
        next: AtomicU32::new(0),
    };
    let ints = IntTy::ALL.map(TyKind::Int);
    for kind in PREDEFINED.into_iter().chain(ints) {
        interner.intern(kind, FxBuildHasher.hash_one(kind));
    }
    interner
});

impl Interner {
    /// The index of `kind`, whose hash is `hash`, interning it if it is new.
    fn intern(&self, kind: TyKind, hash: u64) -> u32 {
        let shard = &self.kinds[shard(hash)].0;
        let eq = |&(h, id): &(u64, u32)| h == hash && KINDS.get(id).is_some_and(|e| e.0 == kind);
        // Most calls find the kind, so look under a shared lock first.
        if let Some(&(_, id)) = shard.read().unwrap().find(hash, eq) {
            return id;
        }
        let mut table = shard.write().unwrap();
        // Another thread may have interned it between the two locks.
        if let Some(&(_, id)) = table.find(hash, eq) {
            return id;
        }
        let of = |ty: Ty| KINDS.get(ty.0).map_or(0, |(_, flags)| flags);
        let flags = match kind {
            TyKind::Param(_) => HAS_PARAM,
            TyKind::Infer(_) => HAS_INFER,
            TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
                list.iter().fold(0, |f, &t| f | of(t))
            }
            TyKind::Alias(alias) => alias.args.iter().fold(HAS_ALIAS, |f, &t| f | of(t)),
            TyKind::Opaque(opaque) => opaque.args.iter().fold(HAS_OPAQUE, |f, &t| f | of(t)),
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Slice(t) => of(t),
            TyKind::Array(t, len) => of(t) | of(len),
            TyKind::FnPtr(sig) => sig.inputs_output.iter().fold(0, |f, &t| f | of(t)),
            TyKind::FnDef(def) => def
                .args
                .iter()
                .chain(def.sig.inputs_output)
                .fold(0, |f, &t| f | of(t)),
            TyKind::Int(_)
            | TyKind::Bool
            | TyKind::Char
            | TyKind::Str
            | TyKind::Never
            | TyKind::CStr
            | TyKind::Value(_) => 0,
            TyKind::Error => HAS_ERROR,
        };
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        assert!(id < EMPTY.0, "more than u32::MAX types interned");
        // Written before the index is published, so whoever has the
        // index finds it.
        KINDS.set(id, (kind, flags));
        table.insert_unique(hash, (hash, id), |&(h, _)| h);
        id
    }

    /// `types`, whose hash is `hash`, interned.
    fn list(&self, types: &[Ty], hash: u64) -> &'static [Ty] {
        let shard = &self.lists[shard(hash)].0;
        let eq = |&(h, list): &(u64, &'static [Ty])| h == hash && list == types;
        if let Some(&(_, list)) = shard.read().unwrap().find(hash, eq) {
            return list;
        }
        let mut table = shard.write().unwrap();
        if let Some(&(_, list)) = table.find(hash, eq) {
            return list;
        }
        let list: &'static [Ty] = Box::leak(types.into());
        table.insert_unique(hash, (hash, list), |&(h, _)| h);
        list
    }
}

impl Ty {
    pub const ERROR: Ty = Ty(0);
    pub const UNIT: Ty = Ty(1);
    pub const BOOL: Ty = Ty(2);
    pub const NEVER: Ty = Ty(3);
    pub const CHAR: Ty = Ty(4);
    pub const STR: Ty = Ty(5);

    pub const fn int(int: IntTy) -> Ty {
        Ty(PREDEFINED.len() as u32 + int as u32)
    }

    /// The type with this kind, interning it if it is new.
    pub fn new(kind: TyKind) -> Ty {
        let hash = FxBuildHasher.hash_one(kind);
        CACHE.with(|cache| {
            let slot = &cache[hash as usize % CACHE_SLOTS];
            let (cached_hash, ty) = slot.get();
            if cached_hash == hash && ty != EMPTY && ty.kind() == kind {
                return ty;
            }
            let ty = Ty(INTERNER.intern(kind, hash));
            slot.set((hash, ty));
            ty
        })
    }

    pub fn kind(self) -> TyKind {
        self.entry().0
    }

    fn entry(self) -> (TyKind, u8) {
        match KINDS.get(self.0) {
            Some(entry) => entry,
            // The predefined types exist before anything is interned.
            None => {
                LazyLock::force(&INTERNER);
                KINDS.get(self.0).expect("types are interned before use")
            }
        }
    }

    pub fn infer(var: InferVar) -> Ty {
        Ty::new(TyKind::Infer(var))
    }

    pub fn raw_ptr(mutability: Mutability, pointee: Ty) -> Ty {
        Ty::new(TyKind::RawPtr(mutability, pointee))
    }

    /// `types`, interned.
    pub fn list(types: &[Ty]) -> &'static [Ty] {
        if types.is_empty() {
            return &[];
        }
        let hash = FxBuildHasher.hash_one(types);
        LIST_CACHE.with(|cache| {
            let slot = &cache[hash as usize % CACHE_SLOTS];
            let (cached_hash, list) = slot.get();
            if cached_hash == hash && list == types {
                return list;
            }
            let list = INTERNER.list(types, hash);
            slot.set((hash, list));
            list
        })
    }

    pub fn tuple(elements: &[Ty]) -> Ty {
        Ty::new(TyKind::Tuple(Ty::list(elements)))
    }

    pub fn adt(strukt: StructId, name: Symbol, args: &[Ty]) -> Ty {
        Ty::new(TyKind::Adt(AdtId { strukt, name }, Ty::list(args)))
    }

    pub fn closure(id: ClosureId, args: &[Ty]) -> Ty {
        Ty::new(TyKind::Closure(id, Ty::list(args)))
    }

    pub fn param(index: u32, name: Symbol) -> Ty {
        Ty::new(TyKind::Param(ParamTy { index, name }))
    }

    fn flags(self) -> u8 {
        self.entry().1
    }

    /// Whether the type mentions a generic parameter.
    pub fn has_params(self) -> bool {
        self.flags() & HAS_PARAM != 0
    }

    /// Whether the type contains an inference variable.
    pub fn has_infer(self) -> bool {
        self.flags() & HAS_INFER != 0
    }

    /// Whether the type contains an associated type.
    pub fn has_aliases(self) -> bool {
        self.flags() & HAS_ALIAS != 0
    }

    pub fn alias(alias: AliasTy) -> Ty {
        Ty::new(TyKind::Alias(alias))
    }

    /// Whether the type contains an opaque type.
    pub fn has_opaques(self) -> bool {
        self.flags() & HAS_OPAQUE != 0
    }

    /// Whether the type is or contains [`Ty::ERROR`].
    pub fn has_error(self) -> bool {
        self.flags() & HAS_ERROR != 0
    }

    pub fn opaque(opaque: OpaqueTy) -> Ty {
        Ty::new(TyKind::Opaque(opaque))
    }

    /// The type with each opaque type replaced by `f`'s, inside out.
    pub fn map_opaques(self, f: &mut dyn FnMut(OpaqueTy) -> Ty) -> Ty {
        if !self.has_opaques() {
            return self;
        }
        let mut go = |t: Ty| t.map_opaques(f);
        match self.kind() {
            TyKind::Opaque(opaque) => {
                let args: Vec<Ty> = opaque.args.iter().map(|&a| go(a)).collect();
                f(OpaqueTy {
                    args: Ty::list(&args),
                    ..opaque
                })
            }
            TyKind::Tuple(list) => Ty::tuple(&list.iter().map(|&t| go(t)).collect::<Vec<_>>()),
            TyKind::Adt(adt, list) => {
                let list: Vec<Ty> = list.iter().map(|&t| go(t)).collect();
                Ty::new(TyKind::Adt(adt, Ty::list(&list)))
            }
            TyKind::Closure(id, list) => {
                let list: Vec<Ty> = list.iter().map(|&t| go(t)).collect();
                Ty::closure(id, &list)
            }
            TyKind::Alias(alias) => {
                let list: Vec<Ty> = alias.args.iter().map(|&t| go(t)).collect();
                Ty::alias(AliasTy {
                    args: Ty::list(&list),
                    ..alias
                })
            }
            TyKind::RawPtr(m, t) => Ty::raw_ptr(m, go(t)),
            TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, go(t))),
            TyKind::Array(t, len) => Ty::array_of(go(t), go(len)),
            TyKind::Slice(t) => Ty::slice(go(t)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => self.map_fn(go),
            _ => self,
        }
    }

    /// The type with each generic parameter replaced by its argument in
    /// `args`.
    pub fn subst(self, args: &[Ty]) -> Ty {
        if !self.has_params() {
            return self;
        }
        match self.kind() {
            TyKind::Param(param) => args.get(param.index as usize).copied().unwrap_or(Ty::ERROR),
            TyKind::Tuple(elements) => Ty::tuple(&subst_list(elements, args)),
            TyKind::Adt(adt, list) => Ty::new(TyKind::Adt(adt, Ty::list(&subst_list(list, args)))),
            TyKind::Closure(id, list) => {
                Ty::new(TyKind::Closure(id, Ty::list(&subst_list(list, args))))
            }
            TyKind::Alias(alias) => Ty::alias(AliasTy {
                args: Ty::list(&subst_list(alias.args, args)),
                ..alias
            }),
            TyKind::Opaque(opaque) => Ty::opaque(OpaqueTy {
                args: Ty::list(&subst_list(opaque.args, args)),
                ..opaque
            }),
            TyKind::RawPtr(m, pointee) => Ty::raw_ptr(m, pointee.subst(args)),
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, pointee.subst(args))),
            TyKind::Array(elem, len) => Ty::array_of(elem.subst(args), len.subst(args)),
            TyKind::Slice(elem) => Ty::slice(elem.subst(args)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => self.map_fn(|t| t.subst(args)),
            _ => self,
        }
    }

    /// `[elem; len]`.
    pub fn array(elem: Ty, len: u64) -> Ty {
        Ty::array_of(elem, Ty::usize_value(len))
    }

    /// `[elem; len]`, with `len` a constant of type `usize`.
    pub fn array_of(elem: Ty, len: Ty) -> Ty {
        Ty::new(TyKind::Array(elem, len))
    }

    /// The constant `bits` of type `ty`.
    pub fn value(ty: Ty, bits: u128) -> Ty {
        Ty::new(TyKind::Value(ConstVal::new(ty, bits)))
    }

    /// The `usize` constant `n`.
    pub fn usize_value(n: u64) -> Ty {
        Ty::value(Ty::int(IntTy::Usize), u128::from(n))
    }

    /// The value of this constant, if it is one.
    pub fn as_value(self) -> Option<ConstVal> {
        match self.kind() {
            TyKind::Value(value) => Some(value),
            _ => None,
        }
    }

    /// The value of this constant of type `usize`, if it is one.
    pub fn as_usize(self) -> Option<u64> {
        self.as_value().map(|v| v.bits() as u64)
    }

    /// The length of an array type, if it is known: always once
    /// monomorphized.
    pub fn array_len(self) -> Option<u64> {
        match self.kind() {
            TyKind::Array(_, len) => len.as_usize(),
            _ => None,
        }
    }

    pub fn slice(elem: Ty) -> Ty {
        Ty::new(TyKind::Slice(elem))
    }

    /// The element type of an array or slice.
    pub fn sequence_elem(self) -> Option<Ty> {
        match self.kind() {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => Some(elem),
            _ => None,
        }
    }

    /// For a function item or pointer type, the type with each type in
    /// it (generic arguments and signature) replaced by `f`'s; any other
    /// type unchanged.
    pub fn map_fn(self, mut f: impl FnMut(Ty) -> Ty) -> Ty {
        match self.kind() {
            TyKind::FnPtr(sig) => Ty::new(TyKind::FnPtr(sig.map(f))),
            TyKind::FnDef(def) => {
                let args: Vec<Ty> = def.args.iter().map(|&a| f(a)).collect();
                Ty::new(TyKind::FnDef(FnDefTy {
                    args: Ty::list(&args),
                    sig: def.sig.map(f),
                    ..def
                }))
            }
            _ => self,
        }
    }

    /// The types directly inside this one.
    pub fn children(self) -> Vec<Ty> {
        match self.kind() {
            TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => list.to_vec(),
            TyKind::Alias(alias) => alias.args.to_vec(),
            TyKind::Opaque(opaque) => opaque.args.to_vec(),
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Slice(t) => vec![t],
            TyKind::Array(t, len) => vec![t, len],
            TyKind::FnPtr(sig) => sig.inputs_output.to_vec(),
            TyKind::FnDef(def) => def
                .args
                .iter()
                .chain(def.sig.inputs_output)
                .copied()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The signature of a function item or pointer type.
    pub fn fn_sig(self) -> Option<FnSigTy> {
        match self.kind() {
            TyKind::FnPtr(sig) => Some(sig),
            TyKind::FnDef(def) => Some(def.sig),
            _ => None,
        }
    }

    /// Whether the type is a slice, which has no size of its own.
    pub fn is_unsized(self) -> bool {
        matches!(self.kind(), TyKind::Slice(_) | TyKind::Str)
    }

    /// The integer type, if this is one.
    pub fn as_int(self) -> Option<IntTy> {
        match self.kind() {
            TyKind::Int(int) => Some(int),
            _ => None,
        }
    }

    /// The integer type a scalar is held as: an integer's own, `u32` for
    /// a `char`.
    pub fn scalar_int(self) -> Option<IntTy> {
        match self.kind() {
            TyKind::Int(int) => Some(int),
            TyKind::Char => Some(IntTy::U32),
            _ => None,
        }
    }

    pub fn is_signed_int(self) -> bool {
        self.as_int().is_some_and(IntTy::is_signed)
    }

    /// `*const T`, `*mut T`, `&T` or `&mut T`.
    pub fn is_pointer(self) -> bool {
        matches!(self.kind(), TyKind::RawPtr(..) | TyKind::Ref(..))
    }
}

/// Each of `list` substituted with `args`.
pub fn subst_list(list: &[Ty], args: &[Ty]) -> Vec<Ty> {
    list.iter().map(|t| t.subst(args)).collect()
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ty({self})")
    }
}

impl IntTy {
    pub const ALL: [IntTy; 12] = [
        IntTy::I8,
        IntTy::I16,
        IntTy::I32,
        IntTy::I64,
        IntTy::I128,
        IntTy::Isize,
        IntTy::U8,
        IntTy::U16,
        IntTy::U32,
        IntTy::U64,
        IntTy::U128,
        IntTy::Usize,
    ];

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize
        )
    }

    /// Width in bits. Both targets flux supports are 64-bit, so `isize`
    /// and `usize` are 64 bits wide.
    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => 64,
            IntTy::I128 | IntTy::U128 => 128,
        }
    }

    /// The largest value, as an unsigned magnitude.
    pub fn max(self) -> u128 {
        let bits = self.bits() - u32::from(self.is_signed());
        if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        }
    }

    pub fn name(self) -> Symbol {
        match self {
            IntTy::I8 => sym::i8,
            IntTy::I16 => sym::i16,
            IntTy::I32 => sym::i32,
            IntTy::I64 => sym::i64,
            IntTy::I128 => sym::i128,
            IntTy::Isize => sym::isize,
            IntTy::U8 => sym::u8,
            IntTy::U16 => sym::u16,
            IntTy::U32 => sym::u32,
            IntTy::U64 => sym::u64,
            IntTy::U128 => sym::u128,
            IntTy::Usize => sym::usize,
        }
    }

    pub fn from_name(name: Symbol) -> Option<IntTy> {
        IntTy::ALL.into_iter().find(|ty| ty.name() == name)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            TyKind::Int(int) => write!(f, "{}", int.name()),
            TyKind::Bool => f.write_str("bool"),
            TyKind::Tuple([one]) => write!(f, "({one},)"),
            TyKind::Tuple(elements) => {
                f.write_str("(")?;
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{element}")?;
                }
                f.write_str(")")
            }
            TyKind::Adt(adt, []) => write!(f, "{}", adt.name),
            TyKind::Adt(adt, args) => {
                write!(f, "{}<", adt.name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(">")
            }
            TyKind::Param(param) => write!(f, "{}", param.name),
            TyKind::Opaque(opaque) => write!(f, "{}", opaque.name),
            TyKind::Alias(alias) => {
                write!(f, "<{} as {}", alias.args[0], alias.trait_name)?;
                if alias.args.len() > 1 {
                    let args: Vec<String> = alias.args[1..].iter().map(Ty::to_string).collect();
                    write!(f, "<{}>", args.join(", "))?;
                }
                write!(f, ">::{}", alias.name)
            }
            TyKind::Never => f.write_str("!"),
            TyKind::RawPtr(Mutability::Not, pointee) => write!(f, "*const {pointee}"),
            TyKind::RawPtr(Mutability::Mut, pointee) => write!(f, "*mut {pointee}"),
            TyKind::Ref(Mutability::Not, pointee) => write!(f, "&{pointee}"),
            TyKind::Ref(Mutability::Mut, pointee) => write!(f, "&mut {pointee}"),
            TyKind::Array(elem, len) => write!(f, "[{elem}; {len}]"),
            TyKind::Slice(elem) => write!(f, "[{elem}]"),
            TyKind::CStr => f.write_str("CStr"),
            TyKind::Char => f.write_str("char"),
            TyKind::Str => f.write_str("str"),
            TyKind::FnPtr(sig) => write_sig(f, sig),
            TyKind::FnDef(def) => {
                write_sig(f, def.sig)?;
                write!(f, " {{{}", def.name)?;
                if !def.args.is_empty() {
                    let args: Vec<String> = def.args.iter().map(Ty::to_string).collect();
                    write!(f, "::<{}>", args.join(", "))?;
                }
                f.write_str("}")
            }
            TyKind::Closure(..) => f.write_str("{closure}"),
            TyKind::Infer(_) => f.write_str("_"),
            TyKind::Value(value) => write!(f, "{value}"),
            TyKind::Error => f.write_str("{error}"),
        }
    }
}

/// `fn(A, B) -> R`, with `unsafe` before it if it is, and without the
/// return type if it is `()`.
fn write_sig(f: &mut fmt::Formatter<'_>, sig: FnSigTy) -> fmt::Result {
    if sig.is_unsafe {
        f.write_str("unsafe ")?;
    }
    let params: Vec<String> = sig.params().iter().map(Ty::to_string).collect();
    write!(f, "fn({})", params.join(", "))?;
    if sig.ret() != Ty::UNIT {
        write!(f, " -> {}", sig.ret())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning() {
        assert_eq!(Ty::new(TyKind::Bool), Ty::BOOL);
        assert_eq!(Ty::new(TyKind::Int(IntTy::U16)), Ty::int(IntTy::U16));
        assert_eq!(Ty::int(IntTy::Usize).kind(), TyKind::Int(IntTy::Usize));
        let p = Ty::raw_ptr(Mutability::Not, Ty::int(IntTy::I8));
        assert_eq!(p, Ty::raw_ptr(Mutability::Not, Ty::int(IntTy::I8)));
        assert_ne!(p, Ty::raw_ptr(Mutability::Mut, Ty::int(IntTy::I8)));
        assert_eq!(p.to_string(), "*const i8");
        assert_eq!(Ty::tuple(&[]), Ty::UNIT);
        let pair = Ty::tuple(&[Ty::BOOL, p]);
        assert_eq!(pair, Ty::tuple(&[Ty::BOOL, p]));
        assert_eq!(pair.to_string(), "(bool, *const i8)");
        assert_eq!(Ty::tuple(&[Ty::BOOL]).to_string(), "(bool,)");
    }
}
