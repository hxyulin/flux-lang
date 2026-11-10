//! The items of a compilation and their signatures: every crate's
//! modules, read from their files, and the functions, structs, `impl`
//! blocks and imports in them.

mod attributes;
mod collect;
#[path = "flib/crate_data.rs"]
mod flib_data;
mod generics;
mod imports;
mod sized_dispatch;

pub(crate) use attributes::{attr_name, check_attr, check_attrs};
use attributes::{
    check_inert_attrs, check_inner_attrs, enum_repr, non_exhaustive_attr, repr_attr, struct_repr,
    target_feature_attr, target_feature_inline,
};
pub(crate) use generics::lower_type;
use generics::{collect_generics, collect_generics_with_drop};
pub use imports::Uses;
pub use sized_dispatch::SizedDispatch;

use std::path::{Path as FsPath, PathBuf};

use flux_arena::{Idx, LayeredArena};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::ast::{self, AstChildren, AstNode};
use flux_syntax::{NodeId, SyntaxElement, SyntaxKind, SyntaxNode, T, TextRange};

use std::sync::{Arc, Mutex};

use crate::Diagnostic;
use crate::def_map::DefMap;
use crate::macros::{self, MacroDef, MacroId, MacroScope, MacroScopeId, RECURSION_LIMIT};
use crate::source::{FileId, SourceDb, SourceFile};

/// Built-in derives recognized during item collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derive {
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Debug,
    Hash,
}

pub type FunctionId = Idx<Function>;
pub type StructId = Idx<Struct>;
pub type ImplId = Idx<Impl>;
pub type ModuleId = Idx<Module>;
pub type ConstId = Idx<Const>;
pub type TraitId = Idx<Trait>;
pub type TypeAliasId = Idx<TypeAlias>;

/// How a native library is linked: `#[link(kind = ...)]`, or the `KIND` of
/// a `-l KIND=NAME` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeKind {
    /// A shared library, or whatever the linker's `-l` search finds first.
    Dylib,
    /// A static archive, `libNAME.a`.
    Static,
    /// An Apple framework, `NAME.framework`.
    Framework,
}

/// A native library the final link needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NativeLibrary {
    pub name: Symbol,
    pub kind: NativeKind,
}

/// A crate, an index into [`ItemTree::crates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CrateId(pub u32);

/// The files with ranges of their own (included files), sorted by where
/// their ranges start, so that finding the file of a range is a binary
/// search rather than a scan of every file.
#[derive(Debug, Clone)]
struct SpanIndex {
    /// How many files there were when it was built.
    files: usize,
    /// Each file's first offset, its last (its length past the first,
    /// as `contains_range` allows an empty range at the end), and it.
    spans: Box<[(u32, u32, FileId)]>,
    /// Whether two files' ranges overlap: a range may then be in either,
    /// and `source_span` keeps the scan's answer.
    overlapping: bool,
}

impl SpanIndex {
    fn new(files: &rustc_hash::FxHashMap<FileId, Arc<SourceFile>>) -> SpanIndex {
        let mut spans: Vec<_> = files
            .iter()
            .filter(|(_, source)| source.range_base != 0)
            .map(|(&id, source)| {
                let start = source.range_base;
                (start, start + source.text().len() as u32, id)
            })
            .collect();
        spans.sort_unstable_by_key(|&(start, _, id)| (start, id.0));
        let overlapping = spans.windows(2).any(|pair| pair[1].0 <= pair[0].1);
        SpanIndex {
            files: files.len(),
            spans: spans.into(),
            overlapping,
        }
    }

    /// The file whose range contains `range`, if one does.
    fn find(&self, range: TextRange) -> Option<FileId> {
        let after = self
            .spans
            .partition_point(|&(start, _, _)| start <= range.start());
        let &(_, end, file) = self.spans.get(after.checked_sub(1)?)?;
        (range.end() <= end).then_some(file)
    }
}

/// Every item of every crate, without bodies. `core`'s items are frozen
/// in [`Core`](crate::Core)'s tree and shared by every program's.
#[derive(Debug, Clone, Default)]
pub struct ItemTree {
    pub(crate) artifact_builtins: bool,
    pub crates: Vec<CrateData>,
    pub modules: LayeredArena<Module>,
    /// Free functions, functions in `extern` blocks, and the associated
    /// functions of `impl` blocks.
    pub functions: LayeredArena<Function>,
    pub sized_dispatch: rustc_hash::FxHashMap<FunctionId, SizedDispatch>,
    sized_dispatch_declarations: rustc_hash::FxHashMap<FunctionId, sized_dispatch::Declaration>,
    pub structs: LayeredArena<Struct>,
    pub traits: LayeredArena<Trait>,
    pub impls: LayeredArena<Impl>,
    /// Free and associated constants.
    pub consts: LayeredArena<Const>,
    pub type_aliases: LayeredArena<TypeAlias>,
    pub uses: Uses,
    /// Native libraries requested by literal foreign-block attributes.
    pub native_libraries: Vec<NativeLibrary>,
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
    /// The anonymous constant of each array length, repeat count and
    /// const generic argument written as an expression other than a
    /// literal or a path, by its file and the expression's node.
    pub anon_consts: rustc_hash::FxHashMap<(FileId, NodeId), ConstId>,
    /// The traits the compiler knows, by their `#[lang = "..."]` names.
    pub lang_traits: rustc_hash::FxHashMap<Symbol, TraitId>,
    /// `core`'s structs and enums the compiler knows, by their
    /// `#[lang = "..."]` names: `Option` and `Result`, which `?` works
    /// on, and the ranges.
    pub lang_enums: rustc_hash::FxHashMap<Symbol, StructId>,
    /// `core`'s `#[lang = "panic"]` function, which aborts, and which
    /// constant evaluation reports as a panic with its message.
    pub panic_fn: Option<FunctionId>,
    /// `core`'s other functions with `#[lang = "..."]` names, by them.
    pub lang_fns: rustc_hash::FxHashMap<Symbol, FunctionId>,
    /// The constructor function of each tuple struct and tuple variant
    /// (variant 0 for a struct), which its path is as a value.
    pub ctor_fns: rustc_hash::FxHashMap<(StructId, u32), FunctionId>,
    /// The variant each enum deriving `Default` marks `#[default]`.
    pub default_variants: rustc_hash::FxHashMap<StructId, u32>,
    /// Every crate's macros: its `macro_rules!` definitions, and those
    /// the compiler implements, which `core` declares.
    pub macros: LayeredArena<MacroDef>,
    /// The textual scopes of `macro_rules!` macros that items record.
    pub macro_scopes: LayeredArena<MacroScope>,
    /// The textual scopes that define each name.
    pub macro_names: rustc_hash::FxHashMap<Symbol, Vec<MacroScopeId>>,
    /// Each crate's `#[macro_export]` macros, which paths from its root
    /// name (`crate::m!`, `core::assert!`).
    pub exported_macros: rustc_hash::FxHashMap<(CrateId, Symbol), MacroId>,
    /// `core`'s exported macros, which every crate sees by name: rustc's
    /// `macro_use` prelude.
    pub macro_prelude: rustc_hash::FxHashMap<Symbol, MacroId>,
    /// The files read, for `line!`, `column!` and `file!`. Add them with
    /// [`ItemTree::insert_file`], which keeps `span_index` current.
    pub files: rustc_hash::FxHashMap<FileId, Arc<SourceFile>>,
    /// The ranges of the files that have their own, for `source_span`.
    span_index: std::sync::OnceLock<SpanIndex>,
    /// Diagnostic files retained by decoded crates, including body-only includes.
    pub(crate) artifact_files: rustc_hash::FxHashMap<CrateId, Vec<FileId>>,
    /// The file each expansion of a macro call in item position is part
    /// of, by the file its items are in.
    pub expansion_files: rustc_hash::FxHashMap<FileId, FileId>,
    /// Body expansions retained so collection and lowering share block identities.
    pub(crate) body_expansions: rustc_hash::FxHashMap<(FileId, NodeId), BodyExpansion>,
    /// Retained body definitions keep collection and lowering hygiene identical.
    pub(crate) body_macro_definitions: rustc_hash::FxHashMap<(FileId, NodeId), macros::MacroId>,
    /// `use m;` imports of `macro_rules!` macros, for the def map.
    pub(crate) macro_imports: Vec<MacroImport>,
    /// Problems found collecting the items and resolving their names;
    /// [`lower`](crate::lower) moves them into the [`Hir`](crate::Hir)'s.
    pub diagnostics: Vec<Diagnostic>,
    /// How many of each item came from [`Core`](crate::Core), built and
    /// resolved before the program's: the items with smaller indices
    /// are `core`'s. Zero in a tree without `core`, or `core`'s own.
    pub core: ItemCounts,
}

/// A body macro expansion retained for block items and source identity.
#[derive(Debug, Clone)]
pub(crate) struct BodyExpansion {
    pub definition: macros::MacroId,
    pub file: FileId,
    pub ctxts: Vec<(u32, u32)>,
    pub fragment: flux_syntax::parser::Fragment,
    /// Whether the item tree has the anonymous constants in `file`: those
    /// of an expansion in a function's body.
    pub anon_consts: bool,
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
    pub(crate) input_trace: crate::inputs::InputTrace,
    /// Compiler process cwd, distinct from CARGO_MANIFEST_DIR.
    pub compilation_dir: Option<PathBuf>,
    /// Isolated compilation inputs; None retains standalone process lookup.
    pub environment: Option<Arc<std::collections::BTreeMap<String, String>>>,
    /// Exported macros imported with this crate's `#[macro_use] extern crate`.
    pub macro_use: Vec<(Symbol, MacroId)>,
    pub cfg: Option<Arc<flux_syntax::CfgOptions>>,
    pub edition: flux_syntax::Edition,
    /// Maximum macro expansion depth selected by the invocation crate.
    pub recursion_limit: usize,
    pub name: Symbol,
    pub root: ModuleId,
    /// flux's own `core` or `alloc`, which may define methods on
    /// primitive types.
    pub is_core: bool,
    /// The crates its `extern crate` declarations name (`alloc`), which
    /// it may name by path as it names `core`.
    pub externs: Vec<Symbol>,
    /// Root `extern crate` aliases also enter the extern prelude.
    pub extern_aliases: Vec<(Symbol, CrateId)>,
    /// Crates actually loaded by declarations or resolved external paths.
    /// This is separate from aliases: a nested declaration also loads a crate.
    loaded_externs: LoadedCrates,
    /// Direct source dependencies, under this crate's local import names.
    pub dependencies: Vec<(Symbol, CrateId)>,
    /// Use core's prelude and omit the implicit std extern/macro preludes.
    pub no_std: bool,
}

/// Resolution can load metadata through an immutable item-tree query. Cloning
/// a tree snapshots the loaded edges rather than sharing mutable graph state.
#[derive(Debug, Default)]
struct LoadedCrates(Mutex<Vec<CrateId>>);

impl Clone for LoadedCrates {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap().clone()))
    }
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

/// `use m;` of a `macro_rules!` macro in textual scope where it is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MacroImport {
    pub module: ModuleId,
    pub name: Symbol,
    pub def: MacroId,
    pub vis: Visibility,
    pub file: FileId,
    pub range: TextRange,
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
    /// The imported symbol of a foreign static, which has no initializer.
    pub extern_symbol: Option<Symbol>,
    /// `#[rustc_specialization]` in `core`: an associated constant
    /// standing in for what rustc's `core` learns by specialization
    /// (whether an iterator is `TrustedLen`, ...), which as there does
    /// not keep its trait off trait objects.
    pub specialization: bool,
    /// An anonymous constant (`[u8; N + 1]`): see `AnonConst`. Its
    /// `node` is the expression.
    pub anon: Option<AnonConst>,
    /// The `macro_rules!` macros in textual scope where it is defined,
    /// which its value may call.
    pub macro_scope: Option<MacroScopeId>,
}

/// An array length, repeat count or const generic argument written as an
/// expression other than a literal or a path (`[u8; N + 1]`): an
/// anonymous constant, of type `usize` for a length, which sees no
/// generic parameters (rustc's `min_const_generics`), and `Self` only
/// in an `impl` block without any, which is then its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnonConst {
    /// The non-generic ADT whose declaration contains this field constant.
    pub self_adt: Option<StructId>,
    /// The function whose signature or body it is in, whose locals it
    /// cannot use (E0435).
    pub function: Option<FunctionId>,
    /// The item it is in, and how many anonymous constants come before
    /// it there: rustc's `f::{constant#0}`.
    pub item: Symbol,
    pub index: u32,
    /// A const generic argument (`f::<{ N + 1 }>`), whose type is its
    /// parameter's, rather than an array's length.
    pub is_arg: bool,
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
    pub maybe_unsized: bool,
    pub name: Symbol,
    /// Family-local parameters and predicates, separate from the trait's.
    pub generics: Generics,
    /// The bounds every type given for it must meet.
    pub bounds: Box<[Bound]>,
    /// Outlives bounds on each selected output, not on the family receiver.
    pub lifetime_bounds: Box<[LifetimeRef]>,
    pub name_range: TextRange,
    /// The whole item.
    pub item_range: TextRange,
}

/// An associated type's definition in a trait's `impl` block:
/// `type Out = i32;`.
#[derive(Debug, Clone)]
pub struct ImplType {
    pub name: Symbol,
    /// Family-local parameters and predicates, separate from the impl header.
    pub generics: Generics,
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub name_range: TextRange,
    pub item_range: TextRange,
}

/// A trait bound as written: `Tr` or `Tr<A>` in `T: Tr`, or the trait of
/// `impl Tr for S`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bound {
    /// A non-callable `for<...>` binder, scoped to this predicate. Callable
    /// binders retain their existing argument-list recipes instead.
    pub bound_lifetimes: Box<[Symbol]>,
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
    /// `impl dyn Tr`: trait objects of a trait.
    Object(TraitId),
}

/// A struct, `struct S { a: i32 }`, `struct P(i32);` or `struct U;`, or
/// an enum, `enum E { A, B(i32), C { x: i32 } }`: a type made of fields,
/// or of one of several variants, each made of fields. They share an
/// arena, since their generics, bounds and `impl` blocks work alike.
#[derive(Debug, Clone)]
pub struct Struct {
    /// Additional fields or variants may be added by the defining crate.
    pub non_exhaustive: bool,
    /// Fields overlap in shared storage; never use struct field semantics.
    pub is_union: bool,
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
    /// `#[repr(C)]`: fields are laid out in declaration order, as C
    /// does, and an enum's discriminant is a C `int` at least.
    pub repr_c: bool,
    /// A transparent wrapper, whose nontrivial field determines its layout.
    pub repr_transparent: bool,
    /// Minimum aggregate alignment from `#[repr(align(N))]`, or 1.
    pub repr_align: u32,
    /// Maximum field alignment from `#[repr(packed(N))]`.
    pub repr_pack: Option<u32>,
    pub loc: ItemLoc,
    pub name_range: TextRange,
    /// `struct S` or `enum E`, where rustc reports errors about the
    /// whole item.
    pub def_range: TextRange,
}

/// A variant of an enum: `A`, `B(i32)` or `C { x: i32 }`.
#[derive(Debug, Clone)]
pub struct Variant {
    /// Fields may be added outside this crate's view of the variant.
    pub non_exhaustive: bool,
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

/// How a function asks to be inlined: `#[inline]`, `#[inline(always)]`
/// or `#[inline(never)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineHint {
    Hint,
    Always,
    Never,
}

/// A function ABI flux implements. rustc gives each one its own function
/// pointer type. On flux's targets every ABI but `Rust` uses the C calling
/// convention, and an `-unwind` ABI lowers like its base ABI because flux
/// builds with `panic=abort`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Abi {
    #[default]
    Rust,
    C,
    CUnwind,
    System,
    SystemUnwind,
    Efiapi,
}

impl Abi {
    /// Whether an exception may cross this function's boundary.
    pub fn can_unwind(self) -> bool {
        matches!(self, Abi::Rust | Abi::CUnwind | Abi::SystemUnwind)
    }

    /// Whether calls use the C calling convention.
    pub fn is_c(self) -> bool {
        self != Abi::Rust
    }

    /// The name written in `extern "name"`.
    pub fn name(self) -> &'static str {
        match self {
            Abi::Rust => "Rust",
            Abi::C => "C",
            Abi::CUnwind => "C-unwind",
            Abi::System => "system",
            Abi::SystemUnwind => "system-unwind",
            Abi::Efiapi => "efiapi",
        }
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
    /// Target features explicitly enabled on this function. `Some([])` keeps
    /// an empty attribute distinct: it still excludes Fn-family impls.
    pub target_features: Option<Box<[Symbol]>>,
    /// Declared in an `extern` block, so defined elsewhere (in C).
    pub is_extern: bool,
    /// An explicit native symbol, from `link_name` or `no_mangle`.
    pub link_name: Option<Symbol>,
    /// The ABI written on the function, or its `extern` block's.
    pub abi: Abi,
    /// `const fn`: callable in constants.
    pub is_const: bool,
    /// Returns an implicit opaque future; its final opaque declaration owns
    /// the written output type and captures every input lifetime.
    pub is_async: bool,
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
    /// `#[rustc_intrinsic]`, restricted to embedded compiler-owned libraries:
    /// the compiler
    /// replaces its calls with what it does once their types are known
    /// (`mono::expand_intrinsics`), so its body never runs.
    pub intrinsic: Option<crate::Intrinsic>,
    /// Core-only legacy calling convention: one const argument at this
    /// zero-based position in a direct call, omitted from runtime arguments.
    pub legacy_const_generic: Option<u32>,
    /// `#[track_caller]`: code generation passes it where it is called
    /// from, which `Location::caller` and its panics report.
    pub track_caller: bool,
    /// Its `#[inline]` attribute, which code generation passes to keel.
    pub inline: Option<InlineHint>,
    /// `#[cold]`: rarely called, so optimized for size and not inlined.
    pub cold: bool,
    /// `#[rustc_do_not_const_check]`, which only `core` may write: a
    /// `const fn` whose body is not checked as one, as constant
    /// evaluation never runs it (`core`'s panics, which it reports).
    pub do_not_const_check: bool,
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
    /// The `impl Trait` types of its return type, which
    /// [`TypeRef::Opaque`] names by index: each an opaque type whose
    /// hidden type its body decides.
    pub opaques: Box<[ImplTraitRef]>,
    /// Made by `#[derive]` on this struct, with a body made for its
    /// fields: `clone`, `eq`, `Eq`'s check, `partial_cmp` or `cmp`.
    /// `Derive::Copy` is a `clone`
    /// that copies, for a struct without parameters that derives `Copy`
    /// too, as rustc's does.
    pub derived: Option<(StructId, Derive)>,
    /// The tuple struct (variant 0) or variant whose constructor this
    /// is, as a function: `fn Some<T>(_0: T) -> Option<T>`, made rather
    /// than written.
    pub ctor: Option<(StructId, u32)>,
    /// The `macro_rules!` macros in textual scope where it is defined,
    /// which its body may call.
    pub macro_scope: Option<MacroScopeId>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub ty: TypeRef,
    /// The binding's name, if the pattern is one: `x` in `x: i32`,
    /// `self`.
    pub name: Option<Symbol>,
    /// The type's range; `self`'s whole, if its type is not written.
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
    /// Trusted Drop implementation promises not to inspect this lifetime.
    pub may_dangle: bool,
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
    /// Trusted Drop implementation promises not to inspect this type parameter.
    pub may_dangle: bool,
    pub name: Symbol,
    pub range: TextRange,
    /// A trailing type or const default. Const values use TypeRef::Const
    /// and retain their parameter's declared kind and type.
    pub default: Option<(TypeRef, TextRange)>,
    /// Made for an `impl Trait` parameter type (`x: impl Copy`), named
    /// as it is written, which turbofish cannot give.
    pub synthetic: bool,
    pub kind: ParamKind,
    /// `T: ?Sized`, in the list or the `where` clause: the parameter has
    /// no implicit `Sized` bound.
    pub maybe_unsized: bool,
}

/// Whether a generic parameter is a type or a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamKind {
    Type,
    /// `const N: usize`: a constant of type `ty` (an integer type, `bool`
    /// or `char`), written at `ty_range`.
    Const {
        ty: TypeRef,
        ty_range: TextRange,
    },
}

impl GenericParam {
    pub fn is_const(&self) -> bool {
        matches!(self.kind, ParamKind::Const { .. })
    }
}

/// Where defaults for type parameters may be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Defaults {
    /// A trait's: supported.
    Trait,
    /// A struct's or enum's: supported, applied in types that leave
    /// them out.
    Struct,
    /// A type alias's: supported, without enforcing the alias's bounds.
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
        let names: Vec<String> = (self.lifetimes.iter().map(|l| l.name.to_string()))
            .chain(self.params.iter().map(|p| match &p.kind {
                ParamKind::Type => p.name.to_string(),
                ParamKind::Const { ty, .. } => format!("const {}: {ty}", p.name),
            }))
            .collect();
        write!(f, "<{}>", names.join(", "))
    }
}

impl Generics {
    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// How many of its type parameters are written, rather than made
    /// for `impl Trait` parameter types: what generic arguments may
    /// give.
    pub fn explicit_len(&self) -> usize {
        self.params.iter().filter(|p| !p.synthetic).count()
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
    /// `use a::Trait as _;`: checked but not bound to a name.
    Anonymous { module_only: bool },
    /// `use a::{self};`: a module only.
    Module(Symbol),
    /// `use a::*;`
    Glob,
}

/// A path as written: `x`, `a::b::c`, `crate::m::f`, `super::f`, `::core::x`,
/// `Pair::<i32>::new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
    /// Edition of the written path's leading token; generated paths use None.
    pub edition: Option<flux_syntax::Edition>,
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
    /// A projection synthesized from `Trait<Item: Bound>` can name an
    /// inherited item. An explicitly qualified Rust path cannot.
    pub associated_bound: bool,
    pub ty: TypeRef,
    pub ty_range: TextRange,
    pub trait_: Option<Path>,
    /// `<T as Tr>`
    pub range: TextRange,
}

/// Generic arguments as written: `<'a, A, B>` or `::<A, B>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericArgs {
    /// Associated output bounds, expanded into predicates in generic declarations.
    pub bounds: Box<[AssocBound]>,
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
    /// Named lifetimes bound by a parenthesized Fn-family argument list.
    pub bound_lifetimes: Box<[Symbol]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssocBound {
    pub name: Symbol,
    pub args: Option<GenericArgs>,
    pub bounds: Box<[Bound]>,
    pub lifetimes: Box<[LifetimeRef]>,
    pub range: TextRange,
}

/// `Out = i32` in a trait's generic arguments: what one of its associated
/// types is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssocBinding {
    pub name: Symbol,
    /// Arguments of this associated family, independent of its trait's args.
    pub args: Option<GenericArgs>,
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
        for bound in &self.bounds {
            diagnostics.push(Diagnostic::new(bound.range, MISPLACED_BINDING));
        }
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
    /// An expansion's `$crate`, identified independently of import spelling.
    DefinitionCrate(CrateId),
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
            edition: None,
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
            PathAnchor::DefinitionCrate(id) => write!(f, "$crate{}::", id.0)?,
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
                write!(f, "{args}")?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for GenericArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<_> = self
            .lifetimes
            .iter()
            .map(|l| l.name.to_string())
            .chain(self.types.iter().map(ToString::to_string))
            .chain(self.bindings.iter().map(ToString::to_string))
            .collect();
        write!(f, "<{}>", parts.join(", "))
    }
}
impl std::fmt::Display for AssocBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(args) = &self.args {
            write!(f, "{args}")?;
        }
        write!(f, " = {}", self.ty)
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
        bound_lifetimes: Box<[Symbol]>,
        params: Box<[TypeRef]>,
        ret: Box<TypeRef>,
        is_unsafe: bool,
        abi: Abi,
    },
    /// `impl Tr + 'a`, where it is not (yet) known what it stands for.
    /// Lowering an item makes those in a function's parameters
    /// [`TypeRef::ImplParam`]s, those in its return type
    /// [`TypeRef::Opaque`]s, and reports the others.
    ImplTrait(Box<ImplTraitRef>),
    /// `dyn Tr + 'a`: a trait object.
    Dyn(Box<DynRef>),
    /// The type parameter made for an `impl Trait` parameter type, by
    /// its index among the function's, its owner's first.
    ImplParam(u32),
    /// The `impl Trait` of a function's return type with this index in
    /// [`Function::opaques`].
    Opaque(u32),
    /// A const generic argument written as other than a path (`3`,
    /// `{ N + 1 }`); a path (`N`) is a [`TypeRef::Path`] until it is
    /// resolved, as in rustc.
    Const(ArrayLen),
    /// Unsupported or malformed; already reported.
    Error,
}

/// `impl Tr + 'a + use<'a, T>`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImplTraitRef {
    pub maybe_unsized: bool,
    /// The `impl` token's definition-site edition controls implicit captures,
    /// including when a macro expands into a crate of another edition.
    pub edition: flux_syntax::Edition,
    /// Its trait bounds, at least one.
    pub bounds: Box<[Bound]>,
    /// `'a` in `impl Tr + 'a`.
    pub lifetimes: Box<[LifetimeRef]>,
    /// What `use<'a, T>` says it captures, if written.
    pub captures: Option<Box<[UseCapture]>>,
    /// The `use<..>` bound's range, if written.
    pub use_range: TextRange,
    pub range: TextRange,
    /// As written, which is how rustc names it: `impl Iterator<Item = u32>`.
    pub name: Symbol,
}

/// `dyn Tr + 'a`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynRef {
    /// Its trait bounds: the first is its trait, the others are errors
    /// (flux has no auto traits).
    pub bounds: Box<[Bound]>,
    /// `'a` in `dyn Tr + 'a`.
    pub lifetime: Option<LifetimeRef>,
    pub range: TextRange,
}

/// A generic parameter in `use<'a, T>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UseCapture {
    Lifetime(LifetimeRef),
    Param(Symbol, TextRange),
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
    /// A negative integer literal in a legacy const argument.
    NegLit { value: u128, suffix: Option<Symbol> },
    /// A constant resolved in a legacy call's lexical scope.
    Const(ConstId),
    /// A path to a constant, resolved where the type is.
    Path(Path),
    /// `true` or `false`, a const generic argument.
    Bool(bool),
    /// A `char`, a const generic argument.
    Char(char),
    /// Another expression, at this node of this file's tree (a macro
    /// expansion's items are in a file of their own): an anonymous
    /// constant ([`ItemTree::anon_consts`]), unless it was an error
    /// (reported).
    Anon(FileId, NodeId),
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
            ArrayLenKind::NegLit { value, suffix } => {
                write!(f, "-{value}")?;
                if let Some(suffix) = suffix {
                    write!(f, "{suffix}")?;
                }
                Ok(())
            }
            ArrayLenKind::Const(id) => write!(f, "{{const {}}}", id.into_raw()),
            ArrayLenKind::Path(path) => write!(f, "{path}"),
            ArrayLenKind::Bool(b) => write!(f, "{b}"),
            ArrayLenKind::Char(c) => write!(f, "{c:?}"),
            ArrayLenKind::Anon(..) => f.write_str("{const}"),
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
                bound_lifetimes,
                params,
                ret,
                is_unsafe,
                abi,
            } => {
                if !bound_lifetimes.is_empty() {
                    f.write_str("for<")?;
                    for (i, name) in bound_lifetimes.iter().enumerate() {
                        if i != 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{name}")?;
                    }
                    f.write_str("> ")?;
                }
                if *is_unsafe {
                    f.write_str("unsafe ")?;
                }
                if *abi != Abi::Rust {
                    write!(f, "extern \"{}\" ", abi.name())?;
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
            TypeRef::Const(value) => write!(f, "{value}"),
            TypeRef::Slice(elem) => write!(f, "[{elem}]"),
            TypeRef::Infer => f.write_str("_"),
            TypeRef::ImplTrait(it) => write!(f, "{}", it.name),
            TypeRef::Dyn(d) => {
                f.write_str("dyn ")?;
                for (i, b) in d.bounds.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" + ")?;
                    }
                    write!(f, "{}", b.path)?;
                }
                if let Some(l) = d.lifetime {
                    write!(f, " + {}", l.name)?;
                }
                Ok(())
            }
            TypeRef::ImplParam(i) => write!(f, "<impl param {i}>"),
            TypeRef::Opaque(i) => write!(f, "<opaque {i}>"),
            TypeRef::Error => f.write_str("<error>"),
        }
    }
}

impl ItemTree {
    /// Whether the compiler library prefix was loaded from trusted metadata.
    pub fn has_artifact_builtins(&self) -> bool {
        self.artifact_builtins
    }

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

    /// The edition of the source crate containing a module.
    pub fn edition(&self, module: ModuleId) -> flux_syntax::Edition {
        self.crates[self.modules[module].krate.0 as usize].edition
    }

    /// Source crates in dependency order, followed by the executable crate.
    pub fn from_sources(db: &mut SourceDb, sources: &[crate::SourceCrate]) -> ItemTree {
        let mut items = crate::Core::get().items.clone();
        items.core = items.counts();
        let base = items.crates.len() as u32;
        for (index, source) in sources.iter().enumerate() {
            let dependencies = source
                .dependencies
                .iter()
                .map(|&(name, target)| {
                    assert!(
                        target < index,
                        "source dependencies must precede their consumer"
                    );
                    (name, CrateId(base + target as u32))
                })
                .collect();
            items.add_source_crate(db, source.root, source.name, false, dependencies);
        }
        items
    }

    /// Embedded library items, frozen together for [`Core`](crate::Core).
    pub(crate) fn core(db: &mut SourceDb) -> ItemTree {
        let mut items = ItemTree::default();
        items.add_crate(db, FileId::CORE_ROOT, sym::core, true);
        // `alloc`, which only `core` comes before, and which may write
        // what `core` may: methods of primitive types, `lang` items.
        items.add_crate(db, FileId::alloc_root(), sym::alloc, true);
        // Std may define its own inherent primitive APIs and intrinsics,
        // while compiler lang items remain restricted to core/alloc.
        items.add_crate(db, FileId::std_root(), sym::std, false);
        items.modules.freeze();
        items.functions.freeze();
        items.structs.freeze();
        items.traits.freeze();
        items.impls.freeze();
        items.consts.freeze();
        items.type_aliases.freeze();
        items.macros.freeze();
        items.macro_scopes.freeze();
        items
    }

    /// Collects a crate after the ones already here, and resolves its
    /// names. It may implement their traits, but cannot change their
    /// items or scopes: `core` sees no other crate.
    fn add_crate(&mut self, db: &mut SourceDb, root: FileId, name: Symbol, is_core: bool) {
        self.add_source_crate(db, root, name, is_core, Vec::new());
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

    pub(crate) fn macro_recursion_limit(&self, module: ModuleId) -> usize {
        self.crates[self.modules[module].krate.0 as usize].recursion_limit
    }

    pub fn module_of(&self, krate: CrateId) -> ModuleId {
        self.crates[krate.0 as usize].root
    }

    /// The crate being compiled, rather than `core`.
    pub fn main_crate(&self) -> CrateId {
        CrateId(self.crates.len() as u32 - 1)
    }

    /// The restriction belongs to the definition, including through aliases and reexports.
    pub fn externally_non_exhaustive(&self, strukt: StructId, from: ModuleId) -> bool {
        let def = &self.structs[strukt];
        def.non_exhaustive && self.modules[def.loc.module].krate != self.modules[from].krate
    }

    /// Whether a variant's fields are open in another crate.
    pub fn externally_non_exhaustive_variant(
        &self,
        strukt: StructId,
        variant: u32,
        from: ModuleId,
    ) -> bool {
        let def = &self.structs[strukt];
        def.variants
            .as_ref()
            .and_then(|v| v.get(variant as usize))
            .is_some_and(|v| v.non_exhaustive)
            && self.modules[def.loc.module].krate != self.modules[from].krate
    }

    pub fn is_core(&self, module: ModuleId) -> bool {
        self.crates[self.modules[module].krate.0 as usize].is_core
    }

    /// The embedded standard library, identified by its reserved source root.
    /// A user source crate named `std` has no compiler-owned privileges.
    pub fn is_std(&self, module: ModuleId) -> bool {
        let krate = &self.crates[self.modules[module].krate.0 as usize];
        self.modules[krate.root].file == FileId::std_root()
    }

    /// Record actual metadata loading without changing an extern's visibility.
    pub(crate) fn load_extern(&self, from: CrateId, target: CrateId) {
        let mut loaded = self.crates[from.0 as usize]
            .loaded_externs
            .0
            .lock()
            .unwrap();
        if !loaded.contains(&target) {
            loaded.push(target);
        }
    }

    /// Std's primitive methods are visible when this crate loads the trusted
    /// std root, directly or through metadata of a loaded dependency. Merely
    /// declaring an available dependency does not load it.
    pub fn std_available(&self, module: ModuleId) -> bool {
        let mut pending = vec![self.modules[module].krate];
        let mut seen = vec![false; self.crates.len()];
        while let Some(id) = pending.pop() {
            if std::mem::replace(&mut seen[id.0 as usize], true) {
                continue;
            }
            let krate = &self.crates[id.0 as usize];
            if self.is_std(krate.root) || (!krate.is_core && !krate.no_std) {
                return true;
            }
            pending.extend(krate.loaded_externs.0.lock().unwrap().iter().copied());
        }
        false
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
        // A variant's constructor is named through its enum.
        if let Some((strukt, _)) = function.ctor
            && self.structs[strukt].is_enum()
        {
            return format!("{}::{}", self.structs[strukt].name, function.name);
        }
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
            Some(ImplTarget::Object(t)) => {
                format!("<dyn {}>::{}", self.traits[t].name, function.name)
            }
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

    /// The file `file` is part of: itself, or for the items of a macro
    /// call in item position, the call's.
    pub fn real_file(&self, file: FileId) -> FileId {
        self.expansion_files.get(&file).copied().unwrap_or(file)
    }

    /// Adds or replaces source file `id`.
    pub(crate) fn insert_file(
        &mut self,
        id: FileId,
        file: Arc<SourceFile>,
    ) -> Option<Arc<SourceFile>> {
        self.span_index.take();
        self.files.insert(id, file)
    }

    /// Resolves a body span across physical include boundaries.
    pub fn source_span(&self, file: FileId, range: TextRange) -> (FileId, TextRange) {
        let index = self.span_index.get_or_init(|| SpanIndex::new(&self.files));
        debug_assert_eq!(
            index.files,
            self.files.len(),
            "files changed without insert_file"
        );
        let found = if index.overlapping || index.files != self.files.len() {
            self.files
                .iter()
                .find(|(_, source)| source.contains_range(range))
                .map(|(&file, _)| file)
        } else {
            index.find(range)
        };
        match found {
            Some(file) => (file, self.files[&file].local_range(range)),
            None => (self.real_file(file), range),
        }
    }

    /// Resolves a byte position used by runtime caller locations.
    pub fn source_offset(&self, file: FileId, offset: u32) -> (FileId, u32) {
        let (file, range) = self.source_span(file, TextRange::empty(offset));
        (file, range.start())
    }

    /// Restores the physical source location at a diagnostic boundary.
    pub fn resolve_diagnostic(&self, diagnostic: &mut Diagnostic) {
        (diagnostic.file, diagnostic.range) = self.source_span(diagnostic.file, diagnostic.range);
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
        if let Some(anon) = konst.anon {
            return format!("{}::{{constant#{}}}", anon.item, anon.index);
        }
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

/// Whether a node of `kind` is an item.
fn is_item_kind(kind: SyntaxKind) -> bool {
    use SyntaxKind as K;
    matches!(
        kind,
        K::FN
            | K::STRUCT
            | K::UNION
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

/// Evaluates conditional attributes before expansion, collection and lowering.
/// A false predicate does not suppress validation of the other attributes;
/// modules and functions also inherit the cfg on their contents.
pub(crate) fn item_enabled(node: SyntaxNode<'_>, diagnostics: &mut Vec<Diagnostic>) -> bool {
    item_enabled_with_test_context(node, diagnostics, None)
}

/// Macro-expanded impl items have a fragment root rather than their impl as
/// syntax parent. Their collector supplies the actual item context explicitly.
fn item_enabled_with_test_context(
    node: SyntaxNode<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    free_test_function: Option<bool>,
) -> bool {
    let attrs = crate::cfg::Attrs::new(node.children().filter_map(ast::Attr::cast));
    let mut enabled = attrs.errors.is_empty();
    diagnostics.extend(
        attrs
            .errors
            .iter()
            .map(|(r, m)| Diagnostic::new(*r, m.clone())),
    );
    for attr in attrs.iter() {
        if attr_name(attr) != Some("cfg") || attr.meta().is_some_and(|m| m.unsafe_token().is_some())
        {
            continue;
        }
        let meta = attr.meta().expect("cfg has a meta path");
        let mut tokens = Vec::new();
        if let Some(tree) = meta.token_tree() {
            macros::tt::tokens_of(tree.syntax(), &mut tokens);
        }
        let range = attr.syntax().text_range();
        let result = match tokens.as_slice() {
            [open, inner @ .., close] if open.kind == T!['('] && close.kind == T![')'] => {
                crate::cfg::eval(range, inner, attrs.cfg.as_deref())
            }
            _ => Err((range, "expected `cfg` predicate in parentheses".into())),
        };
        match result {
            Ok(value) => enabled &= value,
            Err((range, message)) => {
                diagnostics.push(Diagnostic::new(range, message));
                enabled = false;
            }
        }
    }
    if enabled {
        for attr in attrs.iter().filter(|attr| attr_name(*attr) == Some("test")) {
            let range = attr.syntax().text_range();
            let meta = attr.meta().expect("named attribute");
            if meta.unsafe_token().is_some() {
                diagnostics.push(Diagnostic::new(range, "`test` is not an unsafe attribute"));
            } else if meta.token_tree().is_some() || meta.eq_token().is_some() {
                diagnostics.push(Diagnostic::new(
                    range,
                    "attribute must be of the form `#[test]`",
                ));
            } else if !free_test_function.unwrap_or_else(|| {
                node.kind() == SyntaxKind::FN
                    && !node.parent().is_some_and(|parent| {
                        parent.kind() == SyntaxKind::ASSOC_ITEM_LIST
                            || parent.kind() == SyntaxKind::EXTERN_BLOCK
                            || parent
                                .parent()
                                .is_some_and(|owner| owner.kind() == SyntaxKind::EXTERN_BLOCK)
                    })
            }) {
                diagnostics.push(Diagnostic::new(
                    range,
                    "the `test` attribute may only be used on a free function",
                ));
            }
            // Flux builds ordinary crates, not libtest harnesses. Like rustc
            // without `--test`, it removes test functions before resolving their
            // signatures, bodies, or body-local macros, even with `--cfg test`.
            enabled = false;
        }
    }
    // Inner attributes belong to the enclosing module, and can remove it
    // before its name, imports, or macros are collected.
    if matches!(
        node.kind(),
        SyntaxKind::MODULE | SyntaxKind::FN | SyntaxKind::IMPL | SyntaxKind::TRAIT
    ) && let Some(list) = node
        .children()
        .find(|n| matches!(n.kind(), SyntaxKind::ITEM_LIST | SyntaxKind::BLOCK_EXPR))
    {
        enabled &= item_enabled(list, diagnostics);
    }
    enabled
}

/// Positions whose enclosing lowering step can remove the attributed node.
pub(crate) fn cfg_position(p: SyntaxNode<'_>) -> bool {
    is_item_kind(p.kind())
        || matches!(
            p.kind(),
            SyntaxKind::SOURCE_FILE
                | SyntaxKind::ITEM_LIST
                | SyntaxKind::LET_STMT
                | SyntaxKind::EXPR_STMT
                | SyntaxKind::MATCH_ARM
                | SyntaxKind::RECORD_PAT_FIELD
                | SyntaxKind::RECORD_EXPR_FIELD
                | SyntaxKind::RECORD_FIELD
                | SyntaxKind::TUPLE_FIELD
                | SyntaxKind::VARIANT
                | SyntaxKind::PARAM
                | SyntaxKind::SELF_PARAM
        )
        || ast::Expr::can_cast(p.kind())
            && p.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    SyntaxKind::ARG_LIST
                        | SyntaxKind::TUPLE_EXPR
                        | SyntaxKind::BLOCK_EXPR
                        | SyntaxKind::EXPR_STMT
                        | SyntaxKind::FN
                        | SyntaxKind::MACRO_STMTS
                ) || ast::ArrayExpr::cast(parent)
                    .is_some_and(|array| array.semicolon_token().is_none())
            })
}

/// A path to a trait, as [`lower_generic_path`], where the last segment
/// may have `Fn`-family arguments: `Fn(i32) -> i32`.
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
            associated_bound: false,
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
        Some(colons.raw_range().start() < name.syntax().raw_range().start())
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
            anchor = macros::definition_crate(name.syntax().ctxt())
                .map_or(PathAnchor::Crate, PathAnchor::DefinitionCrate);
            anchors += 1;
        } else if kind == Some(T![super]) {
            anchor = match anchor {
                _ if !names.is_empty() => {
                    misplaced("start position", diagnostics);
                    return None;
                }
                PathAnchor::Plain | PathAnchor::SelfModule => PathAnchor::Super(1),
                PathAnchor::Super(n) => PathAnchor::Super(n + 1),
                PathAnchor::Crate | PathAnchor::DefinitionCrate(_) | PathAnchor::Global => {
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
        edition: path
            .syntax()
            .tokens()
            .find(|t| !t.kind().is_trivia())
            .map(|t| t.edition()),
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
    let mut associated_bounds = Vec::new();
    let mut bindings = Vec::new();
    let mut misordered = false;
    for arg in list.args() {
        let range = arg.syntax().text_range();
        match arg {
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
                bindings.push(AssocBinding {
                    name: Symbol::intern(name.text()),
                    args: arg
                        .generic_arg_list()
                        .map(|list| lower_generic_args(list, diagnostics)),
                    ty: lower_type(arg.ty(), diagnostics),
                    name_range: name.syntax().text_range(),
                    ty_range: arg.ty().map_or(range, |t| t.syntax().text_range()),
                });
                continue;
            }
            ast::GenericArg::ConstArg(arg) => {
                types.push(TypeRef::Const(lower_const_arg(
                    arg.expr(),
                    range,
                    diagnostics,
                )));
                ranges.push(range);
                continue;
            }
            ast::GenericArg::AssocTypeArg(arg) => {
                let Some(name) = arg.name_ref() else {
                    continue;
                };
                let mut bounds = Vec::new();
                let mut lifetimes = Vec::new();
                if let Some(list) = arg.type_bound_list() {
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut lifetimes),
                        Relaxed::Unsupported,
                        diagnostics,
                    );
                }
                associated_bounds.push(AssocBound {
                    name: Symbol::intern(name.text()),
                    args: arg
                        .generic_arg_list()
                        .map(|list| lower_generic_args(list, diagnostics)),
                    bounds: bounds.into(),
                    lifetimes: lifetimes.into(),
                    range,
                });
                continue;
            }
        };
    }
    GenericArgs {
        bounds: associated_bounds.into(),
        lifetimes: lifetimes.into(),
        types: types.into(),
        ranges: ranges.into(),
        bindings: bindings.into(),
        range: list.syntax().text_range(),
        parenthesized: false,
        bound_lifetimes: Box::default(),
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
        bounds: Box::default(),
        lifetimes: Box::default(),
        types: Box::new([TypeRef::Tuple(params)]),
        ranges: Box::new([range]),
        bindings: Box::new([AssocBinding {
            name: Symbol::intern("Output"),
            args: None,
            ty,
            name_range: ty_range,
            ty_range,
        }]),
        range: TextRange::new(range.start(), end.end()),
        parenthesized: true,
        bound_lifetimes: Box::default(),
    }
}

fn generic_param_projections(args: &GenericArgs, params: &[GenericParam], out: &mut Vec<TypeRef>) {
    for ty in &args.types {
        param_projections(ty, params, out);
    }
    for binding in &args.bindings {
        if let Some(own) = &binding.args {
            generic_param_projections(own, params, out);
        }
        param_projections(&binding.ty, params, out);
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
                generic_param_projections(args, params, out);
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
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter() {
                for args in b.path.args.iter().flatten() {
                    generic_param_projections(args, params, out);
                }
            }
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

/// A path into physical embedded core, as `#[derive]` names its traits and their
/// types.
fn core_path(module: &str, name: &str, _in_core: bool) -> Path {
    // Compiler-generated paths name the physical embedded core crate even
    // when edition-2015 user code cannot name it through the crate root.
    let anchor = PathAnchor::DefinitionCrate(CrateId(0));
    let segments = Box::new([Symbol::intern(module), Symbol::intern(name)]);

    Path {
        edition: None,
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
    items_tree: &ItemTree,
    module: ModuleId,
) {
    let meta = attr.meta();
    if meta.is_some_and(|m| m.unsafe_token().is_some() || m.eq_token().is_some()) {
        diagnostics.push(Diagnostic::new(
            attr.syntax().text_range(),
            "malformed `derive` attribute input",
        ));
        return;
    }
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
                let names = item
                    .iter()
                    .filter_map(|e| e.into_token())
                    .filter(|t| t.kind() == SyntaxKind::IDENT)
                    .collect::<Vec<_>>();
                let path = Path {
                    edition: None,
                    anchor: if first.kind() == SyntaxKind::COLON {
                        PathAnchor::Global
                    } else {
                        PathAnchor::Plain
                    },
                    qself: None,
                    segments: names.iter().map(|t| Symbol::intern(t.text())).collect(),
                    ranges: names.iter().map(|t| t.text_range()).collect(),
                    anchor_ranges: Box::default(),
                    args: Box::default(),
                };
                match macros::resolve_predecessor(
                    items_tree,
                    module,
                    &path,
                    crate::def_map::Ns::Types,
                ) {
                    Some(crate::def_map::Resolved::Def(crate::def_map::Def::Trait(id)))
                        if items_tree.is_core(items_tree.traits[id].loc.module) =>
                    {
                        items_tree.traits[id].name.as_str()
                    }
                    _ => {
                        error(range, "cannot find derive macro in this scope".into());
                        continue;
                    }
                }
            }
        };
        let derive = match name {
            "Clone" => Derive::Clone,
            "Copy" => Derive::Copy,
            "PartialEq" => Derive::PartialEq,
            "Eq" => Derive::Eq,
            "PartialOrd" => Derive::PartialOrd,
            "Ord" => Derive::Ord,
            "Default" => Derive::Default,
            "Debug" => Derive::Debug,
            "Hash" => Derive::Hash,
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
    Some(Bound {
        bound_lifetimes: Box::default(),
        path,
        range,
    })
}

/// A lifetime as written.
pub(crate) fn lower_lifetime(lifetime: ast::Lifetime<'_>) -> LifetimeRef {
    LifetimeRef {
        name: lifetime_name(lifetime.text()),
        range: lifetime.syntax().text_range(),
    }
}

/// Raw and ordinary spellings name the same lifetime or loop label.
pub(crate) fn lifetime_name(text: &str) -> Symbol {
    match text.strip_prefix("'r#") {
        Some(name) => Symbol::intern(&format!("'{name}")),
        None => Symbol::intern(text),
    }
}

/// What `?Trait` bounds may be in a list of bounds.
enum Relaxed<'a> {
    /// `?Sized`, collected here (an item's generic parameters).
    Collect(&'a mut Vec<TextRange>),
    /// None: rustc's error for supertraits.
    Supertrait,
    /// Reported as not supported.
    Unsupported,
}

/// The bounds in `list`, `A + B + 'a`: trait bounds into `out`, and
/// lifetime bounds into `lifetimes` where they are allowed (`None`
/// reports them as not supported); `?Sized` as `relaxed` says; others
/// are reported.
fn lower_bounds(
    list: ast::TypeBoundList<'_>,
    out: &mut Vec<Bound>,
    mut lifetimes: Option<&mut Vec<LifetimeRef>>,
    mut relaxed: Relaxed<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for bound in list.bounds() {
        let range = bound.syntax().text_range();
        let what = if bound.lifetime().is_some() && bound.l_paren_token().is_some() {
            let message = "lifetime bounds may not be parenthesized";
            diagnostics.push(Diagnostic::new(range, message));
            continue;
        } else if let Some(lifetime) = bound.lifetime() {
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
            match &mut relaxed {
                Relaxed::Collect(out) => {
                    // The trait is resolved later, but only `Sized` may be
                    // relaxed, so the name is enough.
                    let name = bound
                        .path_type()
                        .and_then(|p| p.path())
                        .and_then(|p| p.segment())
                        .and_then(|s| s.name_ref())
                        .map(|n| n.text());
                    if name == Some("Sized") {
                        out.push(range);
                    } else {
                        diagnostics.push(Diagnostic::new(
                            range,
                            "bound modifier `?` can only be applied to `Sized`",
                        ));
                    }
                    continue;
                }
                Relaxed::Supertrait => {
                    diagnostics.push(Diagnostic::new(
                        range,
                        "relaxed bounds are not permitted in supertrait bounds",
                    ));
                    continue;
                }
                Relaxed::Unsupported => "`?Sized` bounds",
            }
        } else {
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            // Fn-family binders retain named input occurrences, including
            // repeated lifetimes in nested generic arguments.
            let binder = bound
                .for_token()
                .map(|_| binder_names(bound.generic_param_list(), diagnostics));
            if let Some(path) = lower_trait_path(path, diagnostics) {
                let mut lowered = Bound {
                    bound_lifetimes: Box::default(),
                    path,
                    range,
                };
                match binder {
                    Some(Some(names)) if apply_bound_binder(&mut lowered, &names, diagnostics) => {}
                    Some(_) => {
                        unsupported(diagnostics, range, "higher-ranked bounds");
                        continue;
                    }
                    None => {}
                }
                out.push(lowered);
            }
            continue;
        };
        unsupported(diagnostics, range, what);
    }
}

/// `dyn Tr + 'a`, or `Tr + 'a` before edition 2021.
fn lower_dyn(d: ast::DynTraitType<'_>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let range = d.syntax().text_range();
    if d.dyn_token().is_none()
        && d.syntax()
            .tokens()
            .find(|t| !t.kind().is_trivia())
            .is_some_and(|t| {
                !matches!(
                    t.edition(),
                    flux_syntax::Edition::E2015 | flux_syntax::Edition::E2018
                )
            })
    {
        diagnostics.push(Diagnostic::new(range, "expected a type, found a trait"));
        return TypeRef::Error;
    }
    let mut bounds = Vec::new();
    let mut lifetime: Option<LifetimeRef> = None;
    for bound in d.type_bound_list().into_iter().flat_map(|l| l.bounds()) {
        let bound_range = bound.syntax().text_range();
        if let Some(l) = bound.lifetime() {
            if bound.l_paren_token().is_some() {
                diagnostics.push(Diagnostic::new(
                    bound_range,
                    "lifetime bounds may not be parenthesized",
                ));
            } else if lifetime.is_some() {
                diagnostics.push(Diagnostic::new(
                    bound_range,
                    "only a single explicit lifetime bound is permitted",
                ));
            } else {
                lifetime = Some(lower_lifetime(l));
            }
            continue;
        }
        let what = if bound.question_token().is_some() {
            diagnostics.push(Diagnostic::new(
                bound_range,
                "relaxed bounds are not permitted in trait object types",
            ));
            continue;
        } else if bound.use_token().is_some() {
            "`use` bounds in trait objects"
        } else if bound.for_token().is_some() {
            "higher-ranked bounds"
        } else {
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            if let Some(path) = lower_trait_path(path, diagnostics) {
                bounds.push(Bound {
                    bound_lifetimes: Box::default(),
                    path,
                    range: bound_range,
                });
            }
            continue;
        };
        unsupported(diagnostics, bound_range, what);
        return TypeRef::Error;
    }
    if bounds.is_empty() {
        diagnostics.push(Diagnostic::new(
            range,
            "at least one trait is required for an object type",
        ));
        return TypeRef::Error;
    }
    TypeRef::Dyn(Box::new(DynRef {
        bounds: bounds.into(),
        lifetime,
        range,
    }))
}

/// `impl Tr + 'a + use<'a, T>`: what it stands for depends on where it
/// is, which lowering the item decides ([`desugar_impl_params`],
/// [`collect_opaques`], [`reject_impl_trait`]).
fn lower_impl_trait(it: ast::ImplTraitType<'_>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let range = it.syntax().text_range();
    let mut bounds = Vec::new();
    let mut lifetimes = Vec::new();
    let mut maybe_unsized = false;
    let mut captures: Option<Box<[UseCapture]>> = None;
    let mut use_range = TextRange::default();
    // Its name as rustc prints it: the bounds, without `use<..>`.
    let mut names: Vec<String> = Vec::new();
    let collapse = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    for bound in it.type_bound_list().into_iter().flat_map(|l| l.bounds()) {
        let bound_range = bound.syntax().text_range();
        if let Some(lifetime) = bound.lifetime() {
            lifetimes.push(lower_lifetime(lifetime));
            names.push(collapse(bound.syntax().text()));
            continue;
        }
        if bound.use_token().is_some() {
            if captures.is_some() {
                diagnostics.push(Diagnostic::new(
                    bound_range,
                    "duplicate `use<...>` precise capturing syntax",
                ));
                continue;
            }
            use_range = bound_range;
            let mut list = Vec::new();
            if let Some(args) = bound.use_bound_generic_args() {
                list.extend(
                    args.lifetimes()
                        .map(|l| UseCapture::Lifetime(lower_lifetime(l))),
                );
                list.extend(
                    args.name_refs().map(|n| {
                        UseCapture::Param(Symbol::intern(n.text()), n.syntax().text_range())
                    }),
                );
            }
            captures = Some(list.into());
            continue;
        }
        if bound.question_token().is_some() {
            let name = bound
                .path_type()
                .and_then(|p| p.path())
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .map(|n| n.text());
            if name == Some("Sized") {
                if maybe_unsized {
                    diagnostics.push(Diagnostic::new(
                        bound_range,
                        "duplicate relaxed `Sized` bounds",
                    ));
                }
                maybe_unsized = true;
                names.push(collapse(bound.syntax().text()));
                continue;
            }
            diagnostics.push(Diagnostic::new(
                bound_range,
                "bound modifier `?` can only be applied to `Sized`",
            ));
            return TypeRef::Error;
        } else if bound.for_token().is_some() {
            let Some(bound_names) = binder_names(bound.generic_param_list(), diagnostics) else {
                unsupported(diagnostics, bound_range, "higher-ranked bounds");
                continue;
            };
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            if let Some(path) = lower_trait_path(path, diagnostics) {
                let mut lowered = Bound {
                    bound_lifetimes: Box::default(),
                    path,
                    range: bound_range,
                };
                if apply_bound_binder(&mut lowered, &bound_names, diagnostics) {
                    bounds.push(lowered);
                    names.push(collapse(bound.syntax().text()));
                }
            }
            continue;
        } else {
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            if let Some(path) = lower_trait_path(path, diagnostics) {
                bounds.push(Bound {
                    bound_lifetimes: Box::default(),
                    path,
                    range: bound_range,
                });
                names.push(collapse(bound.syntax().text()));
            }
            continue;
        }
    }
    if bounds.is_empty() {
        diagnostics.push(Diagnostic::new(
            range,
            "at least one trait must be specified",
        ));
        return TypeRef::Error;
    }
    TypeRef::ImplTrait(Box::new(ImplTraitRef {
        maybe_unsized,
        edition: it
            .impl_token()
            .expect("impl trait has its keyword")
            .edition(),
        bounds: bounds.into(),
        lifetimes: lifetimes.into(),
        captures,
        use_range,
        range,
        name: Symbol::intern(&format!("impl {}", names.join(" + "))),
    }))
}

/// Where in a type an `impl Trait` is nested, for rustc's E0562.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nested {
    /// Not inside any of the below.
    Site,
    FnPtrParam,
    FnPtrRet,
    /// The parameters of `Fn(A) -> B` in a bound.
    FnParam,
    FnRet,
}

/// Calls `f` on each `impl Trait` in `ty` (not inside them), with where
/// it is nested.
fn visit_impl_traits(ty: &mut TypeRef, at: Nested, f: &mut dyn FnMut(&mut TypeRef, Nested)) {
    match ty {
        TypeRef::ImplTrait(_) => f(ty, at),
        TypeRef::Path(path) => visit_path_impl_traits(path, at, f),
        TypeRef::Tuple(list) => {
            for t in list.iter_mut() {
                visit_impl_traits(t, at, f);
            }
        }
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Ref { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => visit_impl_traits(pointee, at, f),
        TypeRef::FnPtr { params, ret, .. } => {
            let (p, r) = match at {
                Nested::Site => (Nested::FnPtrParam, Nested::FnPtrRet),
                at => (at, at),
            };
            for t in params.iter_mut() {
                visit_impl_traits(t, p, f);
            }
            visit_impl_traits(ret, r, f);
        }
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter_mut() {
                visit_path_impl_traits(&mut b.path, at, f);
            }
        }
        TypeRef::Never
        | TypeRef::Infer
        | TypeRef::ImplParam(_)
        | TypeRef::Opaque(_)
        | TypeRef::Const(_)
        | TypeRef::Error => {}
    }
}

/// [`visit_impl_traits`] for the types in `path`.
fn visit_path_impl_traits(path: &mut Path, at: Nested, f: &mut dyn FnMut(&mut TypeRef, Nested)) {
    if let Some(qself) = &mut path.qself {
        visit_impl_traits(&mut qself.ty, at, f);
        if let Some(t) = &mut qself.trait_ {
            visit_path_impl_traits(t, at, f);
        }
    }
    for args in path.args.iter_mut().flatten() {
        visit_generic_impl_traits(args, at, f);
    }
}

fn visit_generic_impl_traits(
    args: &mut GenericArgs,
    at: Nested,
    f: &mut dyn FnMut(&mut TypeRef, Nested),
) {
    let (p, r) = match (args.parenthesized, at) {
        (true, Nested::Site) => (Nested::FnParam, Nested::FnRet),
        _ => (at, at),
    };
    for t in args.types.iter_mut() {
        visit_impl_traits(t, p, f);
    }
    for b in args.bindings.iter_mut() {
        if let Some(own) = &mut b.args {
            visit_generic_impl_traits(own, p, f);
        }
        visit_impl_traits(&mut b.ty, r, f);
    }
}

/// rustc's E0562 for an `impl Trait` nested `at` in a type written in
/// `site` ("field types").
fn not_allowed(at: Nested, site: &str) -> String {
    let position = match at {
        Nested::Site => site,
        Nested::FnPtrParam => "`fn` pointer parameters",
        Nested::FnPtrRet => "`fn` pointer return types",
        Nested::FnParam => "the parameters of `Fn` trait bounds",
        Nested::FnRet => "the return type of `Fn` trait bounds",
    };
    format!("`impl Trait` is not allowed in {position}")
}

const FIELD_TYPES: ImplTraitSite = ImplTraitSite::NotAllowed("field types");
/// Bounds' generic arguments, where rustc allows an `impl Trait` as an
/// associated type bound would be.
const IN_BOUNDS: ImplTraitSite = ImplTraitSite::Unsupported("`impl Trait` types in bounds");

/// Where an `impl Trait` may not be, what rustc says of one there.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ImplTraitSite {
    /// E0562: "`impl Trait` is not allowed in {0}".
    NotAllowed(&'static str),
    /// E0658: "`impl Trait` in {0} is unstable".
    Unstable(&'static str),
    /// Valid Rust: "{0} are not supported yet".
    Unsupported(&'static str),
}

/// Reports each `impl Trait` in `ty`, written at `site`, and makes it an
/// error.
pub(crate) fn reject_impl_trait(
    ty: &mut TypeRef,
    site: ImplTraitSite,
    diagnostics: &mut Vec<Diagnostic>,
) {
    visit_impl_traits(ty, Nested::Site, &mut |t, at| {
        report_impl_trait(t, at, site, diagnostics);
    });
}

/// [`reject_impl_trait`] for the types in a bound's path.
fn reject_impl_trait_in_path(
    path: &mut Path,
    site: ImplTraitSite,
    diagnostics: &mut Vec<Diagnostic>,
) {
    visit_path_impl_traits(path, Nested::Site, &mut |t, at| {
        report_impl_trait(t, at, site, diagnostics);
    });
}

fn report_impl_trait(
    ty: &mut TypeRef,
    at: Nested,
    site: ImplTraitSite,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let TypeRef::ImplTrait(it) = ty else { return };
    let message = match (site, at) {
        (ImplTraitSite::NotAllowed(site), _) => not_allowed(at, site),
        (_, Nested::Site) => match site {
            ImplTraitSite::Unstable(site) => format!("`impl Trait` in {site} is unstable"),
            ImplTraitSite::Unsupported(what) => format!("{what} are not supported yet"),
            ImplTraitSite::NotAllowed(_) => unreachable!(),
        },
        _ => not_allowed(at, ""),
    };
    diagnostics.push(Diagnostic::new(it.range, message));
    *ty = TypeRef::Error;
}

/// The type parameters a function's `impl Trait` parameter types
/// become, as rustc's are: after its others, each named as written and
/// bounded by its bounds.
#[derive(Debug, Default)]
struct ImplParams {
    /// The index of the first, among the function's parameters.
    first: u32,
    params: Vec<GenericParam>,
    predicates: Vec<WherePredicate>,
    type_outlives: Vec<TypeOutlives>,
}

/// Makes each `impl Trait` in `ty`, a parameter's type, a type
/// parameter of its own ([`TypeRef::ImplParam`]), and those in its
/// bounds too.
fn desugar_impl_params(ty: &mut TypeRef, out: &mut ImplParams, diagnostics: &mut Vec<Diagnostic>) {
    visit_impl_traits(ty, Nested::Site, &mut |t, at| {
        desugar_impl_param(t, at, out, diagnostics)
    });
}

fn desugar_impl_param(
    ty: &mut TypeRef,
    at: Nested,
    out: &mut ImplParams,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if at != Nested::Site {
        report_impl_trait(ty, at, ImplTraitSite::NotAllowed(""), diagnostics);
        return;
    }
    let TypeRef::ImplTrait(it) = std::mem::replace(ty, TypeRef::Error) else {
        return;
    };
    let mut it = *it;
    if it.captures.is_some() {
        diagnostics.push(Diagnostic::new(
            it.use_range,
            "`use<...>` precise capturing syntax not allowed in argument-position `impl Trait`",
        ));
    }
    let index = out.first + out.params.len() as u32;
    out.params.push(GenericParam {
        name: it.name,
        range: it.range,
        default: None,
        synthetic: true,
        kind: ParamKind::Type,
        maybe_unsized: it.maybe_unsized,
        may_dangle: false,
    });
    for bound in it.bounds.iter_mut() {
        visit_path_impl_traits(&mut bound.path, Nested::Site, &mut |t, at| {
            desugar_impl_param(t, at, out, diagnostics)
        });
    }
    let param = TypeRef::ImplParam(index);
    out.predicates
        .extend(it.bounds.iter().map(|bound| WherePredicate {
            ty: param.clone(),
            ty_range: it.range,
            bound: bound.clone(),
        }));
    out.type_outlives
        .extend(it.lifetimes.iter().map(|&lifetime| TypeOutlives {
            ty: param.clone(),
            ty_range: it.range,
            lifetime,
        }));
    *ty = param;
}

/// Makes each `impl Trait` in `ty`, a function's return type, an opaque
/// type ([`TypeRef::Opaque`]) in `out`.
fn collect_opaques(
    ty: &mut TypeRef,
    out: &mut Vec<ImplTraitRef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    visit_impl_traits(ty, Nested::Site, &mut |t, at| {
        if at != Nested::Site {
            report_impl_trait(t, at, ImplTraitSite::NotAllowed(""), diagnostics);
            return;
        }
        let TypeRef::ImplTrait(it) = std::mem::replace(t, TypeRef::Error) else {
            return;
        };
        let mut it = *it;
        if it.maybe_unsized {
            diagnostics.push(Diagnostic::new(
                it.range,
                "`?Sized` return-position `impl Trait` is not supported yet",
            ));
            return;
        }
        for bound in it.bounds.iter_mut() {
            reject_impl_trait_in_path(
                &mut bound.path,
                ImplTraitSite::Unsupported("nested `impl Trait` types"),
                diagnostics,
            );
        }
        *t = TypeRef::Opaque(out.len() as u32);
        out.push(it);
    });
}

thread_local! {
    /// The types macro calls in type position expanded to, lowered, by
    /// their tree and node: [`expand_types`] fills it before the types
    /// around them are lowered, which [`lower_type`] does without
    /// knowing which macros are in scope.
    static MACRO_TYPES: std::cell::RefCell<rustc_hash::FxHashMap<(usize, NodeId), TypeRef>> =
        std::cell::RefCell::default();
}

fn macro_type_key(node: SyntaxNode<'_>) -> (usize, NodeId) {
    (std::ptr::from_ref(node.tree()) as usize, node.id())
}

fn expanded_type(node: SyntaxNode<'_>) -> Option<TypeRef> {
    MACRO_TYPES.with(|m| m.borrow().get(&macro_type_key(node)).cloned())
}

/// Runs `f` with the types of macro calls expanded so far set aside, and
/// forgets those `f` expands: their trees may not outlive it.
pub(crate) fn with_macro_types<R>(f: impl FnOnce() -> R) -> R {
    let saved = MACRO_TYPES.with(|m| std::mem::take(&mut *m.borrow_mut()));
    let result = f();
    MACRO_TYPES.with(|m| *m.borrow_mut() = saved);
    result
}

/// Expands the macro calls in type position in `node` with `expand`,
/// for [`lower_type`] to find.
pub(crate) fn expand_types(
    node: SyntaxNode<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    expand: &mut dyn FnMut(SyntaxNode<'_>, &mut Vec<Diagnostic>) -> Option<flux_syntax::Parse>,
) {
    if !node
        .descendants()
        .any(|d| d.kind() == SyntaxKind::MACRO_TYPE)
    {
        return;
    }
    fn walk(
        node: SyntaxNode<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        expand: &mut dyn FnMut(SyntaxNode<'_>, &mut Vec<Diagnostic>) -> Option<flux_syntax::Parse>,
    ) {
        // Collection reports malformed attributes. This preliminary walk
        // only decides which subtrees may contain macros to expand.
        if !item_enabled(node, &mut Vec::new()) {
            return;
        }
        if node.kind() == SyntaxKind::MACRO_TYPE {
            let ty = match expand(node, diagnostics) {
                Some(parse) => {
                    let root = parse.tree().root();
                    walk(root, diagnostics, expand);
                    lower_type(root.children().find_map(ast::Type::cast), diagnostics)
                }
                None => TypeRef::Error,
            };
            MACRO_TYPES.with(|m| m.borrow_mut().insert(macro_type_key(node), ty));
        } else {
            for child in node.children() {
                walk(child, diagnostics, expand);
            }
        }
    }
    walk(node, diagnostics, expand);
}

thread_local! {
    /// How many expansions of macro calls in a function body the code
    /// being lowered is in: their trees are not files', so the item tree
    /// has no anonymous constants for them.
    static BODY_EXPANSIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Notes entering (`+1`) or leaving (`-1`) an expansion in a body.
pub(crate) fn body_expansion(delta: i32) {
    BODY_EXPANSIONS.with(|c| c.set(c.get().wrapping_add_signed(delta)));
}

thread_local! {
    /// The file whose tree the types being lowered are in, which the
    /// item tree's anonymous constants are keyed by: an item a macro
    /// expansion makes is in a file of its own, not its module's.
    static TYPES_FILE: std::cell::Cell<Option<FileId>> = const { std::cell::Cell::new(None) };
}

/// While it lives, types are lowered from the tree of `file`.
pub(crate) struct TypesFile(Option<FileId>);

impl TypesFile {
    pub(crate) fn enter(file: FileId) -> TypesFile {
        TypesFile(TYPES_FILE.with(|f| f.replace(Some(file))))
    }
}

impl Drop for TypesFile {
    fn drop(&mut self) {
        TYPES_FILE.with(|f| f.set(self.0));
    }
}

/// The anonymous constant `expr` is, if the item tree has it.
fn anon_kind(expr: ast::Expr<'_>, diagnostics: &mut Vec<Diagnostic>) -> ArrayLenKind {
    if BODY_EXPANSIONS.with(std::cell::Cell::get) > 0 {
        unsupported(
            diagnostics,
            expr.syntax().text_range(),
            "constant expressions in macro expansions in function bodies",
        );
        return ArrayLenKind::Error;
    }
    match TYPES_FILE.with(std::cell::Cell::get) {
        Some(file) => ArrayLenKind::Anon(file, expr.syntax().id()),
        None => {
            debug_assert!(false, "types are lowered in a file");
            ArrayLenKind::Error
        }
    }
}

/// Lowers the length of an array type or repeat expression, `expr`,
/// missing (and reported by the parser) within `whole`.
pub(crate) fn lower_array_len(
    expr: Option<ast::Expr<'_>>,
    whole: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> ArrayLen {
    let Some(expr) = expr.map(const_expr_root) else {
        return ArrayLen {
            kind: ArrayLenKind::Error,
            range: whole,
        };
    };
    let range = expr.syntax().text_range();
    if is_anon_const(expr) {
        return ArrayLen {
            kind: anon_kind(expr, diagnostics),
            range,
        };
    }
    let kind = match expr {
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
            Some(ast::LiteralKind::Char) => {
                diagnostics.push(Diagnostic::new(
                    range,
                    "mismatched types: expected `usize`, found `char`",
                ));
                ArrayLenKind::Error
            }
            _ => ArrayLenKind::Error,
        },
        ast::Expr::PathExpr(path) => {
            match path.path().and_then(|p| lower_generic_path(p, diagnostics)) {
                Some(path) => ArrayLenKind::Path(path),
                None => ArrayLenKind::Error,
            }
        }
        _ => ArrayLenKind::Error,
    };
    ArrayLen { kind, range }
}

/// Lowers a const generic argument written as an expression, `expr`,
/// missing (and reported by the parser) within `whole`: a literal, a
/// path in braces (`{ N }`), or an anonymous constant.
pub(crate) fn lower_const_arg(
    expr: Option<ast::Expr<'_>>,
    whole: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> ArrayLen {
    let Some(expr) = expr.map(const_expr_root) else {
        return ArrayLen {
            kind: ArrayLenKind::Error,
            range: whole,
        };
    };
    let range = expr.syntax().text_range();
    let kind = match expr {
        _ if is_anon_const(expr) => anon_kind(expr, diagnostics),
        ast::Expr::Literal(literal) => match literal.kind() {
            Some(ast::LiteralKind::Bool(b)) => ArrayLenKind::Bool(b),
            Some(ast::LiteralKind::Char) => {
                match literal
                    .value()
                    .map(|t| crate::literal::unescape_char(t.text()))
                {
                    Some(Ok(c)) => ArrayLenKind::Char(c),
                    Some(Err(message)) => {
                        diagnostics.push(Diagnostic::new(range, message));
                        ArrayLenKind::Error
                    }
                    None => ArrayLenKind::Error,
                }
            }
            _ => return lower_array_len(Some(expr), whole, diagnostics),
        },
        _ => return lower_array_len(Some(expr), whole, diagnostics),
    };
    ArrayLen { kind, range }
}

/// The expression a constant written as `expr` is: `expr` without
/// parentheses, or braces around nothing but a literal or a path
/// (`{ N }`, which rustc reads as `N`).
pub(crate) fn const_expr_root(expr: ast::Expr<'_>) -> ast::Expr<'_> {
    match expr {
        ast::Expr::ParenExpr(paren) => match paren.expr() {
            Some(inner) => const_expr_root(inner),
            None => expr,
        },
        ast::Expr::BlockExpr(block)
            if block.label().is_none()
                && block.unsafe_token().is_none()
                && block.const_token().is_none()
                && block.async_token().is_none()
                && block.attrs().next().is_none()
                && block.stmts().next().is_none() =>
        {
            match block.tail_expr().map(const_expr_root) {
                Some(inner @ (ast::Expr::Literal(_) | ast::Expr::PathExpr(_))) => inner,
                _ => expr,
            }
        }
        _ => expr,
    }
}

/// Whether the constant `expr`, a [`const_expr_root`], is an anonymous
/// constant: neither a scalar literal nor a single-name path. Paths through
/// types need ordinary expression resolution to select associated constants.
pub(crate) fn is_anon_const(expr: ast::Expr<'_>) -> bool {
    match expr {
        ast::Expr::Literal(literal) => !matches!(
            literal.kind(),
            Some(ast::LiteralKind::Int | ast::LiteralKind::Bool(_) | ast::LiteralKind::Char)
        ),
        ast::Expr::PathExpr(path) => path.path().is_some_and(|path| {
            // A lone identifier may be a const parameter. Preserve it for
            // declaration-scoped substitution; all qualified paths use the
            // same checked/evaluated expression pipeline as const arithmetic.
            path.qualifier().is_some()
                || path.colon2_token().is_some()
                || path.segment().is_some_and(|segment| {
                    segment.generic_arg_list().is_some() || segment.l_angle_token().is_some()
                })
        }),
        _ => true,
    }
}

/// `fn(A, B) -> R`, `unsafe fn(A)`; `extern "C" fn` and C-variadic ones
/// are reported.
fn lower_fn_ptr(ptr: ast::FnPtrType<'_>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let abi = function_abi(ptr.abi(), diagnostics);
    let mut params = Vec::new();
    if let Some(list) = ptr.param_list() {
        for param in list.params() {
            if !item_enabled(param.syntax(), diagnostics) {
                continue;
            }
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
        bound_lifetimes: Box::default(),
        params: params.into(),
        ret: Box::new(ret),
        is_unsafe: ptr.unsafe_token().is_some(),
        abi,
    }
}

/// Whether `name` is a primitive type's.
/// The [`ImplTarget::Primitive`] name of `core`'s `impl<T> [T]` blocks,
/// which no path can name.
pub fn slice_target() -> Symbol {
    Symbol::intern("slice")
}

/// The [`ImplTarget::Primitive`] name of `core`'s `impl<T, const N:
/// usize> [T; N]` blocks.
pub fn array_target() -> Symbol {
    Symbol::intern("array")
}

/// The [`ImplTarget::Primitive`] name of `core`'s `impl<T> *const T`
/// (`const_ptr`) and `impl<T> *mut T` (`mut_ptr`) blocks.
pub fn raw_ptr_target(mutable: bool) -> Symbol {
    Symbol::intern(if mutable { "mut_ptr" } else { "const_ptr" })
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

/// The lifetimes a `for<'a, 'b>` binder declares, if it declares only
/// lifetimes.
fn binder_names(
    list: Option<ast::GenericParamList<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<Symbol>> {
    let list = list?;
    let mut names = Vec::new();
    for param in list.generic_params() {
        match param {
            ast::GenericParam::LifetimeParam(l) => {
                let lifetime = lower_lifetime(l.lifetime()?);
                if lifetime.is_static() {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "invalid lifetime parameter name: `'static`",
                    ));
                } else if lifetime.is_anonymous() {
                    diagnostics.push(Diagnostic::new(lifetime.range, "`'_` cannot be used here"));
                }
                if lifetime
                    .name
                    .as_str()
                    .get(1..)
                    .and_then(SyntaxKind::from_keyword)
                    .is_some()
                    && !lifetime.is_static()
                    && !l
                        .lifetime()
                        .is_some_and(|lifetime| lifetime.text().starts_with("'r#"))
                {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "lifetimes cannot use keyword names",
                    ));
                }
                if let Some(bounds) = l.type_bound_list() {
                    diagnostics.push(Diagnostic::new(
                        bounds.syntax().text_range(),
                        "bounds cannot be used in this context",
                    ));
                }
                // Check before erasing the binder. A block starts a new
                // item scope: nested items do not inherit its lifetimes.
                let mut ancestor = list.syntax().parent();
                while let Some(node) = ancestor {
                    if node.kind() == SyntaxKind::BLOCK_EXPR {
                        break;
                    }
                    let shadows = node.children().filter_map(ast::GenericParamList::cast).any(|g| {
                        g.syntax() != list.syntax() && g.generic_params().any(|p| {
                            matches!(p, ast::GenericParam::LifetimeParam(p)
                                if p.lifetime().is_some_and(|l| lifetime_name(l.text()) == lifetime.name))
                        })
                    });
                    if shadows {
                        diagnostics.push(Diagnostic::new(
                            lifetime.range,
                            format!("lifetime name `{}` shadows a lifetime name that is already in scope", lifetime.name),
                        ));
                        break;
                    }
                    ancestor = node.parent();
                }
                if names.contains(&lifetime.name) {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "lifetime name is already used in this binder",
                    ));
                }
                names.push(lifetime.name);
            }
            _ => return None,
        }
    }
    Some(names)
}

fn apply_bound_binder(
    bound: &mut Bound,
    names: &[Symbol],
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if bind_fn_lifetimes(&mut bound.path, names, diagnostics) {
        return true;
    }
    let mut combined = names.to_vec();
    combined.extend_from_slice(&bound.bound_lifetimes);
    bound.bound_lifetimes = combined.into();
    true
}

/// Retains a Fn-family binder so region lowering can correlate repeated
/// occurrences on references and nested generic arguments.
fn bind_fn_lifetimes(
    path: &mut Path,
    names: &[Symbol],
    _diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let last = path.segments.len().saturating_sub(1);
    let Some(Some(args)) = path.args.get_mut(last) else {
        return false;
    };
    if !args.parenthesized {
        return false;
    }
    let mut combined = names.to_vec();
    combined.extend_from_slice(&args.bound_lifetimes);
    args.bound_lifetimes = combined.into();
    true
}

/// The ABI of `extern "name"`, `extern` (C) or no `extern` (Rust), with
/// the ABIs rustc rejects for the target reported as rustc does.
pub(crate) fn function_abi(abi: Option<ast::Abi<'_>>, diagnostics: &mut Vec<Diagnostic>) -> Abi {
    let Some(abi) = abi else { return Abi::Rust };
    let Some(token) = abi.string_token() else {
        return Abi::C;
    };
    let Ok(name) = crate::literal::unescape_str(token.text()) else {
        return Abi::Rust;
    };
    let x86_64 = std::env::consts::ARCH == "x86_64";
    let message = match name.as_str() {
        "Rust" => return Abi::Rust,
        "C" => return Abi::C,
        "C-unwind" => return Abi::CUnwind,
        "system" => return Abi::System,
        "system-unwind" => return Abi::SystemUnwind,
        // On x86_64, `efiapi` is the Windows x64 convention rather than C.
        "efiapi" if !x86_64 => return Abi::Efiapi,
        "efiapi" | "cdecl" | "cdecl-unwind" => format!("the \"{name}\" ABI is not supported yet"),
        "win64" | "win64-unwind" | "sysv64" | "sysv64-unwind" if x86_64 => {
            format!("the \"{name}\" ABI is not supported yet")
        }
        "win64" | "win64-unwind" | "sysv64" | "sysv64-unwind" | "stdcall" | "stdcall-unwind"
        | "fastcall" | "fastcall-unwind" | "thiscall" | "thiscall-unwind" | "aapcs"
        | "aapcs-unwind" => format!("\"{name}\" is not a supported ABI for the current target"),
        "vectorcall"
        | "vectorcall-unwind"
        | "rust-call"
        | "rust-cold"
        | "rust-invalid"
        | "rust-preserve-none"
        | "unadjusted"
        | "ptx-kernel"
        | "gpu-kernel"
        | "x86-interrupt"
        | "msp430-interrupt"
        | "riscv-interrupt-m"
        | "riscv-interrupt-s"
        | "avr-interrupt"
        | "avr-non-blocking-interrupt"
        | "C-cmse-nonsecure-call"
        | "C-cmse-nonsecure-entry"
        | "custom" => format!("the extern \"{name}\" ABI is experimental and subject to change"),
        _ => format!("invalid ABI: found `{name}`"),
    };
    diagnostics.push(Diagnostic::new(abi.syntax().text_range(), message));
    Abi::Rust
}

#[cfg(test)]
mod drop_parameter_tests {
    use super::*;

    #[test]
    fn internal_drop_promises_preserve_parameter_identity() {
        let parsed = flux_syntax::parse_source_file(
            "unsafe impl<#[may_dangle] 'b, 'a, T, #[may_dangle] U> Drop for Owner<'a, 'b, U, T> {} ",
        );
        assert!(parsed.errors().is_empty());
        let imp = parsed.root().children().find_map(ast::Impl::cast).unwrap();
        let mut errors = Vec::new();
        let generics = collect_generics_with_drop(
            imp.generic_param_list(),
            None,
            Defaults::No,
            true,
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            generics
                .lifetimes
                .iter()
                .map(|p| p.may_dangle)
                .collect::<Vec<_>>(),
            [true, false]
        );
        assert_eq!(
            generics
                .params
                .iter()
                .map(|p| p.may_dangle)
                .collect::<Vec<_>>(),
            [false, true]
        );
        let _ = collect_generics(imp.generic_param_list(), None, Defaults::No, &mut errors);
        assert_eq!(
            errors.len(),
            2,
            "user source cannot acquire the unsafe promises"
        );
    }
}
