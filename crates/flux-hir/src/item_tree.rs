//! The items of a compilation and their signatures: every crate's
//! modules, read from their files, and the functions, structs, `impl`
//! blocks and imports in them.

use std::path::{Path as FsPath, PathBuf};

use flux_arena::{Idx, LayeredArena};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::ast::{self, AstChildren, AstNode};
use flux_syntax::{NodeId, SyntaxElement, SyntaxKind, SyntaxNode, T, TextRange};

use crate::Diagnostic;
use crate::def_map::DefMap;
use crate::source::{FileId, SourceDb};

pub type FunctionId = Idx<Function>;
pub type StructId = Idx<Struct>;
pub type ImplId = Idx<Impl>;
pub type ModuleId = Idx<Module>;
pub type ConstId = Idx<Const>;
pub type TraitId = Idx<Trait>;
pub type TypeAliasId = Idx<TypeAlias>;

/// A crate, an index into [`ItemTree::crates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CrateId(pub u32);

/// Every item of every crate, without bodies. `core`'s items are frozen
/// in [`Core`](crate::Core)'s tree and shared by every program's.
#[derive(Debug, Clone, Default)]
pub struct ItemTree {
    pub crates: Vec<CrateData>,
    pub modules: LayeredArena<Module>,
    /// Free functions, functions in `extern` blocks, and the associated
    /// functions of `impl` blocks.
    pub functions: LayeredArena<Function>,
    pub structs: LayeredArena<Struct>,
    pub traits: LayeredArena<Trait>,
    pub impls: LayeredArena<Impl>,
    /// Free and associated constants.
    pub consts: LayeredArena<Const>,
    pub type_aliases: LayeredArena<TypeAlias>,
    pub uses: Vec<Use>,
    /// What each module's names mean, imports included.
    pub def_map: DefMap,
    /// The associated items of each type's inherent `impl` blocks, by
    /// name, in source order.
    pub assoc: rustc_hash::FxHashMap<(ImplTarget, Symbol), Vec<AssocItem>>,
    /// The `impl` blocks of each trait, in source order.
    pub trait_impls: rustc_hash::FxHashMap<TraitId, Vec<ImplId>>,
    /// The traits with a function or constant of each name.
    pub trait_fns: rustc_hash::FxHashMap<Symbol, Vec<TraitId>>,
    /// The module of each block with items, by its file and node.
    pub block_modules: rustc_hash::FxHashMap<(FileId, NodeId), ModuleId>,
    /// The traits the compiler knows, by their `#[lang = "..."]` names.
    pub lang_traits: rustc_hash::FxHashMap<Symbol, TraitId>,
    /// `core`'s structs and enums the compiler knows, by their
    /// `#[lang = "..."]` names: `Option` and `Result`, which `?` works
    /// on, and the ranges.
    pub lang_enums: rustc_hash::FxHashMap<Symbol, StructId>,
    /// `core`'s `#[lang = "panic"]` function, which aborts, and which
    /// constant evaluation reports as a panic with its message.
    pub panic_fn: Option<FunctionId>,
    /// `core`'s other functions the compiler knows, by their `#[lang =
    /// "..."]` names: those that the built-in `impl` blocks of arrays
    /// call (`array_into_iter`).
    pub lang_fns: rustc_hash::FxHashMap<Symbol, FunctionId>,
    /// Problems found collecting the items and resolving their names;
    /// [`lower`](crate::lower) moves them into the [`Hir`](crate::Hir)'s.
    pub diagnostics: Vec<Diagnostic>,
    /// How many of each item came from [`Core`](crate::Core), built and
    /// resolved before the program's: the items with smaller indices
    /// are `core`'s. Zero in a tree without `core`, or `core`'s own.
    pub core: ItemCounts,
}

/// How many items of each kind an [`ItemTree`] has: where the items a
/// crate added start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemCounts {
    pub modules: u32,
    pub functions: u32,
    pub structs: u32,
    pub traits: u32,
    pub impls: u32,
    pub consts: u32,
    pub type_aliases: u32,
    pub uses: u32,
}

#[derive(Debug, Clone)]
pub struct CrateData {
    pub name: Symbol,
    pub root: ModuleId,
    /// flux's own `core`, which may define methods on primitive types.
    pub is_core: bool,
}

/// A module: a crate's root, `mod m { ... }` or `mod m;`.
#[derive(Debug, Clone)]
pub struct Module {
    /// The crate's name for a root.
    pub name: Symbol,
    pub parent: Option<ModuleId>,
    pub krate: CrateId,
    pub file: FileId,
    /// Where the files of `mod child;` declarations in it are.
    pub dir: PathBuf,
    pub vis: Visibility,
    /// The `mod m` declaration, in the parent's file.
    pub def_range: TextRange,
    /// A block with items, `{ fn f() {} .. }`: an anonymous module, whose
    /// items only code in it sees, and through which names are looked
    /// up in its parent. `self` and `super` skip it.
    pub block: Option<Block>,
}

/// What a block with items is in.
#[derive(Debug, Clone, Copy)]
pub struct Block {
    /// The function whose body it is in, if any: a constant's value may
    /// have blocks with items too.
    pub function: Option<FunctionId>,
    /// The `BLOCK_EXPR` node.
    pub node: NodeId,
}

/// Who may name an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// `pub`
    Public,
    /// `pub(crate)`
    Crate(CrateId),
    /// Private, `pub(self)` or `pub(super)`: this module and the modules
    /// inside it.
    Module(ModuleId),
}

/// Where an item is and who may name it.
#[derive(Debug, Clone, Copy)]
pub struct ItemLoc {
    pub module: ModuleId,
    pub file: FileId,
    pub vis: Visibility,
}

/// An `impl` block: inherent, `impl S { ... }` or `impl<T> S<T> { ... }`,
/// or of a trait, `impl Tr for S { ... }`.
#[derive(Debug, Clone)]
pub struct Impl {
    pub generics: Generics,
    /// `Tr` in `impl Tr for S`, as written.
    pub trait_ref: Option<Bound>,
    /// The trait it implements, once resolved; `None` for an inherent
    /// block or a trait that did not resolve (reported).
    pub trait_: Option<TraitId>,
    pub is_unsafe: bool,
    pub self_ty: TypeRef,
    /// The self type's range.
    pub self_range: TextRange,
    /// `impl T`, where rustc reports errors about the whole block.
    pub def_range: TextRange,
    pub module: ModuleId,
    pub file: FileId,
    /// The type an inherent block is for; `None` for a trait's block or
    /// if it did not resolve (reported).
    pub target: Option<ImplTarget>,
    pub functions: Vec<FunctionId>,
    pub consts: Vec<ConstId>,
    /// A trait's block's associated types.
    pub types: Vec<ImplType>,
}

/// A constant: `const N: i32 = 5;`, free or in an `impl` block.
#[derive(Debug, Clone)]
pub struct Const {
    /// `None` for `const _: T = ...;`, which is evaluated but has no name.
    pub name: Option<Symbol>,
    pub ty: TypeRef,
    /// The type's range.
    pub ty_range: TextRange,
    pub loc: ItemLoc,
    /// The `impl` block or trait it is in, if any.
    pub owner: Option<Owner>,
    /// The `CONST` node in `loc.file`, for lowering its value.
    pub node: NodeId,
    pub has_body: bool,
    /// `const N: i32`, where rustc reports errors about the constant.
    pub def_range: TextRange,
    /// The whole item.
    pub item_range: TextRange,
    /// The enum and variant whose discriminant it is, written as an
    /// expression other than a literal (`A = 1 << 2`): an anonymous
    /// constant of the enum's `repr` type, in no scope, whose `node` is
    /// the variant.
    pub discriminant: Option<(StructId, u32)>,
    /// A static, `static X: T = v;` (`Some(false)`) or `static mut`
    /// (`Some(true)`): one place in memory, whose initial value is
    /// evaluated like a constant's, rather than a value copied at each
    /// use. Its `node` is a `STATIC`.
    pub is_static: Option<bool>,
}

impl Const {
    /// Whether it is a `static mut`.
    pub fn is_static_mut(&self) -> bool {
        self.is_static == Some(true)
    }
}

/// A type alias: `type Pair<T> = (T, T);`, another name for a type,
/// generic over its parameters.
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: Symbol,
    /// Its parameters; bounds on them are not enforced, as in rustc.
    pub generics: Generics,
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub loc: ItemLoc,
    pub name_range: TextRange,
    /// `type Pair`, where rustc reports errors about the whole alias.
    pub def_range: TextRange,
}

/// What an associated item is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    Impl(ImplId),
    Trait(TraitId),
}

impl Owner {
    pub fn as_impl(self) -> Option<ImplId> {
        match self {
            Owner::Impl(id) => Some(id),
            Owner::Trait(_) => None,
        }
    }

    pub fn as_trait(self) -> Option<TraitId> {
        match self {
            Owner::Trait(id) => Some(id),
            Owner::Impl(_) => None,
        }
    }
}

/// A trait: `trait Tr<T>: Super where ... { ... }`.
#[derive(Debug, Clone)]
pub struct Trait {
    pub name: Symbol,
    /// Its own parameters; `Self` is an implicit one before them.
    pub generics: Generics,
    /// `Super` in `trait Tr: Super`: bounds on `Self` that every type
    /// implementing it must also meet.
    pub supertraits: Box<[Bound]>,
    pub loc: ItemLoc,
    pub is_unsafe: bool,
    /// `#[lang = "name"]`, which only `core` may write: a trait the
    /// compiler knows.
    pub lang: Option<Symbol>,
    /// `message` in `#[diagnostic::on_unimplemented(message = "...")]`:
    /// how a bound on it that does not hold is reported, with `{Self}`
    /// and its parameters' names standing for their types.
    pub on_unimplemented: Option<Box<str>>,
    pub functions: Vec<FunctionId>,
    pub consts: Vec<ConstId>,
    pub types: Vec<TraitType>,
    pub name_range: TextRange,
    /// `trait Tr`, where rustc reports errors about the whole trait.
    pub def_range: TextRange,
}

impl Trait {
    /// Its function called `name`, if any.
    pub fn function(&self, items: &ItemTree, name: Symbol) -> Option<FunctionId> {
        self.functions
            .iter()
            .copied()
            .find(|&f| items.functions[f].name == name)
    }

    /// Its constant called `name`, if any.
    pub fn konst(&self, items: &ItemTree, name: Symbol) -> Option<ConstId> {
        self.consts
            .iter()
            .copied()
            .find(|&c| items.consts[c].name == Some(name))
    }

    /// Its function or constant called `name`, if any.
    pub fn value(&self, items: &ItemTree, name: Symbol) -> Option<AssocItem> {
        self.function(items, name)
            .map(AssocItem::Function)
            .or_else(|| self.konst(items, name).map(AssocItem::Const))
    }
}

/// An associated type of a trait: `type Out: Bound;`.
#[derive(Debug, Clone)]
pub struct TraitType {
    pub name: Symbol,
    /// The bounds every type given for it must meet.
    pub bounds: Box<[Bound]>,
    pub name_range: TextRange,
    /// The whole item.
    pub item_range: TextRange,
}

/// An associated type's definition in a trait's `impl` block:
/// `type Out = i32;`.
#[derive(Debug, Clone)]
pub struct ImplType {
    pub name: Symbol,
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub name_range: TextRange,
    pub item_range: TextRange,
}

/// A trait bound as written: `Tr` or `Tr<A>` in `T: Tr`, or the trait of
/// `impl Tr for S`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub path: Path,
    pub range: TextRange,
}

/// One bound on a type: `T: Tr` in a parameter list or `where` clause
/// (`T: A + B` is two).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WherePredicate {
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub bound: Bound,
}

/// An associated item of an `impl` block's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssocItem {
    Function(FunctionId),
    Const(ConstId),
}

/// The type of an `impl` block: a struct, or, in `core` only, a
/// primitive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImplTarget {
    Struct(StructId),
    Primitive(Symbol),
}

/// A struct, `struct S { a: i32 }`, `struct P(i32);` or `struct U;`, or
/// an enum, `enum E { A, B(i32), C { x: i32 } }`: a type made of fields,
/// or of one of several variants, each made of fields. They share an
/// arena, since their generics, bounds and `impl` blocks work alike.
#[derive(Debug, Clone)]
pub struct Struct {
    pub name: Symbol,
    pub generics: Generics,
    /// A struct's kind; `Record` for an enum.
    pub kind: StructKind,
    /// A struct's fields, in declaration order; a tuple struct's are
    /// named `0`, `1`, ... None for an enum.
    pub fields: Box<[Field]>,
    /// An enum's variants, in declaration order; `None` for a struct.
    pub variants: Option<Box<[Variant]>>,
    /// The integer type an enum's `#[repr(u8)]` (say) gives its
    /// discriminant; `isize` without one.
    pub repr: Option<Symbol>,
    pub loc: ItemLoc,
    pub name_range: TextRange,
    /// `struct S` or `enum E`, where rustc reports errors about the
    /// whole item.
    pub def_range: TextRange,
}

/// A variant of an enum: `A`, `B(i32)` or `C { x: i32 }`.
#[derive(Debug, Clone)]
pub struct Variant {
    pub name: Symbol,
    pub kind: StructKind,
    pub fields: Box<[Field]>,
    /// The value `as` casts give a fieldless enum's: written (`A = 5`),
    /// or one more than the previous variant's.
    pub discriminant: Discriminant,
    pub name_range: TextRange,
    pub def_range: TextRange,
}

/// A variant's discriminant, as far as the item tree knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discriminant {
    /// Written as an integer literal, or counted from one (or from the
    /// first variant's 0).
    Value(i128),
    /// The value of `konst`, the anonymous constant of an expression
    /// written for this variant or an earlier one, plus `offset`, the
    /// number of variants since, wrapping in the enum's `repr` type:
    /// known once constants are evaluated.
    Const { konst: ConstId, offset: u32 },
}

/// What a constructor, struct expression or pattern names: a struct, or
/// one variant of an enum.
#[derive(Debug, Clone, Copy)]
pub struct VariantData<'a> {
    pub name: Symbol,
    pub kind: StructKind,
    pub fields: &'a [Field],
    /// The enum's name for a variant, for messages (`E::A`).
    pub enum_name: Option<Symbol>,
}

impl VariantData<'_> {
    pub fn field(&self, name: Symbol) -> Option<(usize, &Field)> {
        self.fields.iter().enumerate().find(|(_, f)| f.name == name)
    }

    /// `S` or `E::A`.
    pub fn path(&self) -> String {
        match self.enum_name {
            Some(e) => format!("{e}::{}", self.name),
            None => self.name.to_string(),
        }
    }

    /// "struct" or "variant", for messages.
    pub fn what(&self) -> &'static str {
        if self.enum_name.is_some() {
            "variant"
        } else {
            "struct"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructKind {
    /// `struct S { a: i32 }`
    Record,
    /// `struct P(i32);`, which also defines the function `P`.
    Tuple,
    /// `struct U;`, which also defines the constant `U`.
    Unit,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: Symbol,
    pub ty: TypeRef,
    pub vis: Visibility,
    /// The type's range.
    pub range: TextRange,
    pub name_range: TextRange,
    /// `a: i32`
    pub def_range: TextRange,
}

impl Struct {
    pub fn field(&self, name: Symbol) -> Option<(usize, &Field)> {
        self.fields.iter().enumerate().find(|(_, f)| f.name == name)
    }

    pub fn is_enum(&self) -> bool {
        self.variants.is_some()
    }

    /// Variant `index` of an enum, or the struct itself for index 0.
    pub fn variant(&self, index: u32) -> VariantData<'_> {
        match &self.variants {
            Some(variants) => {
                let v = &variants[index as usize];
                VariantData {
                    name: v.name,
                    kind: v.kind,
                    fields: &v.fields,
                    enum_name: Some(self.name),
                }
            }
            None => VariantData {
                name: self.name,
                kind: self.kind,
                fields: &self.fields,
                enum_name: None,
            },
        }
    }

    /// An enum's variant called `name`, by index.
    pub fn variant_named(&self, name: Symbol) -> Option<u32> {
        self.variants
            .as_ref()?
            .iter()
            .position(|v| v.name == name)
            .map(|i| i as u32)
    }

    /// How many variants it has: one for a struct.
    pub fn variant_count(&self) -> usize {
        self.variants.as_ref().map_or(1, |v| v.len())
    }

    /// Whether an enum's discriminants are known only once constants
    /// are evaluated: one is written as an expression other than a
    /// literal.
    pub fn has_const_discriminants(&self) -> bool {
        self.variants
            .iter()
            .flatten()
            .any(|v| matches!(v.discriminant, Discriminant::Const { .. }))
    }
}

/// A function: one with a body, or one declared in an `extern` block.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: Symbol,
    /// Its own, not its `impl` block's.
    pub generics: Generics,
    pub params: Box<[Param]>,
    pub ret: TypeRef,
    /// The return type's range, or the name's when it is implicitly `()`.
    pub ret_range: TextRange,
    pub loc: ItemLoc,
    /// Declared `unsafe fn`, or inside an `unsafe extern` block: calling
    /// it needs an `unsafe` block.
    pub is_unsafe: bool,
    /// Declared in an `extern` block, so defined elsewhere (in C).
    pub is_extern: bool,
    /// `const fn`: callable in constants.
    pub is_const: bool,
    /// `#[rustc_inherit_overflow_checks]`, which only `core` may write:
    /// arithmetic overflow in it is checked where its caller's would be.
    pub inherits_overflow_checks: bool,
    /// `#[rustc_const_unstable]`, which only `core` may write: a `const
    /// fn` in rustc's `core` that only unstable Rust may call in
    /// constants (one whose bounds are conditionally const). Not
    /// `is_const`.
    pub const_unstable: bool,
    /// `#[rustc_allow_const_fn_unstable(const_precise_live_drops)]`,
    /// which only `core` may write: as a `const fn`, it may drop only
    /// values that need no dropping by then, which drop elaboration
    /// finds, rather than values that may need it at all.
    pub precise_live_drops: bool,
    /// `#[rustc_intrinsic]`, which only `core` may write: the compiler
    /// replaces its calls with what it does once their types are known
    /// (`mono::expand_intrinsics`), so its body never runs.
    pub intrinsic: bool,
    /// Takes more arguments after `params`, like C's `printf(fmt, ...)`.
    pub is_variadic: bool,
    /// The `impl` block or trait it is in, if any.
    pub owner: Option<Owner>,
    /// Has a `self` parameter, which is `params[0]`: a method.
    pub has_self: bool,
    pub has_body: bool,
    /// The `FN` node in `loc.file`, for lowering its body.
    pub node: NodeId,
    /// The name's range, for diagnostics.
    pub name_range: TextRange,
    /// From the first keyword to the end of the signature, where rustc
    /// reports errors about the whole function: `fn f(x: i32) -> i32`.
    pub def_range: TextRange,
    /// `def_range` with the `where` clause, and the `;` of a function
    /// without a body: where rustc reports an associated type in its
    /// bounds (or its block's or trait's) whose trait does not hold.
    pub sig_range: TextRange,
    /// The whole item, body included.
    pub item_range: TextRange,
    /// `(x: i32)`
    pub param_list: TextRange,
    /// Made by `#[derive]` on this struct, with a body made for its
    /// fields: `clone`, `eq`, `Eq`'s check, `partial_cmp` or `cmp`.
    /// `Derive::Copy` is a `clone`
    /// that copies, for a struct without parameters that derives `Copy`
    /// too, as rustc's does.
    pub derived: Option<(StructId, Derive)>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub ty: TypeRef,
    /// The binding's name, if the pattern is one: `x` in `x: i32`,
    /// `self`.
    pub name: Option<Symbol>,
    /// The type's range; `self`'s whole.
    pub range: TextRange,
    /// `x: i32`
    pub whole: TextRange,
}

/// An item's generic parameters, `<'a, T, U>`, and the bounds on them,
/// from the list and its `where` clause: lifetime and type parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Generics {
    /// The type parameters.
    pub params: Box<[GenericParam]>,
    /// The lifetime parameters, `'a`.
    pub lifetimes: Box<[LifetimeParam]>,
    /// In source order: `T: A + B` in the list, then the `where` clause.
    pub predicates: Box<[WherePredicate]>,
    /// `'a: 'b`, in the list (`<'a: 'b>`) or the `where` clause.
    pub outlives: Box<[OutlivesPredicate]>,
    /// `T: 'a`, in the list or the `where` clause.
    pub type_outlives: Box<[TypeOutlives]>,
    /// From `<` to `>`; empty if there is no list.
    pub range: TextRange,
}

/// A lifetime as written: `'a`, `'static` or `'_`, with its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LifetimeRef {
    pub name: Symbol,
    pub range: TextRange,
}

impl LifetimeRef {
    pub fn is_static(&self) -> bool {
        self.name == kw::StaticLifetime
    }

    /// `'_`
    pub fn is_anonymous(&self) -> bool {
        self.name == kw::UnderscoreLifetime
    }
}

/// A lifetime parameter, `'a` in `<'a>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifetimeParam {
    pub name: Symbol,
    pub range: TextRange,
}

/// `'a: 'b`: lifetime `longer` outlives `shorter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlivesPredicate {
    pub longer: LifetimeRef,
    pub shorter: LifetimeRef,
}

/// `T: 'a`: every reference in type `ty` outlives `lifetime`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeOutlives {
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub lifetime: LifetimeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Symbol,
    pub range: TextRange,
    /// `= Self` in `trait Add<Rhs = Self>`: traits' parameters only.
    pub default: Option<(TypeRef, TextRange)>,
}

/// Where defaults for type parameters may be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Defaults {
    /// A trait's: supported.
    Trait,
    /// A struct's: valid Rust, not supported yet.
    Struct,
    /// A type alias's: valid Rust, not supported yet.
    Alias,
    /// Functions' and `impl` blocks': an error.
    No,
}

impl std::fmt::Display for Generics {
    /// `<'a, T, U>`, or nothing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.params.is_empty() && self.lifetimes.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = (self.lifetimes.iter().map(|l| l.name))
            .chain(self.params.iter().map(|p| p.name))
            .map(|n| n.to_string())
            .collect();
        write!(f, "<{}>", names.join(", "))
    }
}

impl Generics {
    pub fn len(&self) -> usize {
        self.params.len()
    }

    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }
}

/// A `use` declaration's import of one name, or of all of a module's
/// names (a glob).
#[derive(Debug, Clone)]
pub struct Use {
    pub module: ModuleId,
    pub file: FileId,
    pub vis: Visibility,
    pub path: Path,
    pub kind: UseKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    /// `use a::b;` or `use a::b as c;`: the name it is imported as.
    Named(Symbol),
    /// `use a::{self};`: a module only.
    Module(Symbol),
    /// `use a::*;`
    Glob,
}

/// A path as written: `x`, `a::b::c`, `crate::m::f`, `super::f`, `::core::x`,
/// `Pair::<i32>::new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
    pub anchor: PathAnchor,
    /// `<T as Tr>` or `<T>` before the segments: a qualified path.
    pub qself: Option<Box<QSelf>>,
    pub segments: Box<[Symbol]>,
    /// Each segment's range, for diagnostics; empty for paths flux made.
    pub ranges: Box<[TextRange]>,
    /// The ranges of the `crate`, `self` or `super`s before the names.
    pub anchor_ranges: Box<[TextRange]>,
    /// The generic arguments after each segment, if any; empty if no
    /// segment has them.
    pub args: Box<[Option<GenericArgs>]>,
}

/// The start of a qualified path: `<T as Tr>` or `<T>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QSelf {
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub trait_: Option<Path>,
    /// `<T as Tr>`
    pub range: TextRange,
}

/// Generic arguments as written: `<'a, A, B>` or `::<A, B>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericArgs {
    /// `'a` in `S<'a, T>`, which come before the types.
    pub lifetimes: Box<[LifetimeRef]>,
    pub types: Box<[TypeRef]>,
    /// Each type's range.
    pub ranges: Box<[TextRange]>,
    /// `Out = i32` in `Tr<Out = i32>`, after the types.
    pub bindings: Box<[AssocBinding]>,
    /// From `<` to `>`.
    pub range: TextRange,
    /// Written `(A, B) -> C`, as a `Fn`-family trait's are: one type, the
    /// tuple of the parameters, and `Output` bound to the return type
    /// (`()` if none is written).
    pub parenthesized: bool,
}

/// `Out = i32` in a trait's generic arguments: what one of its associated
/// types is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssocBinding {
    pub name: Symbol,
    pub ty: TypeRef,
    pub name_range: TextRange,
    pub ty_range: TextRange,
}

impl AssocBinding {
    /// From its name to the end of its type.
    pub fn range(&self) -> TextRange {
        TextRange::new(self.name_range.start(), self.ty_range.end())
    }
}

/// rustc's E0229, for `Out = i32` in generic arguments that are not a
/// trait bound's.
pub const MISPLACED_BINDING: &str = "associated item constraints are not allowed here";

impl GenericArgs {
    /// Reports each of its bindings: they belong in trait bounds only.
    pub fn reject_bindings(&self, diagnostics: &mut Vec<Diagnostic>) {
        for b in self.bindings.iter() {
            diagnostics.push(Diagnostic::new(b.range(), MISPLACED_BINDING));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathAnchor {
    /// Starts at a name in scope.
    Plain,
    /// `crate::`
    Crate,
    /// `self::`
    SelfModule,
    /// `super::`, this many times.
    Super(u32),
    /// `::`, a crate's name.
    Global,
}

impl Path {
    /// Reports the bindings in any of its segments' generic arguments,
    /// for a path that is not a trait bound.
    pub fn reject_bindings(&self, diagnostics: &mut Vec<Diagnostic>) {
        for args in self.args.iter().flatten() {
            args.reject_bindings(diagnostics);
        }
    }

    pub fn single(name: Symbol) -> Path {
        Path {
            anchor: PathAnchor::Plain,
            qself: None,
            segments: Box::new([name]),
            ranges: Box::default(),
            anchor_ranges: Box::default(),
            args: Box::default(),
        }
    }

    /// The generic arguments after segment `i`, if any.
    pub fn args(&self, i: usize) -> Option<&GenericArgs> {
        self.args.get(i)?.as_ref()
    }

    /// Segment `i`'s range, or `whole` if unknown.
    pub fn range(&self, i: usize, whole: TextRange) -> TextRange {
        self.ranges.get(i).copied().unwrap_or(whole)
    }

    /// The name, if this is one plain name.
    pub fn as_name(&self) -> Option<Symbol> {
        match (self.anchor, &*self.segments) {
            (PathAnchor::Plain, [name]) if self.qself.is_none() => Some(*name),
            _ => None,
        }
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.anchor {
            PathAnchor::Plain => {}
            PathAnchor::Crate => f.write_str("crate::")?,
            PathAnchor::SelfModule => f.write_str("self::")?,
            PathAnchor::Super(n) => {
                for _ in 0..n {
                    f.write_str("super::")?;
                }
            }
            PathAnchor::Global => f.write_str("::")?,
        }
        if let Some(qself) = &self.qself {
            match &qself.trait_ {
                Some(t) => write!(f, "<{} as {t}>", qself.ty)?,
                None => write!(f, "<{}>", qself.ty)?,
            }
        }
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 || self.qself.is_some() {
                f.write_str("::")?;
            }
            write!(f, "{segment}")?;
            if let Some(args) = self.args(i) {
                f.write_str("<")?;
                let lifetimes = args.lifetimes.iter().map(|l| l.name.to_string());
                let types = lifetimes.chain(args.types.iter().map(|t| t.to_string()));
                let bindings = args
                    .bindings
                    .iter()
                    .map(|b| format!("{} = {}", b.name, b.ty));
                let all: Vec<String> = types.chain(bindings).collect();
                f.write_str(&all.join(", "))?;
                f.write_str(">")?;
            }
        }
        Ok(())
    }
}

/// A type as written in the source, not yet resolved: `u8` is a path
/// until type checking finds the primitive it names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    /// A struct (with generic arguments), a primitive, `Self`, a generic
    /// parameter.
    Path(Path),
    /// `()` or `(A, B)`.
    Tuple(Box<[TypeRef]>),
    /// `!`
    Never,
    /// `*const T` or `*mut T`.
    Ptr {
        mutable: bool,
        pointee: Box<TypeRef>,
    },
    /// `&T`, `&'a T` or `&mut T`.
    Ref {
        lifetime: Option<LifetimeRef>,
        /// The `&`'s range, where rustc reports a missing lifetime.
        amp: TextRange,
        mutable: bool,
        pointee: Box<TypeRef>,
    },
    /// `[T; N]`.
    Array { elem: Box<TypeRef>, len: ArrayLen },
    /// `[T]`.
    Slice(Box<TypeRef>),
    /// `_`: a type to infer, in a body.
    Infer,
    /// `fn(A, B) -> R`, or `unsafe fn(A) -> R`.
    FnPtr {
        params: Box<[TypeRef]>,
        ret: Box<TypeRef>,
        is_unsafe: bool,
    },
    /// Unsupported or malformed; already reported.
    Error,
}

/// The length of an array type or a repeat expression (`[x; N]`): a
/// constant expression of type `usize`, whose value type checking
/// needs. flux supports integer literals and paths to constants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrayLen {
    pub kind: ArrayLenKind,
    /// The expression's range.
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayLenKind {
    /// An integer literal, with its suffix (`4usize`) if any.
    Lit { value: u128, suffix: Option<Symbol> },
    /// A path to a constant, resolved where the type is.
    Path(Path),
    /// Unsupported or malformed; already reported.
    Error,
}

impl std::fmt::Display for ArrayLen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ArrayLenKind::Lit {
                value,
                suffix: Some(suffix),
            } => write!(f, "{value}{suffix}"),
            ArrayLenKind::Lit {
                value,
                suffix: None,
            } => write!(f, "{value}"),
            ArrayLenKind::Path(path) => write!(f, "{path}"),
            ArrayLenKind::Error => f.write_str("<error>"),
        }
    }
}

impl TypeRef {
    pub fn unit() -> TypeRef {
        TypeRef::Tuple(Box::new([]))
    }
}

impl std::fmt::Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeRef::Path(path) => write!(f, "{path}"),
            TypeRef::Tuple(fields) => {
                f.write_str("(")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{field}")?;
                }
                if fields.len() == 1 {
                    f.write_str(",")?;
                }
                f.write_str(")")
            }
            TypeRef::Never => f.write_str("!"),
            TypeRef::FnPtr {
                params,
                ret,
                is_unsafe,
            } => {
                if *is_unsafe {
                    f.write_str("unsafe ")?;
                }
                let params: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({})", params.join(", "))?;
                if **ret != TypeRef::unit() {
                    write!(f, " -> {ret}")?;
                }
                Ok(())
            }
            TypeRef::Ptr { mutable, pointee } => {
                let m = if *mutable { "mut" } else { "const" };
                write!(f, "*{m} {pointee}")
            }
            TypeRef::Ref {
                lifetime,
                mutable,
                pointee,
                ..
            } => {
                f.write_str("&")?;
                if let Some(lifetime) = lifetime {
                    write!(f, "{} ", lifetime.name)?;
                }
                let m = if *mutable { "mut " } else { "" };
                write!(f, "{m}{pointee}")
            }
            TypeRef::Array { elem, len } => write!(f, "[{elem}; {len}]"),
            TypeRef::Slice(elem) => write!(f, "[{elem}]"),
            TypeRef::Infer => f.write_str("_"),
            TypeRef::Error => f.write_str("<error>"),
        }
    }
}

impl ItemTree {
    /// Collects the items of the crate whose root is `root`, reading the
    /// files of its modules, and resolves their names: after a copy of
    /// `core`'s, if `with_core`.
    pub fn new(db: &mut SourceDb, root: FileId, crate_name: Symbol, with_core: bool) -> ItemTree {
        let mut items = if with_core {
            crate::Core::get().items.clone()
        } else {
            ItemTree::default()
        };
        items.core = items.counts();
        items.add_crate(db, root, crate_name, false);
        items
    }

    /// `core`'s items alone, frozen, for [`Core`](crate::Core).
    pub(crate) fn core(db: &mut SourceDb) -> ItemTree {
        let mut items = ItemTree::default();
        items.add_crate(db, FileId::CORE_ROOT, sym::core, true);
        items.modules.freeze();
        items.functions.freeze();
        items.structs.freeze();
        items.traits.freeze();
        items.impls.freeze();
        items.consts.freeze();
        items.type_aliases.freeze();
        items
    }

    /// Collects a crate after the ones already here, and resolves its
    /// names. It may implement their traits, but cannot change their
    /// items or scopes: `core` sees no other crate.
    fn add_crate(&mut self, db: &mut SourceDb, root: FileId, name: Symbol, is_core: bool) {
        let from = self.counts();
        let mut collector = Collector {
            db,
            items: self,
            derives: Vec::new(),
            block_counts: rustc_hash::FxHashMap::default(),
        };
        collector.collect_crate(root, name, is_core);
        collector.expand_derives();
        crate::def_map::resolve(self, from);
    }

    /// How many items of each kind it has.
    pub fn counts(&self) -> ItemCounts {
        ItemCounts {
            modules: self.modules.len() as u32,
            functions: self.functions.len() as u32,
            structs: self.structs.len() as u32,
            traits: self.traits.len() as u32,
            impls: self.impls.len() as u32,
            consts: self.consts.len() as u32,
            type_aliases: self.type_aliases.len() as u32,
            uses: self.uses.len() as u32,
        }
    }

    pub fn module_of(&self, krate: CrateId) -> ModuleId {
        self.crates[krate.0 as usize].root
    }

    /// The crate being compiled, rather than `core`.
    pub fn main_crate(&self) -> CrateId {
        CrateId(self.crates.len() as u32 - 1)
    }

    pub fn is_core(&self, module: ModuleId) -> bool {
        self.crates[self.modules[module].krate.0 as usize].is_core
    }

    /// `module`, or for a block, the module it is in: what `self` names.
    pub fn named_module(&self, module: ModuleId) -> ModuleId {
        let mut module = module;
        while self.modules[module].block.is_some() {
            match self.modules[module].parent {
                Some(parent) => module = parent,
                None => break,
            }
        }
        module
    }

    /// `f`, `S::f` for an associated function, `Tr::f` for a trait's
    /// and `<S as Tr>::f` for one in a trait's `impl` block.
    pub fn function_path(&self, function: &Function) -> String {
        let imp = match function.owner {
            Some(Owner::Trait(t)) => return format!("{}::{}", self.traits[t].name, function.name),
            Some(Owner::Impl(imp)) => &self.impls[imp],
            None => return function.name.to_string(),
        };
        if let Some(r) = &imp.trait_ref {
            return format!("<{} as {}>::{}", imp.self_ty, r.path, function.name);
        }
        match imp.target {
            Some(ImplTarget::Struct(strukt)) => {
                format!("{}::{}", self.structs[strukt].name, function.name)
            }
            Some(ImplTarget::Primitive(name)) => format!("{name}::{}", function.name),
            None => function.name.to_string(),
        }
    }

    /// The `impl` block `function` is in, if any.
    pub fn impl_of(&self, function: FunctionId) -> Option<ImplId> {
        self.functions[function].owner.and_then(Owner::as_impl)
    }

    /// The generic parameters of `owner`, before an associated item's own:
    /// an `impl` block's, or a trait's after its `Self`.
    pub fn owner_generics(&self, owner: Option<Owner>) -> Option<&Generics> {
        match owner? {
            Owner::Impl(imp) => Some(&self.impls[imp].generics),
            Owner::Trait(t) => Some(&self.traits[t].generics),
        }
    }

    /// The associated function `name` of `target`, from any of its
    /// `impl` blocks.
    pub fn assoc_function(&self, target: ImplTarget, name: Symbol) -> Option<FunctionId> {
        match self.assoc_item(target, name)? {
            AssocItem::Function(f) => Some(f),
            AssocItem::Const(_) => None,
        }
    }

    /// Every associated function `name` of `target`: more than one only
    /// for a generic struct, in blocks for different instances of it.
    pub fn assoc_functions(&self, target: ImplTarget, name: Symbol) -> Vec<FunctionId> {
        let items = self.assoc.get(&(target, name)).map_or(&[][..], |v| v);
        items
            .iter()
            .filter_map(|&item| match item {
                AssocItem::Function(f) => Some(f),
                AssocItem::Const(_) => None,
            })
            .collect()
    }

    /// The associated function or constant `name` of `target`; a
    /// function if there are both (reported).
    pub fn assoc_item(&self, target: ImplTarget, name: Symbol) -> Option<AssocItem> {
        let items = self.assoc.get(&(target, name))?;
        items
            .iter()
            .copied()
            .find(|item| matches!(item, AssocItem::Function(_)))
            .or_else(|| items.first().copied())
    }

    /// Whether code in `from` may name an item with visibility `vis`.
    pub fn is_visible(&self, vis: Visibility, from: ModuleId) -> bool {
        match vis {
            Visibility::Public => true,
            Visibility::Crate(krate) => self.modules[from].krate == krate,
            Visibility::Module(module) => {
                let mut m = Some(from);
                while let Some(id) = m {
                    if id == module {
                        return true;
                    }
                    m = self.modules[id].parent;
                }
                false
            }
        }
    }

    /// Whether `module` is in the crate being compiled.
    pub fn in_main_crate(&self, module: ModuleId) -> bool {
        self.modules[module].krate == self.main_crate()
    }

    /// Constant `id`'s name in messages: `N`, `_`, or rustc's
    /// `m::E::A::{constant#0}` for a discriminant's, with the path of
    /// the enum's module in its crate.
    pub fn const_name(&self, id: ConstId) -> String {
        let konst = &self.consts[id];
        match (konst.name, konst.discriminant) {
            (Some(name), _) => name.to_string(),
            (None, Some((strukt, variant))) => {
                let mut path = vec![format!(
                    "{}::{{constant#0}}",
                    self.structs[strukt].variant(variant).path()
                )];
                let named = self.named_module(self.structs[strukt].loc.module);
                let mut module = &self.modules[named];
                while let Some(parent) = module.parent {
                    path.push(module.name.to_string());
                    module = &self.modules[parent];
                }
                path.reverse();
                path.join("::")
            }
            (None, None) => "_".to_owned(),
        }
    }
}

/// Reads crates' files and collects their items.
struct Collector<'a> {
    db: &'a mut SourceDb,
    items: &'a mut ItemTree,
    /// The `#[derive]`s of a crate's structs, made into `impl` blocks
    /// after its other items, as rustc expands them.
    derives: Vec<(Ctx, StructId, Derive, TextRange, NodeId)>,
    /// How many blocks with items each function has had, for their
    /// modules' names.
    block_counts: rustc_hash::FxHashMap<Option<FunctionId>, u32>,
}

/// What collecting is inside of.
#[derive(Clone, Copy)]
struct Ctx {
    module: ModuleId,
    file: FileId,
}

impl Collector<'_> {
    /// Gives the diagnostics added since `start` without a file `file`.
    fn tag(&mut self, start: usize, file: FileId) {
        for d in &mut self.items.diagnostics[start..] {
            if d.file == FileId::UNKNOWN {
                d.file = file;
            }
        }
    }

    fn collect_crate(&mut self, root: FileId, name: Symbol, is_core: bool) {
        let krate = CrateId(self.items.crates.len() as u32);
        let dir = self
            .db
            .file(root)
            .path
            .parent()
            .map_or_else(PathBuf::new, FsPath::to_path_buf);
        let module = self.items.modules.alloc(Module {
            name,
            parent: None,
            krate,
            file: root,
            dir,
            vis: Visibility::Public,
            def_range: TextRange::empty(0),
            block: None,
        });
        self.items.crates.push(CrateData {
            name,
            root: module,
            is_core,
        });
        self.collect_file(module, root);
    }

    /// The items of a whole file, the contents of `module`.
    fn collect_file(&mut self, module: ModuleId, file: FileId) {
        let start = self.items.diagnostics.len();
        let shared = self.db.shared(file);
        let tree = shared.parse.tree();
        let source = ast::SourceFile::cast(tree.root()).expect("the root is a SOURCE_FILE");
        let is_core = self.items.is_core(module);
        check_inner_attrs(source.attrs(), is_core, &mut self.items.diagnostics);
        let items: Vec<NodeId> = source.items().map(|i| i.syntax().id()).collect();
        self.collect_items(Ctx { module, file }, &items);
        self.tag(start, file);
    }

    /// Items by node, so that collecting them can read more files.
    fn collect_items(&mut self, ctx: Ctx, items: &[NodeId]) {
        for &node in items {
            let start = self.items.diagnostics.len();
            self.collect_item(ctx, node);
            self.tag(start, ctx.file);
        }
    }

    fn collect_item(&mut self, ctx: Ctx, node: NodeId) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let item = ast::Item::cast(tree.node(node)).expect("items are ITEM nodes");
        let what = match item {
            ast::Item::Fn(f) => {
                self.collect_fn(ctx, f, false, None);
                return;
            }
            ast::Item::Impl(imp) => {
                self.collect_impl(ctx, imp);
                return;
            }
            ast::Item::ExternBlock(block) => {
                self.collect_extern_block(ctx, block);
                return;
            }
            ast::Item::Struct(s) => {
                self.collect_struct(ctx, s);
                return;
            }
            ast::Item::Module(m) => {
                self.collect_module(ctx, m);
                return;
            }
            ast::Item::Use(u) => {
                self.collect_use(ctx, u);
                return;
            }
            ast::Item::Const(c) => {
                self.collect_const(ctx, c, None);
                return;
            }
            ast::Item::Enum(e) => {
                self.collect_enum(ctx, e);
                return;
            }
            ast::Item::ExternCrate(_) => "`extern crate` declarations",
            ast::Item::MacroCall(_) => "macros",
            ast::Item::Static(st) => {
                self.collect_static(ctx, st);
                return;
            }
            ast::Item::Trait(t) => {
                self.collect_trait(ctx, t);
                return;
            }
            ast::Item::TypeAlias(t) => {
                self.collect_type_alias(ctx, t);
                return;
            }
        };
        unsupported(
            &mut self.items.diagnostics,
            item.syntax().text_range(),
            what,
        );
    }

    fn visibility(&mut self, ctx: Ctx, vis: Option<ast::Visibility<'_>>) -> Visibility {
        // An item in a block is private to the module the block is in, as
        // in rustc: blocks scope names, not privacy.
        let ctx = Ctx {
            module: self.items.named_module(ctx.module),
            ..ctx
        };
        let private = Visibility::Module(ctx.module);
        let Some(vis) = vis else {
            return private;
        };
        if vis.l_paren_token().is_none() {
            return Visibility::Public;
        }
        let module = &self.items.modules[ctx.module];
        if vis.crate_token().is_some() {
            Visibility::Crate(module.krate)
        } else if vis.self_token().is_some() {
            private
        } else if vis.super_token().is_some() {
            match module.parent {
                Some(parent) => Visibility::Module(parent),
                None => {
                    let range = vis
                        .super_token()
                        .map_or(vis.syntax().text_range(), |t| t.text_range());
                    let message = "too many leading `super` keywords within `crate`";
                    self.items.diagnostics.push(Diagnostic::new(range, message));
                    private
                }
            }
        } else {
            self.restricted_visibility(ctx, vis).unwrap_or(private)
        }
    }

    /// `pub(in path)`: visible in an ancestor module, named by `path`.
    fn restricted_visibility(&mut self, ctx: Ctx, vis: ast::Visibility<'_>) -> Option<Visibility> {
        let path = lower_path(vis.path()?, &mut self.items.diagnostics)?;
        let whole = vis.path()?.syntax().text_range();
        let mut module = match path.anchor {
            PathAnchor::Crate => self.items.module_of(self.items.modules[ctx.module].krate),
            PathAnchor::SelfModule => ctx.module,
            PathAnchor::Super(n) => {
                let mut module = ctx.module;
                for _ in 0..n {
                    module = self.items.modules[module].parent?;
                }
                module
            }
            // `pub(in self)`
            PathAnchor::Plain if path.as_name() == Some(kw::SelfLower) => {
                return Some(Visibility::Module(ctx.module));
            }
            PathAnchor::Plain | PathAnchor::Global => {
                let message =
                    "relative paths are not supported in visibilities in 2018 edition or later";
                self.items.diagnostics.push(Diagnostic::new(whole, message));
                return None;
            }
        };
        for (i, &name) in path.segments.iter().enumerate() {
            let child = self
                .items
                .modules
                .iter()
                .find(|(_, m)| m.parent == Some(module) && m.name == name);
            match child {
                Some((id, _)) => module = id,
                None => {
                    let message =
                        format!("cannot find `{name}` in {}", self.items.module_desc(module));
                    let range = path.range(i, whole);
                    self.items.diagnostics.push(Diagnostic::new(range, message));
                    return None;
                }
            }
        }
        let vis = Visibility::Module(module);
        if !self.items.is_visible(vis, ctx.module) {
            let message = "visibilities can only be restricted to ancestor modules";
            self.items.diagnostics.push(Diagnostic::new(whole, message));
            return None;
        }
        Some(vis)
    }

    /// The visibility of an associated item: as written in an inherent
    /// `impl` block, and the trait's in a trait or its `impl` blocks,
    /// where none may be written.
    fn item_visibility(
        &mut self,
        ctx: Ctx,
        vis: Option<ast::Visibility<'_>>,
        owner: Option<Owner>,
    ) -> Visibility {
        let trait_vis = match owner {
            Some(Owner::Trait(t)) => self.items.traits[t].loc.vis,
            Some(Owner::Impl(imp)) if self.items.impls[imp].trait_ref.is_some() => {
                Visibility::Public
            }
            _ => return self.visibility(ctx, vis),
        };
        if let Some(vis) = vis {
            self.items.diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        trait_vis
    }

    /// `type Pair<T> = (T, T);`, outside traits and `impl` blocks.
    fn collect_type_alias(&mut self, ctx: Ctx, t: ast::TypeAlias<'_>) {
        check_attrs(t.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, t.visibility());
        let Some(name) = t.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let item_range = TextRange::new(def_start(t.syntax()), t.syntax().text_range().end());
        if let Some(list) = t.type_bound_list() {
            diagnostics.push(Diagnostic::new(
                list.syntax().text_range(),
                "bounds on `type`s in this context have no effect",
            ));
        }
        // Bounds on its parameters are not enforced, as in rustc: they
        // are kept only to resolve their traits.
        let mut generics = collect_generics(
            t.generic_param_list(),
            t.where_clauses().next(),
            Defaults::Alias,
            diagnostics,
        );
        if let Some(clause) = t.where_clauses().nth(1) {
            let mut more = collect_generics(None, Some(clause), Defaults::No, diagnostics);
            let mut predicates = generics.predicates.into_vec();
            predicates.extend(std::mem::take(&mut more.predicates));
            generics.predicates = predicates.into();
        }
        let (ty, ty_range) = match t.ty() {
            Some(ty) => (lower_type(Some(ty), diagnostics), ty.syntax().text_range()),
            None => {
                diagnostics.push(Diagnostic::new(item_range, "free type alias without body"));
                (TypeRef::Error, item_range)
            }
        };
        let name_range = name.syntax().text_range();
        self.items.type_aliases.alloc(TypeAlias {
            name: Symbol::intern(name.text()),
            generics,
            ty,
            ty_range,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range: TextRange::new(def_start(t.syntax()), name_range.end()),
        });
    }

    /// `type Out: Bound;` in trait `id`.
    fn collect_trait_type(&mut self, id: TraitId, t: ast::TypeAlias<'_>) {
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(t.attrs(), diagnostics);
        let Some(name) = t.name() else {
            return;
        };
        let item_range = TextRange::new(def_start(t.syntax()), t.syntax().text_range().end());
        if let Some(vis) = t.visibility() {
            diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        if let Some(list) = t.generic_param_list() {
            unsupported(
                diagnostics,
                list.syntax().text_range(),
                "generic associated types",
            );
        }
        if let Some(clause) = t.where_clauses().next() {
            unsupported(
                diagnostics,
                clause.syntax().text_range(),
                "`where` clauses on associated types",
            );
        }
        if t.ty().is_some() {
            diagnostics.push(Diagnostic::new(
                item_range,
                "associated type defaults are unstable",
            ));
        }
        let mut bounds = Vec::new();
        if let Some(list) = t.type_bound_list() {
            lower_bounds(list, &mut bounds, None, diagnostics);
        }
        let name_range = name.syntax().text_range();
        let name = Symbol::intern(name.text());
        let data = &mut self.items.traits[id];
        if data.types.iter().any(|ty| ty.name == name) {
            diagnostics.push(Diagnostic::new(
                item_range,
                format!("the name `{name}` is defined multiple times"),
            ));
            return;
        }
        data.types.push(TraitType {
            name,
            bounds: bounds.into(),
            name_range,
            item_range,
        });
    }

    /// `type Out = T;` in `impl` block `id`.
    fn collect_impl_type(&mut self, id: ImplId, t: ast::TypeAlias<'_>, is_trait_impl: bool) {
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(t.attrs(), diagnostics);
        let item_range = TextRange::new(def_start(t.syntax()), t.syntax().text_range().end());
        let Some(name) = t.name() else {
            return;
        };
        if !is_trait_impl {
            diagnostics.push(Diagnostic::new(
                item_range,
                "inherent associated types are unstable",
            ));
            return;
        }
        if let Some(vis) = t.visibility() {
            diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        let generic = t.generic_param_list();
        if let Some(list) = generic {
            unsupported(
                diagnostics,
                list.syntax().text_range(),
                "generic associated types",
            );
        }
        if let Some(list) = t.type_bound_list() {
            diagnostics.push(Diagnostic::new(
                list.syntax().text_range(),
                "bounds on `type`s in `impl`s have no effect",
            ));
        }
        let (ty, ty_range) = match t.ty() {
            Some(ty) => {
                let range = ty.syntax().text_range();
                match generic {
                    // Reported; defined, as far as the trait is concerned.
                    Some(_) => (TypeRef::Error, range),
                    None => (lower_type(Some(ty), diagnostics), range),
                }
            }
            None => {
                // Reported; defined too.
                diagnostics.push(Diagnostic::new(
                    item_range,
                    "associated type in `impl` without body",
                ));
                (TypeRef::Error, item_range)
            }
        };
        let name_range = name.syntax().text_range();
        let name = Symbol::intern(name.text());
        let imp = &mut self.items.impls[id];
        if imp.types.iter().any(|t| t.name == name) {
            diagnostics.push(Diagnostic::new(
                item_range,
                format!("duplicate definitions with name `{name}`:"),
            ));
            return;
        }
        imp.types.push(ImplType {
            name,
            ty,
            ty_range,
            name_range,
            item_range,
        });
    }

    fn collect_trait(&mut self, ctx: Ctx, t: ast::Trait<'_>) {
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        let mut on_unimplemented = None;
        for attr in t.attrs() {
            if let Some(message) = on_unimplemented_attr(attr) {
                on_unimplemented = message;
                continue;
            }
            match lang_attr(attr) {
                Some(name) if is_core => lang = Some(name),
                _ => check_attr(attr, &mut self.items.diagnostics),
            }
        }
        let vis = self.visibility(ctx, t.visibility());
        let Some(name) = t.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let generics = collect_generics(
            t.generic_param_list(),
            t.where_clause(),
            Defaults::Trait,
            diagnostics,
        );
        let mut supertraits = Vec::new();
        if let Some(list) = t.type_bound_list() {
            lower_bounds(list, &mut supertraits, None, diagnostics);
        }
        let name_range = name.syntax().text_range();
        let id = self.items.traits.alloc(Trait {
            name: Symbol::intern(name.text()),
            generics,
            supertraits: supertraits.into(),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            is_unsafe: t.unsafe_token().is_some(),
            lang,
            on_unimplemented,
            functions: Vec::new(),
            consts: Vec::new(),
            types: Vec::new(),
            name_range,
            def_range: TextRange::new(def_start(t.syntax()), name_range.end()),
        });
        if let Some(lang) = lang {
            self.items.lang_traits.insert(lang, id);
        }
        let Some(list) = t.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        for item in list.items() {
            let what = match item {
                ast::Item::Fn(f) => {
                    if let Some(function) = self.collect_fn(ctx, f, false, Some(Owner::Trait(id))) {
                        let data = &self.items.functions[function];
                        let duplicate =
                            self.items.traits[id].value(self.items, data.name).is_some();
                        if duplicate {
                            // rustc's span has a bodiless item's `;`.
                            let range = if data.has_body {
                                data.def_range
                            } else {
                                data.item_range
                            };
                            let name = data.name;
                            self.items.diagnostics.push(Diagnostic::new(
                                range,
                                format!("the name `{name}` is defined multiple times"),
                            ));
                        } else {
                            self.items.traits[id].functions.push(function);
                            let name = self.items.functions[function].name;
                            self.items.trait_fns.entry(name).or_default().push(id);
                        }
                    }
                    continue;
                }
                ast::Item::Const(c) => {
                    let Some(konst) = self.collect_const(ctx, c, Some(Owner::Trait(id))) else {
                        continue;
                    };
                    let data = &self.items.consts[konst];
                    let Some(name) = data.name else {
                        continue;
                    };
                    if self.items.traits[id].value(self.items, name).is_some() {
                        self.items.diagnostics.push(Diagnostic::new(
                            data.item_range,
                            format!("the name `{name}` is defined multiple times"),
                        ));
                    } else {
                        self.items.traits[id].consts.push(konst);
                        self.items.trait_fns.entry(name).or_default().push(id);
                    }
                    continue;
                }
                ast::Item::TypeAlias(t) => {
                    self.collect_trait_type(id, t);
                    continue;
                }
                ast::Item::MacroCall(_) => "macros",
                _ => "items like this in traits",
            };
            unsupported(
                &mut self.items.diagnostics,
                item.syntax().text_range(),
                what,
            );
        }
    }

    fn collect_module(&mut self, ctx: Ctx, m: ast::Module<'_>) {
        check_attrs(m.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, m.visibility());
        let Some(name) = m.name() else {
            return;
        };
        let name_text = name.text();
        let parent = &self.items.modules[ctx.module];
        if parent.block.is_some() && m.item_list().is_none() {
            let range = TextRange::new(def_start(m.syntax()), m.syntax().text_range().end());
            self.items.diagnostics.push(Diagnostic::new(
                range,
                "cannot declare a file module inside a block unless it has a path attribute",
            ));
            return;
        }
        let def_range = TextRange::new(def_start(m.syntax()), name.syntax().text_range().end());
        let child_dir = parent.dir.join(name_text);
        let krate = parent.krate;
        let parent_dir = parent.dir.clone();
        let module = self.items.modules.alloc(Module {
            name: Symbol::intern(name_text),
            parent: Some(ctx.module),
            krate,
            file: ctx.file,
            dir: child_dir.clone(),
            vis,
            def_range,
            block: None,
        });
        match m.item_list() {
            Some(list) => {
                check_inner_attrs(list.attrs(), false, &mut self.items.diagnostics);
                let items: Vec<NodeId> = list.items().map(|i| i.syntax().id()).collect();
                self.collect_items(
                    Ctx {
                        module,
                        file: ctx.file,
                    },
                    &items,
                );
            }
            None => {
                // `mod m;` is `m.rs` or `m/mod.rs` next to the module's
                // files; the files of `m`'s own children are in `m/`.
                let flat = parent_dir.join(format!("{name_text}.rs"));
                let nested = child_dir.join("mod.rs");
                let (flat_exists, nested_exists) = (self.db.exists(&flat), self.db.exists(&nested));
                let path = match (flat_exists, nested_exists) {
                    (true, false) => flat,
                    (false, true) => nested,
                    (true, true) => {
                        let message = format!(
                            "file for module `{name_text}` found at both \"{}\" and \"{}\"",
                            flat.display(),
                            nested.display()
                        );
                        self.items
                            .diagnostics
                            .push(Diagnostic::new(def_range, message));
                        return;
                    }
                    (false, false) => {
                        let message = format!("file not found for module `{name_text}`");
                        self.items
                            .diagnostics
                            .push(Diagnostic::new(def_range, message));
                        return;
                    }
                };
                match self.db.load(&path) {
                    Ok(file) => {
                        self.items.modules[module].file = file;
                        self.collect_file(module, file);
                    }
                    Err(message) => self
                        .items
                        .diagnostics
                        .push(Diagnostic::new(def_range, message)),
                }
            }
        }
    }

    fn collect_use(&mut self, ctx: Ctx, u: ast::Use<'_>) {
        check_attrs(u.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, u.visibility());
        if let Some(tree) = u.use_tree() {
            let prefix = Path {
                anchor: PathAnchor::Plain,
                qself: None,
                segments: Box::default(),
                ranges: Box::default(),
                anchor_ranges: Box::default(),
                args: Box::default(),
            };
            self.collect_use_tree(ctx, vis, &prefix, tree);
        }
    }

    /// Flattens `use a::{b, c::d as e, f::*}` into one import per name.
    fn collect_use_tree(
        &mut self,
        ctx: Ctx,
        vis: Visibility,
        prefix: &Path,
        tree: ast::UseTree<'_>,
    ) {
        let range = tree.syntax().text_range();
        let mut path = prefix.clone();
        if let Some(p) = tree.path() {
            let Some(p) = lower_path(p, &mut self.items.diagnostics) else {
                return;
            };
            if prefix.segments.is_empty() && prefix.anchor == PathAnchor::Plain {
                path = p;
            } else if p.anchor == PathAnchor::Plain {
                let segments: Vec<Symbol> = prefix
                    .segments
                    .iter()
                    .chain(p.segments.iter())
                    .copied()
                    .collect();
                let ranges: Vec<TextRange> = prefix
                    .ranges
                    .iter()
                    .chain(p.ranges.iter())
                    .copied()
                    .collect();
                path.segments = segments.into();
                path.ranges = ranges.into();
            } else {
                let message = "`crate`, `self` and `super` can only start a path";
                self.items.diagnostics.push(Diagnostic::new(range, message));
                return;
            }
        }
        if tree.star_token().is_some() {
            self.items.uses.push(Use {
                module: ctx.module,
                file: ctx.file,
                vis,
                path,
                kind: UseKind::Glob,
                range,
            });
            return;
        }
        if let Some(list) = tree.use_tree_list() {
            for child in list.use_trees() {
                self.collect_use_tree(ctx, vis, &path, child);
            }
            return;
        }
        // `use m::{self};` imports the module `m`.
        let module_only = path.segments.last() == Some(&kw::SelfLower);
        if module_only {
            let mut segments = path.segments.to_vec();
            segments.pop();
            path.segments = segments.into();
            let mut ranges = path.ranges.to_vec();
            ranges.pop();
            path.ranges = ranges.into();
        }
        let rename = match tree.rename() {
            Some(rename) => match rename.name() {
                Some(name) => Some(Symbol::intern(name.text())),
                None => {
                    unsupported(&mut self.items.diagnostics, range, "`as _` imports");
                    return;
                }
            },
            None => None,
        };
        let Some(name) = rename.or_else(|| path.segments.last().copied()) else {
            let message = "imports need to be explicitly named";
            self.items.diagnostics.push(Diagnostic::new(range, message));
            return;
        };
        let kind = if module_only {
            UseKind::Module(name)
        } else {
            UseKind::Named(name)
        };
        self.items.uses.push(Use {
            module: ctx.module,
            file: ctx.file,
            vis,
            path,
            kind,
            range,
        });
    }

    fn collect_impl(&mut self, ctx: Ctx, imp: ast::Impl<'_>) {
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(imp.attrs(), diagnostics);
        let range = imp.syntax().text_range();
        if let Some(vis) = imp.visibility() {
            diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        let generics = collect_generics(
            imp.generic_param_list(),
            imp.where_clause(),
            Defaults::No,
            diagnostics,
        );
        let trait_ref = match imp.trait_ty() {
            Some(ty) => match lower_bound_type(ty, diagnostics) {
                Some(bound) => Some(bound),
                None => return,
            },
            // `impl for S`, reported by the parser.
            None if imp.for_token().is_some() => return,
            None => None,
        };
        if trait_ref.is_none()
            && let Some(token) = imp.unsafe_token()
        {
            diagnostics.push(Diagnostic::new(
                token.text_range(),
                "inherent impls cannot be unsafe",
            ));
        }
        let self_ty = lower_type(imp.self_ty(), diagnostics);
        let self_range = imp.self_ty().map_or(range, |t| t.syntax().text_range());
        let header_end = imp
            .where_clause()
            .map_or(self_range.end(), |w| w.syntax().text_range().end());
        let def_range = TextRange::new(def_start(imp.syntax()), header_end);
        let is_trait_impl = trait_ref.is_some();
        let id = self.items.impls.alloc(Impl {
            generics,
            trait_ref,
            trait_: None,
            is_unsafe: imp.unsafe_token().is_some(),
            self_ty,
            self_range,
            def_range,
            module: ctx.module,
            file: ctx.file,
            target: None,
            functions: Vec::new(),
            consts: Vec::new(),
            types: Vec::new(),
        });
        let Some(list) = imp.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        for item in list.items() {
            let what = match item {
                ast::Item::Fn(f) => {
                    if let Some(function) = self.collect_fn(ctx, f, false, Some(Owner::Impl(id))) {
                        let (name, range) = {
                            let data = &self.items.functions[function];
                            (data.name, data.item_range)
                        };
                        if !(is_trait_impl && self.duplicate_value(id, name, range)) {
                            self.items.impls[id].functions.push(function);
                        }
                    }
                    continue;
                }
                ast::Item::Const(c) => {
                    if let Some(konst) = self.collect_const(ctx, c, Some(Owner::Impl(id))) {
                        let (name, range) = {
                            let data = &self.items.consts[konst];
                            (data.name, data.item_range)
                        };
                        let duplicate = name.is_some_and(|name| {
                            is_trait_impl && self.duplicate_value(id, name, range)
                        });
                        if !duplicate {
                            self.items.impls[id].consts.push(konst);
                        }
                    }
                    continue;
                }
                ast::Item::TypeAlias(t) => {
                    self.collect_impl_type(id, t, is_trait_impl);
                    continue;
                }
                ast::Item::MacroCall(_) => "macros",
                _ => "items like this in `impl` blocks",
            };
            unsupported(
                &mut self.items.diagnostics,
                item.syntax().text_range(),
                what,
            );
        }
    }

    /// Whether trait `impl` block `id` already has a function or constant
    /// called `name`, reported at `range` (E0201): they share a namespace.
    fn duplicate_value(&mut self, id: ImplId, name: Symbol, range: TextRange) -> bool {
        let imp = &self.items.impls[id];
        let items = &self.items;
        let exists = imp
            .functions
            .iter()
            .any(|&f| items.functions[f].name == name)
            || imp
                .consts
                .iter()
                .any(|&c| items.consts[c].name == Some(name));
        if exists {
            self.items.diagnostics.push(Diagnostic::new(
                range,
                format!("duplicate definitions with name `{name}`:"),
            ));
        }
        exists
    }

    fn collect_fn(
        &mut self,
        ctx: Ctx,
        f: ast::Fn<'_>,
        in_extern_block: bool,
        owner: Option<Owner>,
    ) -> Option<FunctionId> {
        let is_core = self.items.is_core(ctx.module);
        let mut inherits_overflow_checks = false;
        let mut const_unstable = false;
        let mut precise_live_drops = false;
        let mut intrinsic = false;
        let mut lang = None;
        for attr in f.attrs() {
            if is_core && attr_name(attr) == Some("rustc_inherit_overflow_checks") {
                inherits_overflow_checks = true;
            } else if is_core && attr_name(attr) == Some("rustc_const_unstable") {
                const_unstable = true;
            } else if is_core && attr_name(attr) == Some("rustc_allow_const_fn_unstable") {
                precise_live_drops = true;
            } else if is_core && attr_name(attr) == Some("rustc_intrinsic") {
                intrinsic = true;
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.item_visibility(ctx, f.visibility(), owner);
        let name = f.name()?;
        let (outer, outer_lifetimes): (Vec<Symbol>, Vec<Symbol>) = match f.generic_param_list() {
            Some(_) => self
                .items
                .owner_generics(owner)
                .map_or_else(Default::default, |g| {
                    (
                        g.params.iter().map(|p| p.name).collect(),
                        g.lifetimes.iter().map(|l| l.name).collect(),
                    )
                }),
            None => Default::default(),
        };
        let diagnostics = &mut self.items.diagnostics;
        let generics = collect_generics(
            f.generic_param_list(),
            f.where_clause(),
            Defaults::No,
            diagnostics,
        );
        if in_extern_block && !generics.is_empty() {
            diagnostics.push(Diagnostic::new(
                TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
                "foreign items may not have type parameters",
            ));
        }
        {
            // A method's parameters may not shadow its block's.
            for param in generics.params.iter() {
                if outer.contains(&param.name) {
                    diagnostics.push(Diagnostic::new(
                        param.range,
                        format!(
                            "the name `{}` is already used for a generic parameter in this item's generic parameters",
                            param.name
                        ),
                    ));
                }
            }
            for lifetime in generics.lifetimes.iter() {
                if outer_lifetimes.contains(&lifetime.name) {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        format!(
                            "lifetime name `{}` shadows a lifetime name that is already in scope",
                            lifetime.name
                        ),
                    ));
                }
            }
        }
        if let Some(token) = f.async_token() {
            unsupported(diagnostics, token.text_range(), "`async` functions");
        }
        if let Some(abi) = f.abi() {
            // `extern "C" fn f() {}`: a Rust function callable from C.
            unsupported(diagnostics, abi.syntax().text_range(), "`extern` functions");
        }
        let mut params = Vec::new();
        let mut is_variadic = false;
        let mut has_self = false;
        if let Some(list) = f.param_list() {
            if let Some(param) = list.self_param() {
                let range = param.syntax().text_range();
                check_inert_attrs(param.attrs(), diagnostics);
                if owner.is_none() {
                    diagnostics.push(Diagnostic::new(
                        range,
                        "`self` parameter is only allowed in associated functions",
                    ));
                } else if param.ty().is_some() {
                    unsupported(diagnostics, range, "`self` parameters with a type");
                } else {
                    let self_ty = TypeRef::Path(Path::single(kw::SelfUpper));
                    let ty = match param.amp_token() {
                        Some(_) => TypeRef::Ref {
                            lifetime: param.lifetime().map(lower_lifetime),
                            amp: param.amp_token().map_or(range, |t| t.text_range()),
                            mutable: param.mut_token().is_some(),
                            pointee: Box::new(self_ty),
                        },
                        None => self_ty,
                    };
                    params.push(Param {
                        ty,
                        range,
                        whole: range,
                        name: Some(kw::SelfLower),
                    });
                    has_self = true;
                }
            }
            let count = list.params().count();
            for (i, param) in list.params().enumerate() {
                check_inert_attrs(param.attrs(), diagnostics);
                if let Some(dots) = param.dot3_token() {
                    let message = if !in_extern_block {
                        Some("`...` is not supported for non-extern functions")
                    } else if i + 1 != count {
                        Some("`...` must be the last argument of a C-variadic function")
                    } else {
                        None
                    };
                    match message {
                        Some(message) => {
                            diagnostics.push(Diagnostic::new(dots.text_range(), message))
                        }
                        None => is_variadic = true,
                    }
                    continue;
                }
                let name = match param.pat() {
                    Some(ast::Pat::IdentPat(pat)) => pat.name().map(|n| Symbol::intern(n.text())),
                    _ => None,
                };
                params.push(Param {
                    ty: lower_type(param.ty(), diagnostics),
                    range: param
                        .ty()
                        .map_or(param.syntax().text_range(), |t| t.syntax().text_range()),
                    whole: param.syntax().text_range(),
                    name,
                });
            }
        }
        let (ret, ret_range) = match f.ret_type() {
            Some(ret) => (
                lower_type(ret.ty(), diagnostics),
                ret.ty()
                    .map_or(ret.syntax().text_range(), |t| t.syntax().text_range()),
            ),
            None => (TypeRef::unit(), name.syntax().text_range()),
        };
        let has_body = f.body().is_some();
        if matches!(owner, Some(Owner::Impl(_))) && !has_body {
            diagnostics.push(Diagnostic::new(
                TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
                "associated function in `impl` without body",
            ));
        }
        if !has_body
            && !in_extern_block
            && let Some(list) = f.param_list()
        {
            for param in list.params() {
                if let Some(pat) = param.pat()
                    && !matches!(pat, ast::Pat::IdentPat(_) | ast::Pat::WildcardPat(_))
                {
                    diagnostics.push(Diagnostic::new(
                        pat.syntax().text_range(),
                        "patterns aren't allowed in functions without bodies",
                    ));
                }
            }
        }
        if in_extern_block && has_body {
            diagnostics.push(Diagnostic::new(
                name.syntax().text_range(),
                "functions in `extern` blocks cannot have bodies",
            ));
        } else if !in_extern_block && !has_body && owner.is_none() {
            diagnostics.push(Diagnostic::new(
                name.syntax().text_range(),
                "free function without a body",
            ));
        }
        let signature_end = [
            f.ret_type().map(|r| r.syntax()),
            f.param_list().map(|p| p.syntax()),
            Some(name.syntax()),
        ]
        .into_iter()
        .flatten()
        .next()
        .map_or(0, |n| n.text_range().end());
        let sig_end = if has_body {
            f.where_clause()
                .map_or(signature_end, |w| w.syntax().text_range().end())
        } else {
            f.syntax().text_range().end()
        };
        let id = self.items.functions.alloc(Function {
            def_range: TextRange::new(def_start(f.syntax()), signature_end),
            sig_range: TextRange::new(def_start(f.syntax()), sig_end),
            item_range: TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
            param_list: f
                .param_list()
                .map_or(name.syntax().text_range(), |p| p.syntax().text_range()),
            name: Symbol::intern(name.text()),
            generics,
            params: params.into(),
            ret,
            ret_range,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            is_unsafe: in_extern_block || f.unsafe_token().is_some(),
            is_extern: in_extern_block,
            is_const: f.const_token().is_some(),
            inherits_overflow_checks,
            const_unstable,
            precise_live_drops,
            intrinsic,
            is_variadic,
            owner,
            has_self,
            has_body: has_body && !in_extern_block,
            node: f.syntax().id(),
            name_range: name.syntax().text_range(),
            derived: None,
        });
        if lang.is_some_and(|l| l.as_str() == "panic") {
            self.items.panic_fn = Some(id);
        } else if let Some(lang) = lang {
            self.items.lang_fns.insert(lang, id);
        }
        if let Some(body) = f.body() {
            self.collect_body_items(ctx, body.syntax().id(), Some(id));
        }
        Some(id)
    }

    /// The items declared in the blocks of a body, at `node` in
    /// `ctx.file`, of `function` if it is one's: each block with items
    /// becomes a module inside `ctx.module`.
    fn collect_body_items(&mut self, ctx: Ctx, node: NodeId, function: Option<FunctionId>) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        // Most bodies have no items: they are found without a walk.
        if !tree
            .node(node)
            .descendants()
            .any(|n| is_item_kind(n.kind()))
        {
            return;
        }
        let mut blocks = Vec::new();
        outer_blocks(tree.node(node), &mut blocks);
        for block in blocks {
            self.collect_block(ctx, block, function);
        }
    }

    /// Block `node`'s items, in a module of their own, then those of the
    /// blocks inside it.
    fn collect_block(&mut self, ctx: Ctx, node: NodeId, function: Option<FunctionId>) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let block = tree.node(node);
        let items: Vec<NodeId> = block
            .children()
            .filter(|c| is_item_kind(c.kind()))
            .map(|c| c.id())
            .collect();
        let mut inner = Vec::new();
        for child in block.children().filter(|c| !is_item_kind(c.kind())) {
            outer_blocks(child, &mut inner);
        }
        let ctx = if items.is_empty() {
            ctx
        } else {
            let parent = &self.items.modules[ctx.module];
            let outer = function.map_or(kw::Empty, |f| self.items.functions[f].name);
            let count = self.block_counts.entry(function).or_default();
            let index = *count;
            *count += 1;
            let module = self.items.modules.alloc(Module {
                name: Symbol::intern(&format!("{outer}.{index}")),
                parent: Some(ctx.module),
                krate: parent.krate,
                file: ctx.file,
                dir: parent.dir.clone(),
                vis: Visibility::Module(ctx.module),
                def_range: TextRange::empty(0),
                block: Some(Block { function, node }),
            });
            self.items.block_modules.insert((ctx.file, node), module);
            let ctx = Ctx {
                module,
                file: ctx.file,
            };
            self.collect_items(ctx, &items);
            ctx
        };
        for block in inner {
            self.collect_block(ctx, block, function);
        }
    }

    fn collect_const(
        &mut self,
        ctx: Ctx,
        c: ast::Const<'_>,
        owner: Option<Owner>,
    ) -> Option<ConstId> {
        check_attrs(c.attrs(), &mut self.items.diagnostics);
        let vis = self.item_visibility(ctx, c.visibility(), owner);
        let name = c.name();
        let name_end = name
            .map(|n| n.syntax().text_range().end())
            .or_else(|| c.underscore_token().map(|t| t.text_range().end()))?;
        let diagnostics = &mut self.items.diagnostics;
        let ty = match c.ty() {
            Some(ty) => lower_type(Some(ty), diagnostics),
            None => {
                diagnostics.push(Diagnostic::new(
                    TextRange::empty(name_end),
                    "missing type for `const` item",
                ));
                TypeRef::Error
            }
        };

        let has_body = c.body().is_some();
        let def_start = def_start(c.syntax());
        let header_end = c.ty().map_or(name_end, |t| t.syntax().text_range().end());
        let item_range = TextRange::new(def_start, c.syntax().text_range().end());
        if !has_body {
            let message = match owner {
                Some(Owner::Trait(_)) => None,
                Some(Owner::Impl(_)) => Some("associated constant in `impl` without body"),
                None => Some("free constant item without body"),
            };
            if let Some(message) = message {
                diagnostics.push(Diagnostic::new(item_range, message));
            }
        }
        let id = self.items.consts.alloc(Const {
            name: name.map(|n| Symbol::intern(n.text())),
            ty,
            ty_range: c
                .ty()
                .map_or(TextRange::empty(name_end), |t| t.syntax().text_range()),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            owner,
            node: c.syntax().id(),
            has_body,
            def_range: TextRange::new(def_start, header_end),
            item_range,
            discriminant: None,
            is_static: None,
        });
        if let Some(body) = c.body() {
            self.collect_body_items(ctx, body.syntax().id(), None);
        }
        Some(id)
    }

    /// `static X: T = v;` or `static mut X: T = v;`.
    fn collect_static(&mut self, ctx: Ctx, st: ast::Static<'_>) {
        check_attrs(st.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, st.visibility());
        let Some(name) = st.name() else {
            return;
        };
        let name_end = name.syntax().text_range().end();
        let diagnostics = &mut self.items.diagnostics;
        let ty = match st.ty() {
            Some(ty) => lower_type(Some(ty), diagnostics),
            None => {
                diagnostics.push(Diagnostic::new(
                    TextRange::empty(name_end),
                    "missing type for `static` item",
                ));
                TypeRef::Error
            }
        };
        let has_body = st.body().is_some();
        let def_start = def_start(st.syntax());
        let header_end = st.ty().map_or(name_end, |t| t.syntax().text_range().end());
        let item_range = TextRange::new(def_start, st.syntax().text_range().end());
        if !has_body {
            diagnostics.push(Diagnostic::new(item_range, "free static item without body"));
        }
        self.items.consts.alloc(Const {
            name: Some(Symbol::intern(name.text())),
            ty,
            ty_range: st
                .ty()
                .map_or(TextRange::empty(name_end), |t| t.syntax().text_range()),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            owner: None,
            node: st.syntax().id(),
            has_body,
            def_range: TextRange::new(def_start, header_end),
            item_range,
            discriminant: None,
            is_static: Some(st.mut_token().is_some()),
        });
        if let Some(body) = st.body() {
            self.collect_body_items(ctx, body.syntax().id(), None);
        }
    }

    fn collect_struct(&mut self, ctx: Ctx, s: ast::Struct<'_>) {
        let mut derives = Vec::new();
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        for attr in s.attrs() {
            if attr_name(attr) == Some("derive") {
                derive_attr(attr, &mut derives, &mut self.items.diagnostics);
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.visibility(ctx, s.visibility());
        let Some(name) = s.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let generics = collect_generics(
            s.generic_param_list(),
            s.where_clause(),
            Defaults::Struct,
            diagnostics,
        );
        let (kind, fields) =
            self.collect_fields(ctx, s.record_field_list(), s.tuple_field_list(), false);
        let name_range = name.syntax().text_range();
        let id = self.items.structs.alloc(Struct {
            name: Symbol::intern(name.text()),
            generics,
            kind,
            fields: fields.into(),
            variants: None,
            repr: None,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range: TextRange::new(def_start(s.syntax()), name_range.end()),
        });
        if let Some(lang) = lang {
            self.items.lang_enums.insert(lang, id);
        }
        for (derive, range) in derives {
            self.derives.push((ctx, id, derive, range, s.syntax().id()));
        }
    }

    /// The fields of a struct or variant, and so its kind. A variant's
    /// fields are as visible as its enum, and may not say otherwise.
    fn collect_fields(
        &mut self,
        ctx: Ctx,
        record: Option<ast::RecordFieldList<'_>>,
        tuple: Option<ast::TupleFieldList<'_>>,
        in_variant: bool,
    ) -> (StructKind, Vec<Field>) {
        let mut fields = Vec::new();
        let field_vis = |this: &mut Self, vis: Option<ast::Visibility<'_>>| {
            if !in_variant {
                return this.visibility(ctx, vis);
            }
            if let Some(vis) = vis {
                this.items.diagnostics.push(Diagnostic::new(
                    vis.syntax().text_range(),
                    "visibility qualifiers are not permitted here",
                ));
            }
            Visibility::Public
        };
        let kind = if let Some(list) = record {
            for field in list.fields() {
                check_inert_attrs(field.attrs(), &mut self.items.diagnostics);
                let vis = field_vis(self, field.visibility());
                let diagnostics = &mut self.items.diagnostics;
                let Some(field_name) = field.name() else {
                    continue;
                };
                let field_name_range = field_name.syntax().text_range();
                let field_name = Symbol::intern(field_name.text());
                let def_range =
                    TextRange::new(def_start(field.syntax()), field.syntax().text_range().end());
                if fields.iter().any(|f: &Field| f.name == field_name) {
                    diagnostics.push(Diagnostic::new(
                        def_range,
                        format!("field `{field_name}` is already declared"),
                    ));
                    continue;
                }
                fields.push(Field {
                    name: field_name,
                    ty: lower_type(field.ty(), diagnostics),
                    vis,
                    range: field
                        .ty()
                        .map_or(field_name_range, |t| t.syntax().text_range()),
                    name_range: field_name_range,
                    def_range,
                });
            }
            StructKind::Record
        } else if let Some(list) = tuple {
            for (i, field) in list.fields().enumerate() {
                check_inert_attrs(field.attrs(), &mut self.items.diagnostics);
                let vis = field_vis(self, field.visibility());
                let range = field.syntax().text_range();
                fields.push(Field {
                    name: Symbol::intern(&i.to_string()),
                    ty: lower_type(field.ty(), &mut self.items.diagnostics),
                    vis,
                    range: field.ty().map_or(range, |t| t.syntax().text_range()),
                    name_range: range,
                    def_range: range,
                });
            }
            StructKind::Tuple
        } else {
            StructKind::Unit
        };
        (kind, fields)
    }

    fn collect_enum(&mut self, ctx: Ctx, e: ast::Enum<'_>) {
        let mut derives = Vec::new();
        let mut hints = Vec::new();
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        for attr in e.attrs() {
            if attr_name(attr) == Some("derive") {
                derive_attr(attr, &mut derives, &mut self.items.diagnostics);
            } else if attr_name(attr) == Some("repr") {
                repr_attr(attr, &mut hints, &mut self.items.diagnostics);
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.visibility(ctx, e.visibility());
        let Some(name) = e.name() else {
            return;
        };
        let generics = collect_generics(
            e.generic_param_list(),
            e.where_clause(),
            Defaults::Struct,
            &mut self.items.diagnostics,
        );
        let def_range = TextRange::new(def_start(e.syntax()), name.syntax().text_range().end());
        let all_variants = e.variant_list().into_iter().flat_map(|l| l.variants());
        let has_fields = all_variants
            .clone()
            .any(|v| v.record_field_list().is_some() || v.tuple_field_list().is_some());
        let repr = enum_repr(
            &hints,
            has_fields,
            all_variants.clone().next().is_none(),
            &mut self.items.diagnostics,
        );
        let repr_name = repr.map_or("isize", |r| r.as_str());
        let values = int_range(repr_name);
        // Whether a discriminant is an expression other than a literal,
        // whose value is known only once constants are evaluated: then
        // duplicates are found after evaluation.
        let computed = all_variants
            .clone()
            .any(|v| v.expr().is_some_and(|e| discriminant_value(e).is_none()));
        let strukt = StructId::from_raw(self.items.structs.len() as u32);
        let generic_names: Vec<Symbol> = generics.params.iter().map(|p| p.name).collect();
        let mut variants: Vec<Variant> = Vec::new();
        // The next discriminant, `None` past the type's maximum.
        let mut next = Some(Discriminant::Value(0));
        // Whether a discriminant is written, which rustc allows on an
        // enum with fields only with an integer `repr`.
        let mut written = false;
        let mut duplicates = Vec::new();
        for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
            check_attrs(v.attrs(), &mut self.items.diagnostics);
            if let Some(vis) = v.visibility() {
                self.items.diagnostics.push(Diagnostic::new(
                    vis.syntax().text_range(),
                    "visibility qualifiers are not permitted here",
                ));
            }
            let Some(v_name) = v.name() else {
                continue;
            };
            let (kind, fields) =
                self.collect_fields(ctx, v.record_field_list(), v.tuple_field_list(), true);
            let name_range = v_name.syntax().text_range();
            let v_name = Symbol::intern(v_name.text());
            let v_range = TextRange::new(def_start(v.syntax()), v.syntax().text_range().end());
            let diagnostics = &mut self.items.diagnostics;
            let placeholder = next.unwrap_or(Discriminant::Value(0));
            // An expression, which becomes an anonymous constant.
            let mut anonymous = None;
            let mut discriminant = match v.expr() {
                Some(expr) => {
                    written = true;
                    let range = expr.syntax().text_range();
                    match discriminant_value(expr) {
                        Some((_, _, Some((suffix, at)))) if suffix.as_str() != repr_name => {
                            diagnostics.push(Diagnostic::new(at, "mismatched types"));
                            placeholder
                        }
                        Some((true, _, _)) if repr_name.starts_with('u') => {
                            diagnostics.push(Diagnostic::new(
                                range,
                                format!("cannot apply unary operator `-` to type `{repr_name}`"),
                            ));
                            placeholder
                        }
                        Some((negative, value, _)) => {
                            let value = if negative {
                                0i128.checked_sub_unsigned(value)
                            } else {
                                i128::try_from(value).ok()
                            };
                            Discriminant::Value(match value {
                                Some(value) if values.contains(&value) => value,
                                None if repr_name == "u128" && !negative => {
                                    unsupported(
                                        diagnostics,
                                        range,
                                        "discriminants above `i128::MAX`",
                                    );
                                    0
                                }
                                _ => {
                                    diagnostics.push(Diagnostic::new(
                                        range,
                                        format!("literal out of range for `{repr_name}`"),
                                    ));
                                    0
                                }
                            })
                        }
                        None => {
                            if !generic_in_discriminant(expr, &generic_names, diagnostics) {
                                anonymous = Some(range);
                            }
                            placeholder
                        }
                    }
                }
                None => match next {
                    Some(value) => value,
                    None => {
                        diagnostics
                            .push(Diagnostic::new(name_range, "enum discriminant overflowed"));
                        // As rustc, wrapped around.
                        Discriminant::Value(*values.start())
                    }
                },
            };
            if !computed
                && variants.iter().any(|p| p.discriminant == discriminant)
                && !duplicates.contains(&discriminant)
                && let Discriminant::Value(value) = discriminant
            {
                duplicates.push(discriminant);
                diagnostics.push(Diagnostic::new(
                    def_range,
                    format!("discriminant value `{value}` assigned more than once"),
                ));
            }
            if variants.iter().any(|p| p.name == v_name) {
                diagnostics.push(Diagnostic::new(
                    name_range,
                    format!("the name `{v_name}` is defined multiple times"),
                ));
                continue;
            }
            if let Some(range) = anonymous {
                let konst = self.items.consts.alloc(Const {
                    name: None,
                    ty: TypeRef::Path(Path::single(Symbol::intern(repr_name))),
                    ty_range: range,
                    loc: ItemLoc {
                        module: ctx.module,
                        file: ctx.file,
                        vis: Visibility::Public,
                    },
                    owner: None,
                    node: v.syntax().id(),
                    has_body: true,
                    def_range: range,
                    item_range: range,
                    discriminant: Some((strukt, variants.len() as u32)),
                    is_static: None,
                });
                discriminant = Discriminant::Const { konst, offset: 0 };
            }
            next = match discriminant {
                Discriminant::Value(value) => value
                    .checked_add(1)
                    .filter(|n| values.contains(n))
                    .map(Discriminant::Value),
                // Overflow is found once the constant's value is known.
                Discriminant::Const { konst, offset } => Some(Discriminant::Const {
                    konst,
                    offset: offset + 1,
                }),
            };
            variants.push(Variant {
                name: v_name,
                kind,
                fields: fields.into(),
                discriminant,
                name_range,
                def_range: v_range,
            });
        }
        if written && has_fields && repr.is_none() {
            self.items.diagnostics.push(Diagnostic::new(
                def_range,
                "`#[repr(inttype)]` must be specified for enums with explicit discriminants and non-unit variants",
            ));
        }
        let name_range = name.syntax().text_range();
        let id = self.items.structs.alloc(Struct {
            name: Symbol::intern(name.text()),
            generics,
            kind: StructKind::Record,
            fields: Box::default(),
            variants: Some(variants.into()),
            repr,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range,
        });
        if let Some(lang) = lang {
            self.items.lang_enums.insert(lang, id);
        }
        for (derive, range) in derives {
            self.derives.push((ctx, id, derive, range, e.syntax().id()));
        }
    }

    fn expand_derives(&mut self) {
        let derives = std::mem::take(&mut self.derives);
        let copy: rustc_hash::FxHashSet<StructId> = derives
            .iter()
            .filter(|d| d.2 == Derive::Copy)
            .map(|d| d.1)
            .collect();
        for &(ctx, strukt, derive, range, node) in &derives {
            let copies = derive == Derive::Clone
                && self.items.structs[strukt].generics.params.is_empty()
                && copy.contains(&strukt);
            self.derive(ctx, strukt, derive, copies, range, node);
        }
    }

    /// The `impl` block `#[derive(Tr)]` (written at `range`) makes for
    /// struct `strukt`, whose node is `node`: rustc's `impl<T: Clone>
    /// Clone for S<T>`, with a method whose body is made from the fields.
    fn derive(
        &mut self,
        ctx: Ctx,
        strukt: StructId,
        derive: Derive,
        copies: bool,
        range: TextRange,
        node: NodeId,
    ) {
        let s = &self.items.structs[strukt];
        let (module, name) = match derive {
            Derive::Clone => ("clone", "Clone"),
            Derive::Copy => ("marker", "Copy"),
            Derive::PartialEq => ("cmp", "PartialEq"),
            Derive::Eq => ("cmp", "Eq"),
            Derive::PartialOrd => ("cmp", "PartialOrd"),
            Derive::Ord => ("cmp", "Ord"),
        };
        let in_core = self.items.is_core(ctx.module);
        let trait_path = core_path(module, name, in_core);
        let bound = Bound {
            path: trait_path,
            range,
        };
        let params: Box<[GenericParam]> = s
            .generics
            .params
            .iter()
            .map(|p| GenericParam {
                name: p.name,
                range: p.range,
                default: None,
            })
            .collect();
        // As rustc expands it: each parameter's own bounds and the
        // trait's, then the struct's `where` clause.
        let inline = |p: &WherePredicate| p.ty_range.end() <= s.generics.range.end();
        let mut predicates = Vec::new();
        for param in params.iter() {
            let ty = TypeRef::Path(Path::single(param.name));
            predicates.extend(
                s.generics
                    .predicates
                    .iter()
                    .filter(|p| inline(p) && p.ty == ty)
                    .cloned(),
            );
            predicates.push(WherePredicate {
                ty,
                ty_range: param.range,
                bound: bound.clone(),
            });
        }
        predicates.extend(s.generics.predicates.iter().filter(|p| !inline(p)).cloned());
        // And the trait for each field type's associated types of the
        // parameters (`T::Out`), which a bound on `T` cannot give.
        let mut projections = Vec::new();
        let variant_fields = s.variants.iter().flatten().flat_map(|v| v.fields.iter());
        for field in s.fields.iter().chain(variant_fields) {
            param_projections(&field.ty, &params, &mut projections);
        }
        for ty in projections {
            predicates.push(WherePredicate {
                ty,
                ty_range: range,
                bound: bound.clone(),
            });
        }
        let mut self_path = Path::single(s.name);
        self_path.ranges = Box::new([s.name_range]);
        if !params.is_empty() || !s.generics.lifetimes.is_empty() {
            self_path.args = Box::new([Some(GenericArgs {
                lifetimes: s
                    .generics
                    .lifetimes
                    .iter()
                    .map(|l| LifetimeRef {
                        name: l.name,
                        range: l.range,
                    })
                    .collect(),
                types: params
                    .iter()
                    .map(|p| TypeRef::Path(Path::single(p.name)))
                    .collect(),
                ranges: params.iter().map(|p| p.range).collect(),
                bindings: Box::default(),
                range: s.generics.range,
                parenthesized: false,
            })]);
        }
        let generics = Generics {
            params,
            lifetimes: s.generics.lifetimes.clone(),
            predicates: predicates.into(),
            outlives: s.generics.outlives.clone(),
            type_outlives: s.generics.type_outlives.clone(),
            range: s.generics.range,
        };
        let name_range = s.name_range;
        let id = self.items.impls.alloc(Impl {
            generics,
            trait_ref: Some(bound),
            trait_: None,
            is_unsafe: false,
            self_ty: TypeRef::Path(self_path),
            self_range: name_range,
            def_range: range,
            module: ctx.module,
            file: ctx.file,
            target: None,
            functions: Vec::new(),
            consts: Vec::new(),
            types: Vec::new(),
        });
        let self_ty = || TypeRef::Path(Path::single(kw::SelfUpper));
        let self_ref = || Param {
            ty: TypeRef::Ref {
                lifetime: None,
                amp: range,
                mutable: false,
                pointee: Box::new(self_ty()),
            },
            name: None,
            range,
            whole: range,
        };
        let (method, params, ret) = match derive {
            Derive::Copy => return,
            Derive::Clone => ("clone", vec![self_ref()], self_ty()),
            Derive::PartialEq => (
                "eq",
                vec![self_ref(), self_ref()],
                TypeRef::Path(Path::single(Symbol::intern("bool"))),
            ),
            Derive::Eq => (
                "assert_receiver_is_total_eq",
                vec![self_ref()],
                TypeRef::unit(),
            ),
            Derive::PartialOrd => {
                // `Option<Ordering>`
                let mut option = core_path("option", "Option", in_core);
                option.args = Box::new([
                    None,
                    None,
                    Some(GenericArgs {
                        lifetimes: Box::default(),
                        types: Box::new([TypeRef::Path(core_path("cmp", "Ordering", in_core))]),
                        ranges: Box::new([range]),
                        bindings: Box::default(),
                        range,
                        parenthesized: false,
                    }),
                ]);
                (
                    "partial_cmp",
                    vec![self_ref(), self_ref()],
                    TypeRef::Path(option),
                )
            }
            Derive::Ord => (
                "cmp",
                vec![self_ref(), self_ref()],
                TypeRef::Path(core_path("cmp", "Ordering", in_core)),
            ),
        };
        let function = self.items.functions.alloc(Function {
            name: Symbol::intern(method),
            generics: Generics::default(),
            params: params.into(),
            ret,
            ret_range: range,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis: Visibility::Public,
            },
            is_unsafe: false,
            is_extern: false,
            is_const: false,
            inherits_overflow_checks: false,
            const_unstable: false,
            precise_live_drops: false,
            intrinsic: false,
            is_variadic: false,
            owner: Some(Owner::Impl(id)),
            has_self: true,
            has_body: true,
            node,
            name_range: range,
            def_range: range,
            sig_range: range,
            item_range: range,
            param_list: range,
            derived: Some((strukt, if copies { Derive::Copy } else { derive })),
        });
        self.items.impls[id].functions.push(function);
    }

    fn collect_extern_block(&mut self, ctx: Ctx, block: ast::ExternBlock<'_>) {
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(block.attrs(), diagnostics);
        let abi = block.abi();
        if let Some(abi) = abi
            && abi.string_token().is_some_and(|s| s.text() != "\"C\"")
        {
            unsupported(
                diagnostics,
                abi.syntax().text_range(),
                "ABIs other than \"C\"",
            );
        }
        if block.unsafe_token().is_none() {
            let range = abi.map_or(block.syntax().text_range(), |a| a.syntax().text_range());
            diagnostics.push(Diagnostic::new(
                range,
                "`extern` blocks must be `unsafe` in edition 2024",
            ));
        }
        let Some(list) = block.extern_item_list() else {
            return;
        };
        for item in list.items() {
            match item {
                ast::Item::Fn(f) => {
                    self.collect_fn(ctx, f, true, None);
                }
                ast::Item::Static(_) => unsupported(
                    &mut self.items.diagnostics,
                    item.syntax().text_range(),
                    "statics",
                ),
                _ => self.items.diagnostics.push(Diagnostic::new(
                    item.syntax().text_range(),
                    "only functions and statics can be declared in `extern` blocks",
                )),
            }
        }
    }
}

/// Whether a node of `kind` is an item.
fn is_item_kind(kind: SyntaxKind) -> bool {
    use SyntaxKind as K;
    matches!(
        kind,
        K::FN
            | K::STRUCT
            | K::ENUM
            | K::TRAIT
            | K::IMPL
            | K::CONST
            | K::STATIC
            | K::TYPE_ALIAS
            | K::MODULE
            | K::USE
            | K::EXTERN_BLOCK
            | K::EXTERN_CRATE
            | K::MACRO_CALL
    )
}

/// The outermost blocks in `node`, itself included, outside items.
fn outer_blocks(node: SyntaxNode<'_>, out: &mut Vec<NodeId>) {
    if node.kind() == SyntaxKind::BLOCK_EXPR {
        out.push(node.id());
        return;
    }
    if is_item_kind(node.kind()) {
        return;
    }
    for child in node.children() {
        outer_blocks(child, out);
    }
}

pub(crate) fn unsupported(diagnostics: &mut Vec<Diagnostic>, range: TextRange, what: &str) {
    diagnostics.push(Diagnostic::new(
        range,
        format!("{what} are not supported yet"),
    ));
}

/// Where an item starts, after its attributes: rustc's spans for items
/// leave them out.
pub(crate) fn def_start(node: SyntaxNode<'_>) -> u32 {
    node.children_with_tokens()
        .find(|e| !e.kind().is_trivia() && e.kind() != SyntaxKind::ATTR)
        .map_or(node.text_range(), SyntaxElement::text_range)
        .start()
}

/// Attributes that cannot change what a program means are allowed and
/// ignored; any other attribute is reported, so that `#[cfg]` and friends
/// are never silently dropped.
pub(crate) fn check_attrs(
    attrs: AstChildren<'_, ast::Attr<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for attr in attrs {
        check_attr(attr, diagnostics);
    }
}

/// A file's or module's own attributes: like [`check_attrs`], and in
/// `core` also the crate attributes rustc needs to compile it, which mean
/// nothing to flux.
fn check_inner_attrs(
    attrs: AstChildren<'_, ast::Attr<'_>>,
    is_core: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for attr in attrs {
        if !(is_core && attr_name(attr).is_some_and(|n| ["no_core", "feature"].contains(&n))) {
            check_attr(attr, diagnostics);
        }
    }
}

fn attr_name(attr: ast::Attr<'_>) -> Option<&str> {
    attr.meta()
        .and_then(|m| m.path())
        .filter(|p| p.qualifier().is_none())
        .and_then(|p| p.segment())
        .and_then(|s| s.name_ref())
        .map(|n| n.text())
}

/// [`check_attrs`] for fields and parameters, where rustc rejects
/// `#[derive]` as an attribute macro rather than a misplaced derive.
fn check_inert_attrs(attrs: AstChildren<'_, ast::Attr<'_>>, diagnostics: &mut Vec<Diagnostic>) {
    for attr in attrs {
        let path = attr.meta().and_then(|m| m.path());
        match path {
            Some(path) if attr_name(attr) == Some("derive") => diagnostics.push(Diagnostic::new(
                path.syntax().text_range(),
                "expected non-macro attribute, found attribute macro `derive`",
            )),
            _ => check_attr(attr, diagnostics),
        }
    }
}

pub(crate) fn check_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    const IGNORED: &[&str] = &[
        "allow", "warn", "deny", "expect", "inline", "cold", "must_use", "doc",
    ];
    let name = attr_name(attr);
    if name.is_some_and(|n| IGNORED.contains(&n)) {
        return;
    }
    if name == Some("derive") {
        diagnostics.push(Diagnostic::new(
            attr.syntax().text_range(),
            "`derive` may only be applied to `struct`s, `enum`s and `union`s",
        ));
        return;
    }
    let message = match name {
        Some(name) => format!("the `{name}` attribute is not supported yet"),
        None => "attributes like this are not supported yet".to_owned(),
    };
    diagnostics.push(Diagnostic::new(attr.syntax().text_range(), message));
}

/// A path as written, without generic arguments; `None` (reported) if it
/// has any or is malformed. For `use` declarations and visibilities.
pub(crate) fn lower_path(path: ast::Path<'_>, diagnostics: &mut Vec<Diagnostic>) -> Option<Path> {
    let lowered = lower_generic_path(path, diagnostics)?;
    if lowered.qself.is_some() {
        unsupported(
            diagnostics,
            path.syntax().text_range(),
            "qualified paths here",
        );
        return None;
    }
    if !lowered.args.is_empty() {
        unsupported(diagnostics, path.syntax().text_range(), "generic arguments");
        return None;
    }
    Some(lowered)
}

/// A path as written, with any generic arguments; `None` (reported) if
/// it is malformed.
pub(crate) fn lower_generic_path(
    path: ast::Path<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Path> {
    lower_path_with(path, false, diagnostics)
}

/// A path to a trait, as [`lower_generic_path`], where the last segment
/// may have `Fn`-family arguments: `Fn(i32) -> i32`.
fn lower_trait_path(path: ast::Path<'_>, diagnostics: &mut Vec<Diagnostic>) -> Option<Path> {
    lower_path_with(path, true, diagnostics)
}

fn lower_path_with(
    path: ast::Path<'_>,
    parenthesized: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Path> {
    // Large crates lower millions of paths: each array is allocated once,
    // at its final size.
    let mut count = 1;
    let mut current = path.qualifier();
    while let Some(p) = current {
        count += 1;
        current = p.qualifier();
    }
    let mut segments = Vec::with_capacity(count);
    let mut current = Some(path);
    // `Fn(A) -> B` on the last segment of a trait's path.
    let mut paren = None;
    while let Some(p) = current {
        let segment = p.segment()?;
        if let Some(list) = segment.parenthesized_arg_list() {
            if !(parenthesized && segments.is_empty()) {
                unsupported(
                    diagnostics,
                    list.syntax().text_range(),
                    "parenthesized generic arguments",
                );
                return None;
            }
            paren = Some((list, segment.ret_type()));
        }
        segments.push(segment);
        current = p.qualifier();
    }
    segments.reverse();
    let mut segments = &segments[..];
    // `<T as Tr>::f`: the qualified segment comes first.
    let mut qself = None;
    if segments[0].name_ref().is_none() && segments[0].l_angle_token().is_some() {
        let segment = segments[0];
        segments = &segments[1..];
        let ty_range = segment
            .self_ty()
            .map_or(segment.syntax().text_range(), |t| t.syntax().text_range());
        let ty = lower_type(segment.self_ty(), diagnostics);
        let trait_ = match segment.trait_ty() {
            Some(t) => Some(lower_generic_path(t.path()?, diagnostics)?),
            None => None,
        };
        qself = Some(Box::new(QSelf {
            ty,
            ty_range,
            trait_,
            range: segment.syntax().text_range(),
        }));
        if segments.is_empty() {
            return None;
        }
    }
    // A leading `::`, not a turbofish's.
    let leading = |segment: &ast::PathSegment<'_>| {
        let colons = segment.colon2_token()?;
        let name = segment.name_ref()?;
        Some(colons.text_range().start() < name.syntax().text_range().start())
    };
    let mut anchor = if qself.is_none() && leading(&segments[0]) == Some(true) {
        PathAnchor::Global
    } else {
        PathAnchor::Plain
    };
    let count = segments.len();
    // `crate`, `self` and `super` come before every name: the names are
    // the segments from the first one on.
    let mut anchors = 0;
    let (mut names, mut ranges, mut args) = (Vec::new(), Vec::new(), Vec::new());
    for (i, segment) in segments.iter().enumerate() {
        let name = segment.name_ref()?;
        let kind = name.token().map(|t| t.kind());
        let range = name.syntax().text_range();
        let list = segment.generic_arg_list();
        if let Some(list) = list
            && (kind == Some(T![crate])
                || kind == Some(T![super])
                || (kind == Some(T![self]) && count > 1))
        {
            unsupported(
                diagnostics,
                list.syntax().text_range(),
                "generic arguments here",
            );
            return None;
        }
        let misplaced = |what: &str, diagnostics: &mut Vec<Diagnostic>| {
            let message = format!("`{}` in paths can only be used in {what}", name.text());
            diagnostics.push(Diagnostic::new(range, message));
        };
        if qself.is_some() && matches!(kind, Some(T![crate] | T![super] | T![self])) {
            misplaced("start position", diagnostics);
            return None;
        }
        if kind == Some(T![crate]) {
            if i != 0 || anchor != PathAnchor::Plain {
                misplaced("start position", diagnostics);
                return None;
            }
            anchor = PathAnchor::Crate;
            anchors += 1;
        } else if kind == Some(T![super]) {
            anchor = match anchor {
                _ if !names.is_empty() => {
                    misplaced("start position", diagnostics);
                    return None;
                }
                PathAnchor::Plain | PathAnchor::SelfModule => PathAnchor::Super(1),
                PathAnchor::Super(n) => PathAnchor::Super(n + 1),
                PathAnchor::Crate | PathAnchor::Global => {
                    misplaced("start position", diagnostics);
                    return None;
                }
            };
            anchors += 1;
        } else if kind == Some(T![self]) && i == 0 && count > 1 {
            anchor = PathAnchor::SelfModule;
            anchors += 1;
        } else if kind == Some(T![self]) && i != 0 && i + 1 != count {
            misplaced("start position or last position", diagnostics);
            return None;
        } else {
            if names.is_empty() {
                names.reserve_exact(count - i);
                ranges.reserve_exact(count - i);
            }
            names.push(Symbol::intern(name.text()));
            ranges.push(range);
            // Most paths have no arguments, and get no list of them.
            let lowered = match list {
                Some(list) => Some(lower_generic_args(list, diagnostics)),
                None if i + 1 == count => paren
                    .take()
                    .map(|(list, ret)| lower_parenthesized_args(list, ret, diagnostics)),
                None => None,
            };
            if let Some(lowered) = lowered {
                if args.is_empty() {
                    args.reserve_exact(count - anchors);
                }
                args.resize(names.len() - 1, None);
                args.push(Some(lowered));
            }
        }
    }
    if !args.is_empty() {
        args.resize(names.len(), None);
    }
    let anchor_ranges = segments[..anchors]
        .iter()
        .map(|s| {
            s.name_ref()
                .map_or(TextRange::default(), |n| n.syntax().text_range())
        })
        .collect();
    Some(Path {
        anchor,
        qself,
        segments: names.into(),
        ranges: ranges.into(),
        anchor_ranges,
        args: args.into(),
    })
}

/// `<'a, A, B>`: lifetime and type arguments, and associated types'.
pub(crate) fn lower_generic_args(
    list: ast::GenericArgList<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> GenericArgs {
    let mut lifetimes = Vec::new();
    let mut types = Vec::new();
    let mut ranges = Vec::new();
    let mut bindings = Vec::new();
    let mut misordered = false;
    for arg in list.args() {
        let range = arg.syntax().text_range();
        let what = match arg {
            ast::GenericArg::LifetimeArg(arg) => {
                let Some(lifetime) = arg.lifetime() else {
                    continue;
                };
                // Lifetimes come first: the type before it is where one
                // was expected (or the item has none, E0107).
                if let Some(&first) = ranges.first() {
                    if !misordered {
                        misordered = true;
                        diagnostics.push(Diagnostic::new(
                            first,
                            "type provided when a lifetime was expected",
                        ));
                    }
                    continue;
                }
                lifetimes.push(lower_lifetime(lifetime));
                continue;
            }
            ast::GenericArg::TypeArg(arg) => {
                types.push(lower_type(arg.ty(), diagnostics));
                ranges.push(range);
                continue;
            }
            ast::GenericArg::AssocTypeArg(arg) if arg.eq_token().is_some() => {
                let Some(name) = arg.name_ref() else {
                    continue;
                };
                if let Some(list) = arg.generic_arg_list() {
                    unsupported(
                        diagnostics,
                        list.syntax().text_range(),
                        "generic associated types",
                    );
                    continue;
                }
                bindings.push(AssocBinding {
                    name: Symbol::intern(name.text()),
                    ty: lower_type(arg.ty(), diagnostics),
                    name_range: name.syntax().text_range(),
                    ty_range: arg.ty().map_or(range, |t| t.syntax().text_range()),
                });
                continue;
            }
            ast::GenericArg::ConstArg(_) => "const arguments",
            ast::GenericArg::AssocTypeArg(_) => "associated type bounds",
        };
        unsupported(diagnostics, range, what);
        types.push(TypeRef::Error);
        ranges.push(range);
    }
    GenericArgs {
        lifetimes: lifetimes.into(),
        types: types.into(),
        ranges: ranges.into(),
        bindings: bindings.into(),
        range: list.syntax().text_range(),
        parenthesized: false,
    }
}

/// `(A, B) -> C` after a trait's name, as `Fn`-family traits take
/// them: the tuple `(A, B)`, and `Output = C` (`()` if no type is
/// written).
fn lower_parenthesized_args(
    list: ast::ParenthesizedArgList<'_>,
    ret: Option<ast::RetType<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> GenericArgs {
    let range = list.syntax().text_range();
    let params: Box<[TypeRef]> = list
        .types()
        .map(|ty| lower_type(Some(ty), diagnostics))
        .collect();
    let (ty, ty_range) = match ret {
        Some(ret) => {
            let ty = ret.ty();
            let at = ty.map_or(ret.syntax().text_range(), |t| t.syntax().text_range());
            (lower_type(ty, diagnostics), at)
        }
        None => (TypeRef::unit(), range),
    };
    let end = ret.map_or(range, |r| r.syntax().text_range());
    GenericArgs {
        lifetimes: Box::default(),
        types: Box::new([TypeRef::Tuple(params)]),
        ranges: Box::new([range]),
        bindings: Box::new([AssocBinding {
            name: Symbol::intern("Output"),
            ty,
            name_range: ty_range,
            ty_range,
        }]),
        range: TextRange::new(range.start(), end.end()),
        parenthesized: true,
    }
}

/// The associated types of `params` that `ty` names (`T::Out`,
/// `<T as Tr>::Out`), each once, into `out`.
fn param_projections(ty: &TypeRef, params: &[GenericParam], out: &mut Vec<TypeRef>) {
    let is_param = |p: &Path| {
        p.qself.is_none() && p.segments.len() == 1 && params.iter().any(|g| g.name == p.segments[0])
    };
    match ty {
        TypeRef::Path(path) => {
            let of_param = match &path.qself {
                Some(qself) => matches!(&qself.ty, TypeRef::Path(p) if is_param(p)),
                None => {
                    path.segments.len() > 1 && params.iter().any(|g| g.name == path.segments[0])
                }
            };
            if of_param {
                if !out.contains(ty) {
                    out.push(ty.clone());
                }
                return;
            }
            for args in path.args.iter().flatten() {
                for arg in args.types.iter() {
                    param_projections(arg, params, out);
                }
            }
        }
        TypeRef::Tuple(elements) => {
            for e in elements.iter() {
                param_projections(e, params, out);
            }
        }
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Ref { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => {
            param_projections(pointee, params, out);
        }
        TypeRef::FnPtr {
            params: inputs,
            ret,
            ..
        } => {
            for p in inputs.iter().chain([&**ret]) {
                param_projections(p, params, out);
            }
        }
        TypeRef::Never | TypeRef::Infer | TypeRef::Error => {}
    }
}

/// The value of an enum discriminant written as an integer literal,
/// optionally negated, without a suffix or with `isize`'s, the type rustc
/// gives discriminants by default.
/// A discriminant written as an integer literal, `5` or `-5`: whether it
/// is negated, its value, and its suffix and the literal's range if it
/// has one.
#[allow(clippy::type_complexity)]
fn discriminant_value(expr: ast::Expr<'_>) -> Option<(bool, u128, Option<(Symbol, TextRange)>)> {
    let (negative, literal) = match expr {
        ast::Expr::Literal(literal) => (false, literal),
        ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Neg) => {
            match prefix.expr()? {
                ast::Expr::Literal(literal) => (true, literal),
                _ => return None,
            }
        }
        _ => return None,
    };
    let token = literal.value()?;
    let crate::Literal::Int { value, suffix } = crate::literal::parse_int(token.text()).ok()?
    else {
        return None;
    };
    let suffix = suffix.map(|s| (s, literal.syntax().text_range()));
    Some((negative, value, suffix))
}

/// Reports `Self` and the enum's type parameters in a discriminant of a
/// generic enum, which rustc does not allow; whether there are any.
fn generic_in_discriminant(
    expr: ast::Expr<'_>,
    generics: &[Symbol],
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if generics.is_empty() {
        return false;
    }
    let mut found = false;
    for token in expr.syntax().tokens() {
        let message = match token.kind() {
            SyntaxKind::SELF_TYPE_KW => {
                "generic `Self` types are not permitted in enum discriminant values"
            }
            SyntaxKind::IDENT if generics.iter().any(|g| g.as_str() == token.text()) => {
                "generic parameters may not be used in enum discriminant values"
            }
            _ => continue,
        };
        diagnostics.push(Diagnostic::new(token.text_range(), message));
        found = true;
    }
    found
}

/// The values of integer type `name`, as far as an `i128` holds them;
/// `isize` and `usize` are 64 bits wide.
fn int_range(name: &str) -> std::ops::RangeInclusive<i128> {
    let (signed, bits) = match name {
        "isize" => (true, 64),
        "usize" => (false, 64),
        _ => (name.starts_with('i'), name[1..].parse().unwrap_or(64)),
    };
    match (signed, bits) {
        (true, 128) => i128::MIN..=i128::MAX,
        (false, 128) => 0..=i128::MAX,
        (true, bits) => -(1i128 << (bits - 1))..=(1i128 << (bits - 1)) - 1,
        (false, bits) => 0..=(1i128 << bits) - 1,
    }
}

/// The integer types a `repr` may give a discriminant.
const REPR_INTS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];

/// The hints of a `#[repr(...)]`, with their ranges; malformed ones are
/// reported as rustc does.
fn repr_attr(
    attr: ast::Attr<'_>,
    out: &mut Vec<(Symbol, TextRange)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let meta = attr.meta();
    let malformed = |diagnostics: &mut Vec<Diagnostic>| {
        let range = meta.map_or(attr.syntax().text_range(), |m| m.syntax().text_range());
        diagnostics.push(Diagnostic::new(range, "malformed `repr` attribute input"));
    };
    let Some(tree) = meta.and_then(|m| m.token_tree()) else {
        malformed(diagnostics);
        return;
    };
    let mut items: Vec<Vec<SyntaxElement<'_>>> = vec![Vec::new()];
    for element in tree.syntax().children_with_tokens() {
        match element.kind() {
            k if k.is_trivia() => {}
            SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => {}
            SyntaxKind::COMMA => items.push(Vec::new()),
            _ => items.last_mut().expect("an item").push(element),
        }
    }
    let mut hints = Vec::new();
    for item in items.iter().filter(|i| !i.is_empty()) {
        let name = match item.as_slice() {
            [name] | [name, _] => name.into_token().filter(|t| t.kind() == SyntaxKind::IDENT),
            _ => None,
        };
        let known = name.is_some_and(|n| {
            let args = item.len() == 2;
            match n.text() {
                "align" => args && item[1].kind() == SyntaxKind::TOKEN_TREE,
                "C" | "Rust" | "transparent" | "packed" | "simd" => {
                    !args || (n.text() == "packed" && item[1].kind() == SyntaxKind::TOKEN_TREE)
                }
                other => !args && REPR_INTS.contains(&other),
            }
        });
        match name {
            Some(name) if known => hints.push((Symbol::intern(name.text()), name.text_range())),
            _ => {
                malformed(diagnostics);
                return;
            }
        }
    }
    out.extend(hints);
}

/// The integer type `hints`, an enum's `repr`s, give its discriminant;
/// hints flux does not support, and conflicting ones, are reported.
/// `has_fields`: a variant has fields, which `repr(C, u8)` describes;
/// `empty`: the enum has no variants, which no `repr` describes.
fn enum_repr(
    hints: &[(Symbol, TextRange)],
    has_fields: bool,
    empty: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Symbol> {
    let mut int = None;
    let mut c = false;
    for &(hint, range) in hints {
        match hint.as_str() {
            "C" => c = true,
            "Rust" => {}
            name if REPR_INTS.contains(&name) => {
                if int.is_some() {
                    diagnostics.push(Diagnostic::new(
                        hints[0].1,
                        "conflicting representation hints",
                    ));
                    return int;
                }
                int = Some(hint);
            }
            name => diagnostics.push(Diagnostic::new(
                range,
                format!("the `repr({name})` attribute is not supported yet"),
            )),
        }
    }
    if c && int.is_some() && !has_fields {
        diagnostics.push(Diagnostic::new(
            hints[0].1,
            "conflicting representation hints",
        ));
    }
    if let Some(&(_, range)) = hints.iter().find(|h| h.0.as_str() != "Rust")
        && empty
        && (c || int.is_some())
    {
        diagnostics.push(Diagnostic::new(
            range,
            "unsupported representation for zero-variant enum",
        ));
    }
    int
}

/// A trait `#[derive]` implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derive {
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
}

/// `::core::module::name`, as `#[derive]` names its traits and their
/// types.
fn core_path(module: &str, name: &str, in_core: bool) -> Path {
    // `core` itself names them from its root.
    let (anchor, segments): (PathAnchor, Box<[Symbol]>) = if in_core {
        (
            PathAnchor::Crate,
            Box::new([Symbol::intern(module), Symbol::intern(name)]),
        )
    } else {
        (
            PathAnchor::Global,
            Box::new([
                Symbol::intern("core"),
                Symbol::intern(module),
                Symbol::intern(name),
            ]),
        )
    };
    Path {
        anchor,
        qself: None,
        segments,
        ranges: Box::default(),
        anchor_ranges: Box::default(),
        args: Box::default(),
    }
}

/// The traits `#[derive(...)]` names, with their ranges; others, and
/// malformed lists, are reported as rustc does.
fn derive_attr(
    attr: ast::Attr<'_>,
    out: &mut Vec<(Derive, TextRange)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let meta = attr.meta();
    let Some(tree) = meta.and_then(|m| m.token_tree()) else {
        diagnostics.push(Diagnostic::new(
            attr.syntax().text_range(),
            "malformed `derive` attribute input",
        ));
        return;
    };
    if tree.l_paren_token().is_none() {
        diagnostics.push(Diagnostic::new(
            tree.syntax().text_range(),
            "wrong meta list delimiters",
        ));
        return;
    }
    let mut error = |range: TextRange, message: String| {
        diagnostics.push(Diagnostic::new(range, message));
    };
    // Items between commas: a trait's name, or a path to one.
    let mut item: Vec<SyntaxElement<'_>> = Vec::new();
    let mut items = Vec::new();
    for element in tree.syntax().children_with_tokens() {
        match element.kind() {
            k if k.is_trivia() => {}
            SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => {}
            SyntaxKind::COMMA => {
                if item.is_empty() {
                    error(
                        element.text_range(),
                        "expected identifier, found `,`".into(),
                    );
                }
                items.push(std::mem::take(&mut item));
            }
            _ => item.push(element),
        }
    }
    items.push(item);
    for item in items {
        let Some(&first) = item.first() else {
            continue;
        };
        if first.kind().is_literal() {
            error(
                first.text_range(),
                "expected path to a trait, found literal".into(),
            );
            continue;
        }
        // `Tr`, `a::Tr` or `::a::Tr`; in a token tree, `::` is two `:`s.
        let mut path_len = 0;
        let mut want_ident = true;
        let mut single = true;
        while let Some(&e) = item.get(path_len) {
            let sep = e.kind() == SyntaxKind::COLON
                && item
                    .get(path_len + 1)
                    .is_some_and(|n| n.kind() == SyntaxKind::COLON);
            if sep && (path_len == 0 || !want_ident) {
                path_len += 2;
                want_ident = true;
                single = false;
            } else if want_ident && e.kind() == SyntaxKind::IDENT {
                path_len += 1;
                want_ident = false;
            } else {
                break;
            }
        }
        if let Some(&next) = item.get(path_len) {
            if next.kind() == SyntaxKind::TOKEN_TREE {
                error(
                    next.text_range(),
                    "traits in `#[derive(...)]` don't accept arguments".into(),
                );
            } else {
                let text = next.into_token().map_or("", |t| t.text());
                error(
                    next.text_range(),
                    format!("expected one of `(`, `,`, `::`, or `=`, found `{text}`"),
                );
            }
            continue;
        }
        let range = TextRange::new(
            first.text_range().start(),
            item[path_len - 1].text_range().end(),
        );
        let name = match item.as_slice() {
            [only] if single => only.into_token().map_or("", |t| t.text()),
            _ => {
                error(range, "paths in `derive` are not supported yet".into());
                continue;
            }
        };
        let derive = match name {
            "Clone" => Derive::Clone,
            "Copy" => Derive::Copy,
            "PartialEq" => Derive::PartialEq,
            "Eq" => Derive::Eq,
            "PartialOrd" => Derive::PartialOrd,
            "Ord" => Derive::Ord,
            "Debug" | "Default" | "Hash" => {
                error(range, format!("deriving `{name}` is not supported yet"));
                continue;
            }
            _ => {
                error(
                    range,
                    format!("cannot find derive macro `{name}` in this scope"),
                );
                continue;
            }
        };
        out.push((derive, range));
    }
}

/// `#[diagnostic::on_unimplemented(...)]`'s `message`, if it has one;
/// `None` for any other attribute. Its other options only label spans,
/// which flux does not show.
fn on_unimplemented_attr(attr: ast::Attr<'_>) -> Option<Option<Box<str>>> {
    let path = attr.meta()?.path()?;
    let qualifier = path.qualifier()?;
    fn name<'t>(p: ast::Path<'t>) -> Option<&'t str> {
        p.segment().and_then(|s| s.name_ref()).map(|n| n.text())
    }
    if qualifier.qualifier().is_some()
        || name(qualifier) != Some("diagnostic")
        || name(path) != Some("on_unimplemented")
    {
        return None;
    }
    let text = attr.meta()?.syntax().text();
    let Some(at) = text.find("message") else {
        return Some(None);
    };
    let rest = text[at + "message".len()..]
        .trim_start()
        .strip_prefix('=')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let mut message = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(Some(message.into())),
            '\\' => message.extend(chars.next()),
            c => message.push(c),
        }
    }
    Some(None)
}

/// `#[lang = "name"]`'s name.
fn lang_attr(attr: ast::Attr<'_>) -> Option<Symbol> {
    if attr_name(attr) != Some("lang") {
        return None;
    }
    let text = attr.meta()?.syntax().text();
    let value = text.split('"').nth(1)?;
    Some(Symbol::intern(value))
}

/// The trait of `impl Tr for S`, which must be a path.
fn lower_bound_type(ty: ast::Type<'_>, diagnostics: &mut Vec<Diagnostic>) -> Option<Bound> {
    let range = ty.syntax().text_range();
    let ast::Type::PathType(path) = ty else {
        diagnostics.push(Diagnostic::new(range, "expected a trait, found type"));
        return None;
    };
    let path = lower_trait_path(path.path()?, diagnostics)?;
    Some(Bound { path, range })
}

/// A lifetime as written.
pub(crate) fn lower_lifetime(lifetime: ast::Lifetime<'_>) -> LifetimeRef {
    LifetimeRef {
        name: Symbol::intern(lifetime.text()),
        range: lifetime.syntax().text_range(),
    }
}

/// The bounds in `list`, `A + B + 'a`: trait bounds into `out`, and
/// lifetime bounds into `lifetimes` where they are allowed (`None`
/// reports them as not supported); others are reported.
fn lower_bounds(
    list: ast::TypeBoundList<'_>,
    out: &mut Vec<Bound>,
    mut lifetimes: Option<&mut Vec<LifetimeRef>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for bound in list.bounds() {
        let range = bound.syntax().text_range();
        let what = if let Some(lifetime) = bound.lifetime() {
            match lifetimes.as_deref_mut() {
                Some(lifetimes) => {
                    lifetimes.push(lower_lifetime(lifetime));
                    continue;
                }
                None => "lifetime bounds",
            }
        } else if bound.use_token().is_some() {
            "`use` bounds"
        } else if bound.question_token().is_some() {
            "`?Sized` bounds"
        } else if bound.for_token().is_some() {
            "higher-ranked bounds"
        } else if bound.l_paren_token().is_some() {
            "parenthesized bounds"
        } else {
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            if let Some(path) = lower_trait_path(path, diagnostics) {
                out.push(Bound { path, range });
            }
            continue;
        };
        unsupported(diagnostics, range, what);
    }
}

/// An item's generic parameter list and `where` clause: lifetime and
/// type parameters, type parameters without defaults, with distinct
/// names, trait bounds on types and lifetime bounds.
fn collect_generics(
    list: Option<ast::GenericParamList<'_>>,
    where_clause: Option<ast::WhereClause<'_>>,
    defaults: Defaults,
    diagnostics: &mut Vec<Diagnostic>,
) -> Generics {
    let mut predicates = Vec::new();
    let mut outlives = Vec::new();
    let mut type_outlives = Vec::new();
    let where_predicates = |predicates: &mut Vec<WherePredicate>,
                            outlives: &mut Vec<OutlivesPredicate>,
                            type_outlives: &mut Vec<TypeOutlives>,
                            diagnostics: &mut Vec<Diagnostic>| {
        let Some(clause) = where_clause else {
            return;
        };
        for pred in clause.predicates() {
            let range = pred.syntax().text_range();
            if pred.for_token().is_some() {
                unsupported(diagnostics, range, "higher-ranked bounds");
                continue;
            }
            let mut bounds = Vec::new();
            let mut lifetimes = Vec::new();
            if let Some(lifetime) = pred.lifetime() {
                // `'a: 'b + 'c`
                let longer = lower_lifetime(lifetime);
                if let Some(list) = pred.type_bound_list() {
                    lower_bounds(list, &mut bounds, Some(&mut lifetimes), diagnostics);
                }
                for bound in bounds {
                    diagnostics.push(Diagnostic::new(
                        bound.range,
                        "expected a lifetime, found a trait",
                    ));
                }
                outlives.extend(
                    lifetimes
                        .into_iter()
                        .map(|shorter| OutlivesPredicate { longer, shorter }),
                );
                continue;
            }
            let ty_range = pred.ty().map_or(range, |t| t.syntax().text_range());
            let ty = lower_type(pred.ty(), diagnostics);
            if let Some(list) = pred.type_bound_list() {
                lower_bounds(list, &mut bounds, Some(&mut lifetimes), diagnostics);
            }
            predicates.extend(bounds.into_iter().map(|bound| WherePredicate {
                ty: ty.clone(),
                ty_range,
                bound,
            }));
            type_outlives.extend(lifetimes.into_iter().map(|lifetime| TypeOutlives {
                ty: ty.clone(),
                ty_range,
                lifetime,
            }));
        }
    };
    let Some(list) = list else {
        where_predicates(
            &mut predicates,
            &mut outlives,
            &mut type_outlives,
            diagnostics,
        );
        return Generics {
            predicates: predicates.into(),
            outlives: outlives.into(),
            type_outlives: type_outlives.into(),
            ..Generics::default()
        };
    };
    let mut params: Vec<GenericParam> = Vec::new();
    let mut lifetimes: Vec<LifetimeParam> = Vec::new();
    // Parameters with defaults so far, which no parameter without one
    // may follow.
    let mut defaulted: Vec<TextRange> = Vec::new();
    let already_used = |name: Symbol| {
        format!(
            "the name `{name}` is already used for a generic parameter in this item's generic parameters"
        )
    };
    for param in list.generic_params() {
        let range = param.syntax().text_range();
        let what = match param {
            ast::GenericParam::TypeParam(param) => {
                check_attrs(param.attrs(), diagnostics);
                let Some(name) = param.name() else {
                    continue;
                };
                if let Some(list) = param.type_bound_list() {
                    let mut bounds = Vec::new();
                    let mut bound_lifetimes = Vec::new();
                    lower_bounds(list, &mut bounds, Some(&mut bound_lifetimes), diagnostics);
                    let ty = TypeRef::Path(Path::single(Symbol::intern(name.text())));
                    let ty_range = name.syntax().text_range();
                    predicates.extend(bounds.into_iter().map(|bound| WherePredicate {
                        ty: ty.clone(),
                        ty_range,
                        bound,
                    }));
                    type_outlives.extend(bound_lifetimes.into_iter().map(|lifetime| {
                        TypeOutlives {
                            ty: ty.clone(),
                            ty_range,
                            lifetime,
                        }
                    }));
                }
                let name_range = name.syntax().text_range();
                let mut default = None;
                if let Some(ty) = param.default_type() {
                    let ty_range = ty.syntax().text_range();
                    match defaults {
                        Defaults::Trait => {
                            default = Some((lower_type(Some(ty), diagnostics), ty_range));
                        }
                        Defaults::Struct => unsupported(
                            diagnostics,
                            ty_range,
                            "defaults for type parameters of structs",
                        ),
                        Defaults::Alias => unsupported(
                            diagnostics,
                            ty_range,
                            "defaults for type parameters of type aliases",
                        ),
                        Defaults::No => diagnostics.push(Diagnostic::new(
                            range,
                            "defaults for generic parameters are not allowed here",
                        )),
                    }
                    if defaults != Defaults::No {
                        defaulted.push(name_range);
                    }
                } else if defaults != Defaults::No
                    && let Some(&earlier) = defaulted.first()
                {
                    diagnostics.push(Diagnostic::new(
                        earlier,
                        "generic parameters with a default must be trailing",
                    ));
                    defaulted.clear();
                }
                let name = Symbol::intern(name.text());
                if params.iter().any(|p| p.name == name) {
                    diagnostics.push(Diagnostic::new(name_range, already_used(name)));
                    continue;
                }
                params.push(GenericParam {
                    name,
                    range: name_range,
                    default,
                });
                continue;
            }
            ast::GenericParam::LifetimeParam(param) => {
                check_attrs(param.attrs(), diagnostics);
                let Some(lifetime) = param.lifetime() else {
                    continue;
                };
                let lifetime = lower_lifetime(lifetime);
                if !params.is_empty() {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "lifetime parameters must be declared prior to type and const parameters",
                    ));
                }
                if let Some(list) = param.type_bound_list() {
                    let mut bounds = Vec::new();
                    let mut shorter = Vec::new();
                    lower_bounds(list, &mut bounds, Some(&mut shorter), diagnostics);
                    for bound in bounds {
                        diagnostics.push(Diagnostic::new(
                            bound.range,
                            "expected a lifetime, found a trait",
                        ));
                    }
                    outlives.extend(shorter.into_iter().map(|shorter| OutlivesPredicate {
                        longer: lifetime,
                        shorter,
                    }));
                }
                if lifetime.is_static() {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "invalid lifetime parameter name: `'static`",
                    ));
                    continue;
                }
                if lifetime.is_anonymous() {
                    diagnostics.push(Diagnostic::new(lifetime.range, "`'_` cannot be used here"));
                    continue;
                }
                if lifetimes.iter().any(|l| l.name == lifetime.name) {
                    diagnostics.push(Diagnostic::new(lifetime.range, already_used(lifetime.name)));
                    continue;
                }
                lifetimes.push(LifetimeParam {
                    name: lifetime.name,
                    range: lifetime.range,
                });
                continue;
            }
            ast::GenericParam::ConstParam(_) => "const generics",
        };
        unsupported(diagnostics, range, what);
    }
    where_predicates(
        &mut predicates,
        &mut outlives,
        &mut type_outlives,
        diagnostics,
    );
    Generics {
        params: params.into(),
        lifetimes: lifetimes.into(),
        predicates: predicates.into(),
        outlives: outlives.into(),
        type_outlives: type_outlives.into(),
        range: list.syntax().text_range(),
    }
}

/// Lowers a type as written. A missing type was already reported by the
/// parser.
pub(crate) fn lower_type(ty: Option<ast::Type<'_>>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let Some(ty) = ty else {
        return TypeRef::Error;
    };
    let what = match ty {
        ast::Type::PathType(path) => {
            return match path.path().and_then(|p| lower_generic_path(p, diagnostics)) {
                Some(path) => TypeRef::Path(path),
                None => TypeRef::Error,
            };
        }
        ast::Type::TupleType(tuple) => {
            let fields = tuple
                .fields()
                .map(|field| lower_type(Some(field), diagnostics))
                .collect();
            return TypeRef::Tuple(fields);
        }
        ast::Type::ParenType(paren) => return lower_type(paren.ty(), diagnostics),
        ast::Type::NeverType(_) => return TypeRef::Never,
        ast::Type::ArrayType(array) => {
            let elem = lower_type(array.ty(), diagnostics);
            let len = lower_array_len(array.expr(), ty.syntax().text_range(), diagnostics);
            return TypeRef::Array {
                elem: Box::new(elem),
                len,
            };
        }
        ast::Type::DynTraitType(_) => "trait objects",
        ast::Type::FnPtrType(ptr) => return lower_fn_ptr(ptr, diagnostics),
        ast::Type::ForType(_) => "higher-ranked types",
        ast::Type::ImplTraitType(_) => "`impl Trait` types",
        ast::Type::InferType(_) => return TypeRef::Infer,
        ast::Type::MacroType(_) => "macros",
        ast::Type::PtrType(ptr) => {
            let pointee = lower_type(ptr.ty(), diagnostics);
            return TypeRef::Ptr {
                mutable: ptr.mut_token().is_some(),
                pointee: Box::new(pointee),
            };
        }
        ast::Type::RefType(r) => {
            let pointee = lower_type(r.ty(), diagnostics);
            return TypeRef::Ref {
                lifetime: r.lifetime().map(lower_lifetime),
                amp: r
                    .amp_token()
                    .map_or(ty.syntax().text_range(), |t| t.text_range()),
                mutable: r.mut_token().is_some(),
                pointee: Box::new(pointee),
            };
        }
        ast::Type::SliceType(slice) => {
            return TypeRef::Slice(Box::new(lower_type(slice.ty(), diagnostics)));
        }
    };
    unsupported(diagnostics, ty.syntax().text_range(), what);
    TypeRef::Error
}

/// Lowers the length of an array type or repeat expression, `expr`,
/// missing (and reported by the parser) within `whole`.
pub(crate) fn lower_array_len(
    expr: Option<ast::Expr<'_>>,
    whole: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> ArrayLen {
    let Some(expr) = expr else {
        return ArrayLen {
            kind: ArrayLenKind::Error,
            range: whole,
        };
    };
    let range = expr.syntax().text_range();
    let kind = match expr {
        ast::Expr::ParenExpr(paren) => return lower_array_len(paren.expr(), range, diagnostics),
        ast::Expr::Literal(literal) => match literal.kind() {
            Some(ast::LiteralKind::Int) => {
                let parsed = literal.value().map(|t| crate::literal::parse_int(t.text()));
                match parsed {
                    Some(Ok(crate::Literal::Int { value, suffix })) => {
                        ArrayLenKind::Lit { value, suffix }
                    }
                    Some(Err(message)) => {
                        diagnostics.push(Diagnostic::new(range, message));
                        ArrayLenKind::Error
                    }
                    _ => ArrayLenKind::Error,
                }
            }
            Some(ast::LiteralKind::Bool(_)) => {
                diagnostics.push(Diagnostic::new(
                    range,
                    "mismatched types: expected `usize`, found `bool`",
                ));
                ArrayLenKind::Error
            }
            _ => {
                unsupported(
                    diagnostics,
                    range,
                    "array lengths other than integers and constants",
                );
                ArrayLenKind::Error
            }
        },
        ast::Expr::PathExpr(path) => {
            match path.path().and_then(|p| lower_generic_path(p, diagnostics)) {
                Some(path) => ArrayLenKind::Path(path),
                None => ArrayLenKind::Error,
            }
        }
        _ => {
            unsupported(
                diagnostics,
                range,
                "array lengths other than integers and constants",
            );
            ArrayLenKind::Error
        }
    };
    ArrayLen { kind, range }
}

/// `fn(A, B) -> R`, `unsafe fn(A)`; `extern "C" fn` and C-variadic ones
/// are reported.
fn lower_fn_ptr(ptr: ast::FnPtrType<'_>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    if let Some(abi) = ptr.abi() {
        unsupported(
            diagnostics,
            abi.syntax().text_range(),
            "`extern` function pointer types",
        );
        return TypeRef::Error;
    }
    let mut params = Vec::new();
    if let Some(list) = ptr.param_list() {
        for param in list.params() {
            check_inert_attrs(param.attrs(), diagnostics);
            if let Some(dots) = param.dot3_token() {
                unsupported(
                    diagnostics,
                    dots.text_range(),
                    "C-variadic function pointer types",
                );
                return TypeRef::Error;
            }
            params.push(lower_type(param.ty(), diagnostics));
        }
    }
    let ret = match ptr.ret_type() {
        Some(ret) => lower_type(ret.ty(), diagnostics),
        None => TypeRef::unit(),
    };
    TypeRef::FnPtr {
        params: params.into(),
        ret: Box::new(ret),
        is_unsafe: ptr.unsafe_token().is_some(),
    }
}

/// Whether `name` is a primitive type's.
/// The [`ImplTarget::Primitive`] name of `core`'s `impl<T> [T]` blocks,
/// which no path can name.
pub fn slice_target() -> Symbol {
    Symbol::intern("slice")
}

pub fn is_primitive(name: Symbol) -> bool {
    [
        sym::bool,
        sym::char,
        sym::str,
        sym::f32,
        sym::f64,
        sym::i8,
        sym::i16,
        sym::i32,
        sym::i64,
        sym::i128,
        sym::isize,
        sym::u8,
        sym::u16,
        sym::u32,
        sym::u64,
        sym::u128,
        sym::usize,
    ]
    .contains(&name)
}
