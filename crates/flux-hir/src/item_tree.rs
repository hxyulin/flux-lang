//! The items of a compilation and their signatures: every crate's
//! modules, read from their files, and the functions, structs, `impl`
//! blocks and imports in them.

use std::path::{Path as FsPath, PathBuf};

use flux_arena::{Idx, LayeredArena};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::ast::{self, AstChildren, AstNode};
use flux_syntax::{NodeId, SyntaxElement, SyntaxKind, SyntaxNode, T, TextRange};

use std::sync::Arc;

use crate::Diagnostic;
use crate::def_map::DefMap;
use crate::macros::{self, MacroDef, MacroId, MacroScope, MacroScopeId, RECURSION_LIMIT};
use crate::source::{FileId, SourceDb, SourceFile};

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
    /// The files read, for `line!`, `column!` and `file!`.
    pub files: rustc_hash::FxHashMap<FileId, Arc<SourceFile>>,
    /// The file each expansion of a macro call in item position is part
    /// of, by the file its items are in.
    pub expansion_files: rustc_hash::FxHashMap<FileId, FileId>,
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
    /// flux's own `core` or `alloc`, which may define methods on
    /// primitive types.
    pub is_core: bool,
    /// The crates its `extern crate` declarations name (`alloc`), which
    /// it may name by path as it names `core`.
    pub externs: Vec<Symbol>,
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
    /// `#[rustc_specialization]` in `core`: an associated constant
    /// standing in for what rustc's `core` learns by specialization
    /// (whether an iterator is `TrustedLen`, ...), which as there does
    /// not keep its trait off trait objects.
    pub specialization: bool,
    /// An anonymous constant (`[u8; N + 1]`): see [`AnonConst`]. Its
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// `impl dyn Tr`: trait objects of a trait.
    Object(TraitId),
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
    /// `#[repr(C)]`: fields are laid out in declaration order, as C
    /// does, and an enum's discriminant is a C `int` at least.
    pub repr_c: bool,
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
    /// `#[track_caller]`: code generation passes it where it is called
    /// from, which `Location::caller` and its panics report.
    pub track_caller: bool,
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
    /// `= Self` in `trait Add<Rhs = Self>`, `= ()` in `enum
    /// ControlFlow<B, C = ()>`: traits', structs' and enums' parameters
    /// only.
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

    /// Embedded library items, frozen together for [`Core`](crate::Core).
    pub(crate) fn core(db: &mut SourceDb) -> ItemTree {
        let mut items = ItemTree::default();
        items.add_crate(db, FileId::CORE_ROOT, sym::core, true);
        // `alloc`, which only `core` comes before, and which may write
        // what `core` may: methods of primitive types, `lang` items.
        items.add_crate(db, FileId::alloc_root(), sym::alloc, true);
        // The facade shares definitions through reexports, but has no
        // permission to define primitive impls or compiler lang items.
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
        let from = self.counts();
        let mut collector = Collector {
            db,
            items: self,
            derives: Vec::new(),
            ctors: Vec::new(),
            block_counts: rustc_hash::FxHashMap::default(),
            fn_ids: rustc_hash::FxHashMap::default(),
            impl_ids: rustc_hash::FxHashMap::default(),
            macro_scope: None,
            expansion_depth: 0,
            expanding_types: false,
        };
        collector.collect_crate(root, name, is_core);
        collector.expand_derives();
        collector.make_ctors();
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

/// Reads crates' files and collects their items.
struct Collector<'a> {
    db: &'a mut SourceDb,
    items: &'a mut ItemTree,
    /// The `#[derive]`s of a crate's structs, made into `impl` blocks
    /// after its other items, as rustc expands them.
    derives: Vec<(Ctx, StructId, Derive, TextRange, NodeId)>,
    /// The crate's structs and enums, whose tuple constructors become
    /// functions after its other items.
    ctors: Vec<(Ctx, StructId, NodeId)>,
    /// How many blocks with items each function has had, for their
    /// modules' names.
    block_counts: rustc_hash::FxHashMap<Option<FunctionId>, u32>,
    /// The functions and `impl` blocks collected, by their nodes: what
    /// their anonymous constants are in.
    fn_ids: rustc_hash::FxHashMap<(FileId, NodeId), FunctionId>,
    impl_ids: rustc_hash::FxHashMap<(FileId, NodeId), ImplId>,
    /// The `macro_rules!` macros in textual scope where collecting is.
    macro_scope: Option<MacroScopeId>,
    /// How many macro expansions the items being collected are in.
    expansion_depth: u32,
    /// The macro calls in type position of the item being collected
    /// have been expanded.
    expanding_types: bool,
}

/// What the anonymous constants of an item see: the module, the `impl`
/// block (if `Self` may be used), the generic parameters (and whether
/// each is a const one), whether `Self` is generic, the function, and
/// the item's name, for rustc's names of them.
#[derive(Clone)]
struct AnonScope {
    module: ModuleId,
    owner: Option<Owner>,
    generics: Vec<(Symbol, bool)>,
    generic_self: bool,
    function: Option<FunctionId>,
    item: Symbol,
}

/// Where an anonymous constant is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AnonUse {
    /// An array type's length.
    Len,
    /// A repeat expression's count.
    Repeat,
    /// A const generic argument.
    Arg,
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
            externs: Vec::new(),
        });
        self.collect_file(module, root);
    }

    /// The items of a whole file, the contents of `module`.
    fn collect_file(&mut self, module: ModuleId, file: FileId) {
        let start = self.items.diagnostics.len();
        let shared = self.db.shared(file);
        self.items.files.insert(file, Arc::clone(&shared));
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
        let _file = TypesFile::enter(ctx.file);
        for &node in items {
            let start = self.items.diagnostics.len();
            let shared = self.db.shared(ctx.file);
            if item_enabled(shared.parse.tree().node(node), &mut self.items.diagnostics) {
                self.collect_item_expanding_types(ctx, node);
                self.collect_anon_consts(ctx, node);
            }
            self.tag(start, ctx.file);
        }
    }

    /// [`Collector::collect_item`], with the macro calls in type position
    /// in it expanded first, unless an item around it did that.
    fn collect_item_expanding_types(&mut self, ctx: Ctx, node: NodeId) {
        if self.expanding_types {
            self.collect_item(ctx, node);
            return;
        }
        let shared = self.db.shared(ctx.file);
        let syntax = shared.parse.tree().node(node);
        if !syntax
            .descendants()
            .any(|d| d.kind() == SyntaxKind::MACRO_TYPE)
        {
            self.collect_item(ctx, node);
            return;
        }
        with_macro_types(|| {
            let mut diagnostics = Vec::new();
            expand_types(syntax, &mut diagnostics, &mut |call, diagnostics| {
                self.expand_type_call(ctx, call, diagnostics)
            });
            self.items.diagnostics.extend(diagnostics);
            self.expanding_types = true;
            self.collect_item(ctx, node);
            self.expanding_types = false;
        });
    }

    /// Expands `call`, a macro call in type position.
    fn expand_type_call(
        &self,
        ctx: Ctx,
        call: SyntaxNode<'_>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<flux_syntax::Parse> {
        let path = call.children().find_map(ast::Path::cast)?;
        let def = match macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module) {
            Ok(macros::Resolved::Def(id)) => &self.items.macros[id],
            Ok(macros::Resolved::Local(_)) => unreachable!("no local macros here"),
            Err(d) => {
                diagnostics.push(d);
                return None;
            }
        };
        let site = macros::CallSite {
            items: self.items,
            krate: self.items.modules[ctx.module].krate,
            module: ctx.module,
            real_file: self.real_file(ctx.file),
        };
        let expansion = macros::expand(
            def,
            call,
            &site,
            flux_syntax::parser::Fragment::Type,
            diagnostics,
        )?;
        Some(expansion.parse)
    }

    fn collect_item(&mut self, ctx: Ctx, node: NodeId) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let item = ast::Item::cast(tree.node(node)).expect("items are ITEM nodes");
        match item {
            ast::Item::Fn(f) => {
                self.collect_fn(ctx, f, false, None);
            }
            ast::Item::Impl(imp) => {
                self.collect_impl(ctx, imp);
            }
            ast::Item::ExternBlock(block) => {
                self.collect_extern_block(ctx, block);
            }
            ast::Item::Struct(s) => {
                self.collect_struct(ctx, s);
            }
            ast::Item::Module(m) => {
                self.collect_module(ctx, m);
            }
            ast::Item::Use(u) => {
                self.collect_use(ctx, u);
            }
            ast::Item::Const(c) => {
                self.collect_const(ctx, c, None);
            }
            ast::Item::Enum(e) => {
                self.collect_enum(ctx, e);
            }
            ast::Item::ExternCrate(e) => {
                self.collect_extern_crate(ctx, e);
            }
            ast::Item::MacroCall(mc) => {
                self.collect_macro_call(ctx, mc);
            }
            ast::Item::Static(st) => {
                self.collect_static(ctx, st);
            }
            ast::Item::Trait(t) => {
                self.collect_trait(ctx, t);
            }
            ast::Item::TypeAlias(t) => {
                self.collect_type_alias(ctx, t);
            }
        }
    }

    /// The file `file` is part of: itself, or the file of the call a
    /// macro expansion's items are from.
    fn real_file(&self, file: FileId) -> FileId {
        self.items
            .expansion_files
            .get(&file)
            .copied()
            .unwrap_or(file)
    }

    /// Defines the macro `macro_rules! name { .. }` at `node`, in textual
    /// scope from here on.
    fn define_macro(&mut self, ctx: Ctx, node: SyntaxNode<'_>, export: bool, builtin: bool) {
        let krate = self.items.modules[ctx.module].krate;
        let file = self.real_file(ctx.file);
        let Some(def) =
            MacroDef::from_rules(node, krate, file, builtin, &mut self.items.diagnostics)
        else {
            return;
        };
        let name = def.name;
        let id = self.items.macros.alloc(def);
        self.macro_scope = Some(self.items.push_macro_scope(self.macro_scope, name, id));
        if export {
            self.items.exported_macros.insert((krate, name), id);
            if self.items.crates[krate.0 as usize].is_core {
                self.items.macro_prelude.insert(name, id);
            }
        }
    }

    /// A macro call in item position: a `macro_rules!` definition, or a
    /// call whose expansion's items are collected as if written here.
    fn collect_macro_call(&mut self, ctx: Ctx, mc: ast::MacroCall<'_>) {
        let is_core = self.items.is_core(ctx.module);
        let (mut export, mut builtin) = (false, false);
        for attr in mc.attrs() {
            match attr_name(attr) {
                Some("macro_export") => export = true,
                Some("rustc_builtin_macro") if is_core => builtin = true,
                _ => check_attr(attr, &mut self.items.diagnostics),
            }
        }
        let node = mc.syntax();
        if macros::is_macro_rules(node) {
            self.define_macro(ctx, node, export, builtin);
            return;
        }
        let Some(path) = mc.path() else {
            return;
        };
        let resolved = macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module);
        let def = match resolved {
            Ok(macros::Resolved::Def(id)) => id,
            Ok(macros::Resolved::Local(_)) => unreachable!("no local macros here"),
            Err(d) => {
                self.items.diagnostics.push(d);
                return;
            }
        };
        let def = self.items.macros[def].clone();
        if self.expansion_depth >= RECURSION_LIMIT {
            self.items.diagnostics.push(Diagnostic::new(
                node.text_range(),
                format!("recursion limit reached while expanding `{}!`", def.name),
            ));
            return;
        }
        let real = self.real_file(ctx.file);
        let site = macros::CallSite {
            items: self.items,
            krate: self.items.modules[ctx.module].krate,
            module: ctx.module,
            real_file: real,
        };
        let mut diagnostics = Vec::new();
        let expansion = macros::expand(
            &def,
            node,
            &site,
            flux_syntax::parser::Fragment::Items,
            &mut diagnostics,
        );
        self.items.diagnostics.extend(diagnostics);
        let Some(expansion) = expansion else {
            return;
        };
        let file = self.db.add_expansion(expansion.parse, real);
        self.items.expansion_files.insert(file, real);
        let shared = self.db.shared(file);
        let items: Vec<NodeId> = shared
            .parse
            .tree()
            .root()
            .children()
            .filter(|c| is_item_kind(c.kind()))
            .map(|c| c.id())
            .collect();
        self.expansion_depth += 1;
        let start = self.items.diagnostics.len();
        self.collect_items(
            Ctx {
                module: ctx.module,
                file,
            },
            &items,
        );
        self.tag(start, file);
        self.expansion_depth -= 1;
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
            Some(ty) => {
                let mut lowered = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::Unstable("type aliases");
                reject_impl_trait(&mut lowered, site, diagnostics);
                (lowered, ty.syntax().text_range())
            }
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
            lower_bounds(list, &mut bounds, None, Relaxed::Unsupported, diagnostics);
        }
        for bound in &mut bounds {
            reject_impl_trait_in_path(&mut bound.path, IN_BOUNDS, diagnostics);
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
                    None => {
                        let mut lowered = lower_type(Some(ty), diagnostics);
                        let site = ImplTraitSite::Unstable("associated types");
                        reject_impl_trait(&mut lowered, site, diagnostics);
                        (lowered, range)
                    }
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
            lower_bounds(
                list,
                &mut supertraits,
                None,
                Relaxed::Supertrait,
                diagnostics,
            );
        }
        for bound in &mut supertraits {
            reject_impl_trait_in_path(&mut bound.path, IN_BOUNDS, diagnostics);
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
            if !item_enabled(item.syntax(), &mut self.items.diagnostics) {
                continue;
            }
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
        // Macros defined in a module are in scope until its end, or
        // after it too with `#[macro_use]`.
        let mut macro_use = false;
        for attr in m.attrs() {
            if attr_name(attr) == Some("macro_use") {
                macro_use = true;
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let outer_scope = self.macro_scope;
        self.collect_module_inner(ctx, m);
        if !macro_use {
            self.macro_scope = outer_scope;
        }
    }

    fn collect_module_inner(&mut self, ctx: Ctx, m: ast::Module<'_>) {
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
        // `use m;` of a `macro_rules!` macro in textual scope makes it
        // nameable by path, as `crate::m!` and so on.
        if let (UseKind::Named(name), PathAnchor::Plain, [only]) =
            (kind, path.anchor, &path.segments[..])
            && let Some(def) = self.items.textual_macro(self.macro_scope, *only)
        {
            self.items.macro_imports.push(MacroImport {
                module: ctx.module,
                name,
                def,
                vis,
                file: ctx.file,
                range,
            });
            return;
        }
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
                Some(mut bound) => {
                    let site = ImplTraitSite::NotAllowed("traits");
                    reject_impl_trait_in_path(&mut bound.path, site, diagnostics);
                    Some(bound)
                }
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
        let mut self_ty = lower_type(imp.self_ty(), diagnostics);
        reject_impl_trait(
            &mut self_ty,
            ImplTraitSite::NotAllowed("impl headers"),
            diagnostics,
        );
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
        self.impl_ids.insert((ctx.file, imp.syntax().id()), id);
        let Some(list) = imp.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        for item in list.items() {
            if !item_enabled(item.syntax(), &mut self.items.diagnostics) {
                continue;
            }
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
        let mut track_caller = false;
        let mut do_not_const_check = false;
        let mut lang = None;
        for attr in f.attrs() {
            if attr_name(attr) == Some("track_caller")
                && attr.meta().is_some_and(|m| m.path().is_some())
            {
                track_caller = true;
                continue;
            }
            if is_core && attr_name(attr) == Some("rustc_inherit_overflow_checks") {
                inherits_overflow_checks = true;
            } else if is_core && attr_name(attr) == Some("rustc_const_unstable") {
                const_unstable = true;
            } else if is_core && attr_name(attr) == Some("rustc_allow_const_fn_unstable") {
                precise_live_drops = true;
            } else if is_core && attr_name(attr) == Some("rustc_intrinsic") {
                intrinsic = true;
            } else if is_core && attr_name(attr) == Some("rustc_do_not_const_check") {
                do_not_const_check = true;
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
        // Where the type parameters `impl Trait` parameter types make
        // start: after its owner's and its own.
        let owner_params = match owner {
            Some(Owner::Trait(t)) => 1 + self.items.traits[t].generics.params.len(),
            Some(Owner::Impl(imp)) => self.items.impls[imp].generics.params.len(),
            None => 0,
        };
        let in_trait_or_trait_impl = match owner {
            Some(Owner::Trait(_)) => true,
            Some(Owner::Impl(imp)) => self.items.impls[imp].trait_ref.is_some(),
            None => false,
        };
        let diagnostics = &mut self.items.diagnostics;
        let mut generics = collect_generics(
            f.generic_param_list(),
            f.where_clause(),
            Defaults::No,
            diagnostics,
        );
        if in_extern_block && !generics.is_empty() {
            let what = if generics.params.iter().all(GenericParam::is_const) {
                "const"
            } else {
                "type"
            };
            diagnostics.push(Diagnostic::new(
                TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
                format!("foreign items may not have {what} parameters"),
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
        let mut impl_params = ImplParams {
            first: (owner_params + generics.params.len()) as u32,
            ..ImplParams::default()
        };
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
                let mut ty = lower_type(param.ty(), diagnostics);
                if in_extern_block {
                    let site = ImplTraitSite::NotAllowed("`extern fn` parameters");
                    reject_impl_trait(&mut ty, site, diagnostics);
                } else {
                    desugar_impl_params(&mut ty, &mut impl_params, diagnostics);
                }
                params.push(Param {
                    ty,
                    range: param
                        .ty()
                        .map_or(param.syntax().text_range(), |t| t.syntax().text_range()),
                    whole: param.syntax().text_range(),
                    name,
                });
            }
        }
        if !impl_params.params.is_empty() {
            fn extend<T>(list: &mut Box<[T]>, more: Vec<T>) {
                let mut all = std::mem::take(list).into_vec();
                all.extend(more);
                *list = all.into();
            }
            extend(&mut generics.params, impl_params.params);
            extend(&mut generics.predicates, impl_params.predicates);
            extend(&mut generics.type_outlives, impl_params.type_outlives);
        }
        let (mut ret, ret_range) = match f.ret_type() {
            Some(ret) => (
                lower_type(ret.ty(), diagnostics),
                ret.ty()
                    .map_or(ret.syntax().text_range(), |t| t.syntax().text_range()),
            ),
            None => (TypeRef::unit(), name.syntax().text_range()),
        };
        let mut opaques = Vec::new();
        if in_extern_block {
            let site = ImplTraitSite::NotAllowed("`extern fn` return types");
            reject_impl_trait(&mut ret, site, diagnostics);
        } else if in_trait_or_trait_impl {
            let site = ImplTraitSite::Unsupported(
                "`impl Trait` types in the return types of trait methods",
            );
            reject_impl_trait(&mut ret, site, diagnostics);
        } else {
            collect_opaques(&mut ret, &mut opaques, diagnostics);
        }
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
            track_caller,
            do_not_const_check,
            is_variadic,
            owner,
            has_self,
            has_body: has_body && !in_extern_block,
            node: f.syntax().id(),
            name_range: name.syntax().text_range(),
            derived: None,
            ctor: None,
            opaques: opaques.into(),
            macro_scope: self.macro_scope,
        });
        self.fn_ids.insert((ctx.file, f.syntax().id()), id);
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
    /// blocks inside it. A `macro_rules!` statement defines a macro for
    /// the items and blocks after it.
    fn collect_block(&mut self, ctx: Ctx, node: NodeId, function: Option<FunctionId>) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let block = tree.node(node);
        let has_items = block.children().any(|c| is_item_kind(c.kind()));
        let has_macros = block.children().any(|c| {
            c.first_child()
                .is_some_and(|m| m.kind() == SyntaxKind::MACRO_EXPR && macros::is_macro_rules(m))
        });
        let ctx = if has_items {
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
            Ctx {
                module,
                file: ctx.file,
            }
        } else {
            ctx
        };
        let outer_scope = self.macro_scope;
        // The blocks inside statements, with the macros in scope there,
        // collected after the block's items.
        let mut inner = Vec::new();
        for child in block.children() {
            if is_item_kind(child.kind()) {
                let start = self.items.diagnostics.len();
                if item_enabled(child, &mut self.items.diagnostics) {
                    self.collect_item_expanding_types(ctx, child.id());
                }
                self.tag(start, ctx.file);
                continue;
            }
            if has_macros
                && let Some(mac) = child.first_child()
                && mac.kind() == SyntaxKind::MACRO_EXPR
                && macros::is_macro_rules(mac)
            {
                // Lowering the body reports its errors.
                let start = self.items.diagnostics.len();
                self.define_macro(ctx, mac, false, false);
                self.items.diagnostics.truncate(start);
            }
            let mut blocks = Vec::new();
            outer_blocks(child, &mut blocks);
            inner.extend(blocks.into_iter().map(|b| (b, self.macro_scope)));
        }
        for (block, scope) in inner {
            self.macro_scope = scope;
            self.collect_block(ctx, block, function);
        }
        self.macro_scope = outer_scope;
    }

    /// Allocates the anonymous constants of item `node` (array lengths,
    /// repeat counts and const generic arguments written as expressions
    /// other than literals and paths), its associated items' and its
    /// body's, but not those of items inside it, which are collected on
    /// their own. Uses of generic parameters in them are rustc's errors,
    /// and they get none.
    fn collect_anon_consts(&mut self, ctx: Ctx, node: NodeId) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let root = tree.node(node);
        // Most items have none: they are found without a walk.
        if !root.descendants().any(|n| {
            matches!(
                n.kind(),
                SyntaxKind::ARRAY_TYPE | SyntaxKind::ARRAY_EXPR | SyntaxKind::CONST_ARG
            )
        }) {
            return;
        }
        let mut scope = AnonScope {
            module: ctx.module,
            owner: None,
            generics: Vec::new(),
            generic_self: false,
            function: None,
            item: kw::Empty,
        };
        let mut counts = rustc_hash::FxHashMap::default();
        self.anon_walk(ctx.file, root, &mut scope, &mut counts);
    }

    fn anon_walk(
        &mut self,
        file: FileId,
        node: SyntaxNode<'_>,
        scope: &mut AnonScope,
        counts: &mut rustc_hash::FxHashMap<Symbol, u32>,
    ) {
        let kind = node.kind();
        if is_item_kind(kind) && !item_enabled(node, &mut Vec::new()) {
            return;
        }
        let outer = scope.clone();
        if is_item_kind(kind) {
            if let Some(list) = node
                .children()
                .find(|c| c.kind() == SyntaxKind::GENERIC_PARAM_LIST)
                .and_then(ast::GenericParamList::cast)
            {
                for param in list.generic_params() {
                    let (name, is_const) = match param {
                        ast::GenericParam::TypeParam(p) => (p.name(), false),
                        ast::GenericParam::ConstParam(p) => (p.name(), true),
                        ast::GenericParam::LifetimeParam(_) => {
                            if kind == SyntaxKind::IMPL {
                                scope.generic_self = true;
                            }
                            continue;
                        }
                    };
                    if let Some(name) = name {
                        scope.generics.push((Symbol::intern(name.text()), is_const));
                        if matches!(kind, SyntaxKind::IMPL | SyntaxKind::TRAIT) {
                            scope.generic_self = true;
                        }
                    }
                }
            }
            if let Some(name) = node
                .children()
                .find(|c| c.kind() == SyntaxKind::NAME)
                .map(|n| Symbol::intern(n.text().trim()))
                && scope.item == kw::Empty
            {
                scope.item = name;
            }
            match kind {
                SyntaxKind::FN => {
                    scope.function = self.fn_ids.get(&(file, node.id())).copied();
                    if let Some(f) = scope.function {
                        scope.item = self.items.functions[f].name;
                    }
                }
                SyntaxKind::IMPL => {
                    scope.owner = self
                        .impl_ids
                        .get(&(file, node.id()))
                        .filter(|_| !scope.generic_self)
                        .map(|&imp| Owner::Impl(imp));
                }
                SyntaxKind::TRAIT => scope.generic_self = true,
                _ => {}
            }
        }
        match kind {
            SyntaxKind::BLOCK_EXPR => {
                if let Some(&module) = self.items.block_modules.get(&(file, node.id())) {
                    scope.module = module;
                }
            }
            SyntaxKind::ARRAY_TYPE => {
                if let Some(expr) = ast::ArrayType::cast(node).and_then(|a| a.expr()) {
                    self.anon_const(file, expr, AnonUse::Len, scope, counts);
                }
            }
            SyntaxKind::ARRAY_EXPR => {
                if let Some(array) = ast::ArrayExpr::cast(node)
                    && array.semicolon_token().is_some()
                    && let Some(expr) = array.exprs().nth(1)
                {
                    self.anon_const(file, expr, AnonUse::Repeat, scope, counts);
                }
            }
            SyntaxKind::CONST_ARG => {
                if let Some(expr) = ast::ConstArg::cast(node).and_then(|a| a.expr()) {
                    self.anon_const(file, expr, AnonUse::Arg, scope, counts);
                }
            }
            _ => {}
        }
        // Associated items and those of `extern` blocks are this item's;
        // others are collected on their own.
        let own_items = matches!(
            kind,
            SyntaxKind::ASSOC_ITEM_LIST | SyntaxKind::EXTERN_ITEM_LIST
        );
        for child in node.children() {
            if is_item_kind(child.kind()) && !own_items {
                continue;
            }
            self.anon_walk(file, child, scope, counts);
        }
        *scope = outer;
    }

    /// The anonymous constant of `expr`, used as `what`, if it is one
    /// and does not use generic parameters (reported).
    fn anon_const(
        &mut self,
        file: FileId,
        expr: ast::Expr<'_>,
        what: AnonUse,
        scope: &AnonScope,
        counts: &mut rustc_hash::FxHashMap<Symbol, u32>,
    ) {
        let expr = const_expr_root(expr);
        if !is_anon_const(expr) {
            return;
        }
        let range = expr.syntax().text_range();
        let count = counts.entry(scope.item).or_default();
        let index = *count;
        *count += 1;
        let diagnostics = &mut self.items.diagnostics;
        let mut generic = false;
        let mut depends = false;
        for token in expr.syntax().tokens() {
            match token.kind() {
                SyntaxKind::SELF_TYPE_KW if scope.generic_self => {
                    diagnostics.push(Diagnostic::new(
                        token.text_range(),
                        "generic `Self` types are currently not permitted in anonymous constants",
                    ));
                    generic = true;
                }
                SyntaxKind::IDENT => {
                    let Some(&(_, is_const)) = scope
                        .generics
                        .iter()
                        .rev()
                        .find(|(n, _)| n.as_str() == token.text())
                    else {
                        continue;
                    };
                    generic = true;
                    // A repeat count may name type parameters, as rustc
                    // allows for compatibility; its value then depends
                    // on them.
                    if what == AnonUse::Repeat && !is_const {
                        depends = true;
                        continue;
                    }
                    diagnostics.push(Diagnostic::new(
                        token.text_range(),
                        "generic parameters may not be used in const operations",
                    ));
                }
                _ => {}
            }
        }
        if depends {
            diagnostics.push(Diagnostic::new(
                range,
                "constant expression depends on a generic parameter",
            ));
        }
        if generic {
            return;
        }
        let ty = match what {
            AnonUse::Arg => TypeRef::Infer,
            AnonUse::Len | AnonUse::Repeat => TypeRef::Path(Path::single(sym::usize)),
        };
        let konst = self.items.consts.alloc(Const {
            name: None,
            ty,
            ty_range: range,
            loc: ItemLoc {
                module: scope.module,
                file,
                vis: Visibility::Public,
            },
            owner: scope.owner,
            node: expr.syntax().id(),
            has_body: true,
            def_range: range,
            item_range: range,
            discriminant: None,
            is_static: None,
            specialization: false,
            anon: Some(AnonConst {
                function: scope.function,
                item: scope.item,
                index,
                is_arg: what == AnonUse::Arg,
            }),
            macro_scope: self.macro_scope,
        });
        self.items
            .anon_consts
            .insert((file, expr.syntax().id()), konst);
    }

    fn collect_const(
        &mut self,
        ctx: Ctx,
        c: ast::Const<'_>,
        owner: Option<Owner>,
    ) -> Option<ConstId> {
        let is_core = self.items.is_core(ctx.module);
        let mut specialization = false;
        for attr in c.attrs() {
            if is_core && attr_name(attr) == Some("rustc_specialization") {
                specialization = true;
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.item_visibility(ctx, c.visibility(), owner);
        let name = c.name();
        let name_end = name
            .map(|n| n.syntax().text_range().end())
            .or_else(|| c.underscore_token().map(|t| t.text_range().end()))?;
        let diagnostics = &mut self.items.diagnostics;
        let ty = match c.ty() {
            Some(ty) => {
                let mut ty = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::NotAllowed("const types");
                reject_impl_trait(&mut ty, site, diagnostics);
                ty
            }
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
            anon: None,
            is_static: None,
            specialization,
            macro_scope: self.macro_scope,
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
            Some(ty) => {
                let mut ty = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::NotAllowed("static types");
                reject_impl_trait(&mut ty, site, diagnostics);
                ty
            }
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
            anon: None,
            is_static: Some(st.mut_token().is_some()),
            specialization: false,
            macro_scope: self.macro_scope,
        });
        if let Some(body) = st.body() {
            self.collect_body_items(ctx, body.syntax().id(), None);
        }
    }

    /// `extern crate alloc;`: `alloc` may be named by path, as rustc's
    /// `alloc` may once declared. `core` always may.
    fn collect_extern_crate(&mut self, ctx: Ctx, e: ast::ExternCrate<'_>) {
        let range = e.syntax().text_range();
        let Some(name) = e.name_ref() else {
            return;
        };
        let name = Symbol::intern(name.text());
        let krate = self.items.modules[ctx.module].krate;
        let diagnostics = &mut self.items.diagnostics;
        if e.rename().is_some() {
            unsupported(diagnostics, range, "renamed `extern crate` declarations");
        } else if name == sym::alloc {
            self.items.crates[krate.0 as usize].externs.push(name);
        } else if name != sym::core && name != sym::std {
            diagnostics.push(Diagnostic::new(
                range,
                format!("can't find crate for `{name}`"),
            ));
        }
    }

    fn collect_struct(&mut self, ctx: Ctx, s: ast::Struct<'_>) {
        let mut derives = Vec::new();
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        let mut hints = Vec::new();
        for attr in s.attrs() {
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
        let repr_c = struct_repr(&hints, &mut self.items.diagnostics);
        let name_range = name.syntax().text_range();
        let id = self.items.structs.alloc(Struct {
            name: Symbol::intern(name.text()),
            generics,
            kind,
            fields: fields.into(),
            variants: None,
            repr: None,
            repr_c,
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
        self.ctors.push((ctx, id, s.syntax().id()));
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
                let mut ty = lower_type(field.ty(), diagnostics);
                reject_impl_trait(&mut ty, FIELD_TYPES, diagnostics);
                fields.push(Field {
                    name: field_name,
                    ty,
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
                let diagnostics = &mut self.items.diagnostics;
                let mut ty = lower_type(field.ty(), diagnostics);
                reject_impl_trait(&mut ty, FIELD_TYPES, diagnostics);
                fields.push(Field {
                    name: Symbol::intern(&i.to_string()),
                    ty,
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
        // `#[default]` on variants: the variant's index, the attribute's
        // range and whether it has a value, and the name's range.
        let mut defaults: Vec<(u32, TextRange, bool, TextRange)> = Vec::new();
        for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
            let mut marked = Vec::new();
            for attr in v.attrs() {
                if attr_name(attr) == Some("default") {
                    let meta = attr.meta();
                    let has_value = meta.is_some_and(|m| {
                        m.token_tree().is_some()
                            || m.syntax()
                                .children_with_tokens()
                                .any(|t| t.kind() == SyntaxKind::EQ)
                    });
                    let name = meta
                        .and_then(|m| m.path())
                        .map_or(attr.syntax().text_range(), |p| p.syntax().text_range());
                    marked.push((attr.syntax().text_range(), has_value, name));
                } else {
                    check_attr(attr, &mut self.items.diagnostics);
                }
            }
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
                    specialization: false,
                    anon: None,
                    macro_scope: self.macro_scope,
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
            let index = variants.len() as u32;
            defaults.extend(marked.into_iter().map(|(a, v, n)| (index, a, v, n)));
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
            repr_c: hints.iter().any(|(h, _)| h.as_str() == "C"),
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
        let derives_default = derives.iter().find(|(d, _)| *d == Derive::Default);
        let mut default_ok = true;
        match derives_default {
            // `#[default]` is the derive's helper attribute: without it,
            // an unknown attribute.
            None => {
                for &(_, _, _, name) in &defaults {
                    self.items.diagnostics.push(Diagnostic::new(
                        name,
                        "cannot find attribute `default` in this scope",
                    ));
                }
            }
            Some(&(_, at)) => {
                let variants = self.items.structs[id]
                    .variants
                    .as_deref()
                    .unwrap_or_default();
                for &(_, attr, has_value, _) in &defaults {
                    if has_value {
                        self.items.diagnostics.push(Diagnostic::new(
                            attr,
                            "`#[default]` attribute does not accept a value",
                        ));
                    }
                }
                match defaults.as_slice() {
                    [] => {
                        self.items.diagnostics.push(Diagnostic::new(
                            at,
                            "`#[derive(Default)]` on enum with no `#[default]`",
                        ));
                        default_ok = false;
                    }
                    &[(v, ..)] => {
                        let variant = &variants[v as usize];
                        if variant.kind == StructKind::Unit {
                            self.items.default_variants.insert(id, v);
                        } else {
                            self.items.diagnostics.push(Diagnostic::new(
                                variant.name_range,
                                "the `#[default]` attribute may only be used on unit enum variants",
                            ));
                            default_ok = false;
                        }
                    }
                    _ => {
                        self.items
                            .diagnostics
                            .push(Diagnostic::new(at, "multiple declared defaults"));
                        default_ok = false;
                    }
                }
            }
        }
        for (derive, range) in derives {
            if derive == Derive::Default && !default_ok {
                continue;
            }
            self.derives.push((ctx, id, derive, range, e.syntax().id()));
        }
        self.ctors.push((ctx, id, e.syntax().id()));
    }

    /// A function for each tuple struct's and tuple variant's
    /// constructor, with the struct's generics, its fields as parameters
    /// and the struct as its type: what the constructor's path is as a
    /// value (`.map(Some)`), as rustc's constructors are functions.
    fn make_ctors(&mut self) {
        let ctors = std::mem::take(&mut self.ctors);
        for (ctx, strukt, node) in ctors {
            let s = &self.items.structs[strukt];
            let variants: Vec<(u32, Symbol, TextRange, Box<[Field]>)> = match &s.variants {
                None if s.kind == StructKind::Tuple => {
                    vec![(0, s.name, s.name_range, s.fields.clone())]
                }
                None => Vec::new(),
                Some(variants) => variants
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.kind == StructKind::Tuple)
                    .map(|(i, v)| (i as u32, v.name, v.name_range, v.fields.clone()))
                    .collect(),
            };
            if variants.is_empty() {
                continue;
            }
            let generics = Generics {
                params: s
                    .generics
                    .params
                    .iter()
                    .map(|p| GenericParam {
                        default: None,
                        ..p.clone()
                    })
                    .collect(),
                ..s.generics.clone()
            };
            let mut self_path = Path::single(s.name);
            self_path.ranges = Box::new([s.name_range]);
            if !generics.params.is_empty() || !generics.lifetimes.is_empty() {
                self_path.args = Box::new([Some(GenericArgs {
                    lifetimes: generics
                        .lifetimes
                        .iter()
                        .map(|l| LifetimeRef {
                            name: l.name,
                            range: l.range,
                        })
                        .collect(),
                    types: generics
                        .params
                        .iter()
                        .map(|p| TypeRef::Path(Path::single(p.name)))
                        .collect(),
                    ranges: generics.params.iter().map(|p| p.range).collect(),
                    bindings: Box::default(),
                    range: s.generics.range,
                    parenthesized: false,
                })]);
            }
            for (v, name, range, fields) in variants {
                let params: Box<[Param]> = fields
                    .iter()
                    .map(|f| Param {
                        ty: f.ty.clone(),
                        name: None,
                        range: f.range,
                        whole: f.def_range,
                    })
                    .collect();
                let function = self.items.functions.alloc(Function {
                    name,
                    generics: generics.clone(),
                    params,
                    ret: TypeRef::Path(self_path.clone()),
                    ret_range: range,
                    loc: ItemLoc {
                        module: ctx.module,
                        file: ctx.file,
                        vis: Visibility::Public,
                    },
                    is_unsafe: false,
                    is_extern: false,
                    is_const: true,
                    inherits_overflow_checks: false,
                    const_unstable: false,
                    precise_live_drops: false,
                    intrinsic: false,
                    is_variadic: false,
                    track_caller: false,
                    do_not_const_check: false,
                    owner: None,
                    has_self: false,
                    has_body: true,
                    node,
                    name_range: range,
                    def_range: range,
                    sig_range: range,
                    item_range: range,
                    param_list: range,
                    derived: None,
                    ctor: Some((strukt, v)),
                    opaques: Box::default(),
                    macro_scope: None,
                });
                self.items.ctor_fns.insert((strukt, v), function);
            }
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
            Derive::Default => ("default", "Default"),
            Derive::Debug => ("fmt", "Debug"),
            Derive::Hash => ("hash", "Hash"),
        };
        // rustc's `Default` for an enum bounds no parameters: its
        // default variant has no fields.
        let bounds_params = !(derive == Derive::Default && s.is_enum());
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
                synthetic: false,
                kind: p.kind.clone(),
                maybe_unsized: p.maybe_unsized,
            })
            .collect();
        // As rustc expands it: each parameter's own bounds and the
        // trait's, then the struct's `where` clause.
        let inline = |p: &WherePredicate| p.ty_range.end() <= s.generics.range.end();
        let mut predicates = Vec::new();
        for param in params.iter().filter(|p| !p.is_const()) {
            let ty = TypeRef::Path(Path::single(param.name));
            predicates.extend(
                s.generics
                    .predicates
                    .iter()
                    .filter(|p| inline(p) && p.ty == ty)
                    .cloned(),
            );
            if bounds_params {
                predicates.push(WherePredicate {
                    ty,
                    ty_range: param.range,
                    bound: bound.clone(),
                });
            }
        }
        predicates.extend(s.generics.predicates.iter().filter(|p| !inline(p)).cloned());
        // And the trait for each field type's associated types of the
        // parameters (`T::Out`), which a bound on `T` cannot give.
        let mut projections = Vec::new();
        let variant_fields = s.variants.iter().flatten().flat_map(|v| v.fields.iter());
        for field in s.fields.iter().chain(variant_fields) {
            if bounds_params {
                param_projections(&field.ty, &params, &mut projections);
            }
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
        // `hash<__H: Hasher>`'s parameter.
        let mut method_generics = Generics::default();
        let (method, params, ret) = match derive {
            Derive::Copy => return,
            // `hash<__H: Hasher>(&self, state: &mut __H)`
            Derive::Hash => {
                let h = Symbol::intern("__H");
                method_generics = Generics {
                    params: Box::new([GenericParam {
                        name: h,
                        range,
                        default: None,
                        synthetic: false,
                        kind: ParamKind::Type,
                        maybe_unsized: false,
                    }]),
                    predicates: Box::new([WherePredicate {
                        ty: TypeRef::Path(Path::single(h)),
                        ty_range: range,
                        bound: Bound {
                            path: core_path("hash", "Hasher", in_core),
                            range,
                        },
                    }]),
                    ..Generics::default()
                };
                let state = Param {
                    ty: TypeRef::Ref {
                        lifetime: None,
                        amp: range,
                        mutable: true,
                        pointee: Box::new(TypeRef::Path(Path::single(h))),
                    },
                    name: Some(Symbol::intern("state")),
                    range,
                    whole: range,
                };
                ("hash", vec![self_ref(), state], TypeRef::unit())
            }
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
                // On the last segment: `::core::option::Option`, or
                // `crate::option::Option` in `core`.
                let leading = std::iter::repeat_n(None, option.segments.len() - 1);
                option.args = leading
                    .chain([Some(GenericArgs {
                        lifetimes: Box::default(),
                        types: Box::new([TypeRef::Path(core_path("cmp", "Ordering", in_core))]),
                        ranges: Box::new([range]),
                        bindings: Box::default(),
                        range,
                        parenthesized: false,
                    })])
                    .collect();
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
            Derive::Default => ("default", Vec::new(), self_ty()),
            // `fmt(&self, f: &mut Formatter<'_>) -> fmt::Result`
            Derive::Debug => {
                let formatter = Param {
                    ty: TypeRef::Ref {
                        lifetime: None,
                        amp: range,
                        mutable: true,
                        pointee: Box::new(TypeRef::Path(core_path("fmt", "Formatter", in_core))),
                    },
                    name: Some(Symbol::intern("f")),
                    range,
                    whole: range,
                };
                (
                    "fmt",
                    vec![self_ref(), formatter],
                    TypeRef::Path(core_path("fmt", "Result", in_core)),
                )
            }
        };
        let has_self = !params.is_empty();
        let function = self.items.functions.alloc(Function {
            name: Symbol::intern(method),
            generics: method_generics,
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
            track_caller: false,
            do_not_const_check: false,
            is_variadic: false,
            owner: Some(Owner::Impl(id)),
            has_self,
            has_body: true,
            node,
            name_range: range,
            def_range: range,
            sig_range: range,
            item_range: range,
            param_list: range,
            derived: Some((strukt, if copies { Derive::Copy } else { derive })),
            ctor: None,
            opaques: Box::default(),
            macro_scope: None,
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
            if !item_enabled(item.syntax(), &mut self.items.diagnostics) {
                continue;
            }
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

/// Evaluates every outer `cfg` on an item, conjunctively, before any
/// expansion, name collection or lowering. Even a false predicate does
/// not suppress validation of the item's other `cfg` attributes.
fn item_enabled(node: SyntaxNode<'_>, diagnostics: &mut Vec<Diagnostic>) -> bool {
    let mut enabled = true;
    for attr in node.children().filter_map(ast::Attr::cast) {
        if attr.bang_token().is_some()
            || attr_name(attr) != Some("cfg")
            || attr.meta().is_some_and(|m| m.unsafe_token().is_some())
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
                crate::cfg::eval(range, inner)
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
    enabled
}

/// Attributes that cannot change what a program means are allowed and
/// ignored; other attributes are reported unless already handled by
/// item collection (outer `cfg`).
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
    if name == Some("cfg")
        && attr.bang_token().is_none()
        && attr.meta().is_some_and(|m| m.unsafe_token().is_none())
        && attr
            .syntax()
            .parent()
            .is_some_and(|p| is_item_kind(p.kind()))
    {
        // Evaluated before item collection, including validation.
        return;
    }
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
            ast::GenericArg::ConstArg(arg) => {
                types.push(TypeRef::Const(lower_const_arg(
                    arg.expr(),
                    range,
                    diagnostics,
                )));
                ranges.push(range);
                continue;
            }
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
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter() {
                for args in b.path.args.iter().flatten() {
                    for arg in args.types.iter() {
                        param_projections(arg, params, out);
                    }
                    for binding in args.bindings.iter() {
                        param_projections(&binding.ty, params, out);
                    }
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

/// Whether `hints`, a struct's `repr`s, say `C`; the hints flux does
/// not support are reported.
fn struct_repr(hints: &[(Symbol, TextRange)], diagnostics: &mut Vec<Diagnostic>) -> bool {
    let mut c = false;
    for &(hint, range) in hints {
        match hint.as_str() {
            "C" => c = true,
            "Rust" => {}
            name if REPR_INTS.contains(&name) => diagnostics.push(Diagnostic::new(
                range,
                "attribute should be applied to an enum",
            )),
            name => diagnostics.push(Diagnostic::new(
                range,
                format!("the `repr({name})` attribute is not supported yet"),
            )),
        }
    }
    c
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
    Default,
    Debug,
    Hash,
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
    Some(Bound { path, range })
}

/// A lifetime as written.
pub(crate) fn lower_lifetime(lifetime: ast::Lifetime<'_>) -> LifetimeRef {
    LifetimeRef {
        name: Symbol::intern(lifetime.text()),
        range: lifetime.syntax().text_range(),
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
        } else if bound.l_paren_token().is_some() {
            "parenthesized bounds"
        } else {
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            // `for<'a> Fn(&'a T) -> &'a U` is `Fn(&T) -> &U`, which flux
            // supports; other higher-ranked bounds it does not yet.
            let binder = bound
                .for_token()
                .map(|_| binder_names(bound.generic_param_list(), diagnostics));
            if let Some(mut path) = lower_trait_path(path, diagnostics) {
                match binder {
                    Some(Some(names)) if elide_fn_binder(&mut path, &names, diagnostics) => {}
                    Some(_) => {
                        unsupported(diagnostics, range, "higher-ranked bounds");
                        continue;
                    }
                    None => {}
                }
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
    let mut generics = collect_generics_as_written(list, where_clause, defaults, diagnostics);
    let bounds = ImplTraitSite::NotAllowed("bounds");
    for p in generics.predicates.iter_mut() {
        reject_impl_trait(&mut p.ty, bounds, diagnostics);
        reject_impl_trait_in_path(&mut p.bound.path, IN_BOUNDS, diagnostics);
    }
    for p in generics.type_outlives.iter_mut() {
        reject_impl_trait(&mut p.ty, bounds, diagnostics);
    }
    for p in generics.params.iter_mut() {
        if let Some((ty, _)) = &mut p.default {
            let site = ImplTraitSite::NotAllowed("generic parameter defaults");
            reject_impl_trait(ty, site, diagnostics);
        }
    }
    generics
}

fn collect_generics_as_written(
    list: Option<ast::GenericParamList<'_>>,
    where_clause: Option<ast::WhereClause<'_>>,
    defaults: Defaults,
    diagnostics: &mut Vec<Diagnostic>,
) -> Generics {
    let mut predicates = Vec::new();
    let mut outlives = Vec::new();
    let mut type_outlives = Vec::new();
    // `?Sized` bounds: on what type, and where each is written.
    let mut relaxed: Vec<(TypeRef, Vec<TextRange>)> = Vec::new();
    let where_predicates = |predicates: &mut Vec<WherePredicate>,
                            outlives: &mut Vec<OutlivesPredicate>,
                            type_outlives: &mut Vec<TypeOutlives>,
                            relaxed: &mut Vec<(TypeRef, Vec<TextRange>)>,
                            diagnostics: &mut Vec<Diagnostic>| {
        let Some(clause) = where_clause else {
            return;
        };
        for pred in clause.predicates() {
            let range = pred.syntax().text_range();
            // `where for<'a> F: Fn(&'a T) -> &'a U`, as the bound's own.
            let binder = pred
                .for_token()
                .map(|_| binder_names(pred.generic_param_list(), diagnostics));
            if let Some(None) = binder {
                unsupported(diagnostics, range, "higher-ranked bounds");
                continue;
            }
            let mut bounds = Vec::new();
            let mut lifetimes = Vec::new();
            if let Some(lifetime) = pred.lifetime() {
                // `'a: 'b + 'c`
                let longer = lower_lifetime(lifetime);
                if let Some(list) = pred.type_bound_list() {
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut lifetimes),
                        Relaxed::Unsupported,
                        diagnostics,
                    );
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
            let mut relaxed_here = Vec::new();
            if let Some(list) = pred.type_bound_list() {
                lower_bounds(
                    list,
                    &mut bounds,
                    Some(&mut lifetimes),
                    Relaxed::Collect(&mut relaxed_here),
                    diagnostics,
                );
            }
            if !relaxed_here.is_empty() {
                relaxed.push((ty.clone(), relaxed_here));
            }
            if let Some(Some(names)) = &binder
                && !bounds
                    .iter_mut()
                    .all(|b| elide_fn_binder(&mut b.path, names, diagnostics))
            {
                unsupported(diagnostics, range, "higher-ranked bounds");
                continue;
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
            &mut relaxed,
            diagnostics,
        );
        mark_relaxed(&mut [], relaxed, diagnostics);
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
        match param {
            ast::GenericParam::TypeParam(param) => {
                check_attrs(param.attrs(), diagnostics);
                let Some(name) = param.name() else {
                    continue;
                };
                if let Some(list) = param.type_bound_list() {
                    let mut bounds = Vec::new();
                    let mut bound_lifetimes = Vec::new();
                    let mut relaxed_here = Vec::new();
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut bound_lifetimes),
                        Relaxed::Collect(&mut relaxed_here),
                        diagnostics,
                    );
                    let ty = TypeRef::Path(Path::single(Symbol::intern(name.text())));
                    if !relaxed_here.is_empty() {
                        relaxed.push((ty.clone(), relaxed_here));
                    }
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
                        Defaults::Trait | Defaults::Struct => {
                            default = Some((lower_type(Some(ty), diagnostics), ty_range));
                        }
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
                    synthetic: false,
                    kind: ParamKind::Type,
                    maybe_unsized: false,
                });
                continue;
            }
            ast::GenericParam::ConstParam(param) => {
                check_attrs(param.attrs(), diagnostics);
                let Some(name) = param.name() else {
                    continue;
                };
                let name_range = name.syntax().text_range();
                let ty_range = param.ty().map_or(range, |t| t.syntax().text_range());
                let ty = lower_type(param.ty(), diagnostics);
                if let Some(default) = param.default_val() {
                    let at = default.syntax().text_range();
                    match defaults {
                        Defaults::No => diagnostics.push(Diagnostic::new(
                            at,
                            "defaults for generic parameters are not allowed here",
                        )),
                        _ => unsupported(diagnostics, at, "defaults for const parameters"),
                    }
                }
                let name = Symbol::intern(name.text());
                if params.iter().any(|p| p.name == name) {
                    diagnostics.push(Diagnostic::new(name_range, already_used(name)));
                    continue;
                }
                params.push(GenericParam {
                    name,
                    range: name_range,
                    default: None,
                    synthetic: false,
                    kind: ParamKind::Const { ty, ty_range },
                    maybe_unsized: false,
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
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut shorter),
                        Relaxed::Unsupported,
                        diagnostics,
                    );
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
        }
    }
    where_predicates(
        &mut predicates,
        &mut outlives,
        &mut type_outlives,
        &mut relaxed,
        diagnostics,
    );
    mark_relaxed(&mut params, relaxed, diagnostics);
    Generics {
        params: params.into(),
        lifetimes: lifetimes.into(),
        predicates: predicates.into(),
        outlives: outlives.into(),
        type_outlives: type_outlives.into(),
        range: list.syntax().text_range(),
    }
}

/// Marks the parameters `?Sized` bounds are on, which must be the
/// item's own type parameters, each relaxed once (E0203).
fn mark_relaxed(
    params: &mut [GenericParam],
    relaxed: Vec<(TypeRef, Vec<TextRange>)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: Vec<(Symbol, TextRange)> = Vec::new();
    for (ty, ranges) in relaxed {
        let param = match &ty {
            TypeRef::Path(path) => path
                .as_name()
                .and_then(|name| params.iter_mut().find(|p| p.name == name && !p.is_const())),
            _ => None,
        };
        let Some(param) = param else {
            for range in ranges {
                diagnostics.push(Diagnostic::new(
                    range,
                    "this relaxed bound is not permitted here",
                ));
            }
            continue;
        };
        param.maybe_unsized = true;
        for range in ranges {
            if let Some(&(_, first)) = seen.iter().find(|(n, _)| *n == param.name) {
                diagnostics.push(Diagnostic::new(first, "duplicate relaxed `Sized` bounds"));
            } else {
                seen.push((param.name, range));
            }
        }
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
        ast::Type::DynTraitType(d) => return lower_dyn(d, diagnostics),
        ast::Type::FnPtrType(ptr) => return lower_fn_ptr(ptr, diagnostics),
        ast::Type::ForType(_) => "higher-ranked types",
        ast::Type::ImplTraitType(it) => return lower_impl_trait(it, diagnostics),
        ast::Type::InferType(_) => return TypeRef::Infer,
        ast::Type::MacroType(m) => {
            if let Some(ty) = expanded_type(m.syntax()) {
                return ty;
            }
            "macros in this position"
        }
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

/// `dyn Tr + 'a`.
fn lower_dyn(d: ast::DynTraitType<'_>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let range = d.syntax().text_range();
    let mut bounds = Vec::new();
    let mut lifetime: Option<LifetimeRef> = None;
    for bound in d.type_bound_list().into_iter().flat_map(|l| l.bounds()) {
        let bound_range = bound.syntax().text_range();
        if let Some(l) = bound.lifetime() {
            if lifetime.is_some() {
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
        } else if bound.l_paren_token().is_some() {
            "parenthesized bounds"
        } else {
            let Some(path) = bound.path_type().and_then(|p| p.path()) else {
                continue;
            };
            if let Some(path) = lower_trait_path(path, diagnostics) {
                bounds.push(Bound {
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
        let what = if bound.question_token().is_some() {
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
                bounds.push(Bound {
                    path,
                    range: bound_range,
                });
                names.push(collapse(bound.syntax().text()));
            }
            continue;
        };
        unsupported(diagnostics, bound_range, what);
        return TypeRef::Error;
    }
    if bounds.is_empty() {
        diagnostics.push(Diagnostic::new(
            range,
            "at least one trait must be specified",
        ));
        return TypeRef::Error;
    }
    TypeRef::ImplTrait(Box::new(ImplTraitRef {
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
        let (p, r) = match (args.parenthesized, at) {
            (true, Nested::Site) => (Nested::FnParam, Nested::FnRet),
            _ => (at, at),
        };
        for t in args.types.iter_mut() {
            visit_impl_traits(t, p, f);
        }
        for b in args.bindings.iter_mut() {
            visit_impl_traits(&mut b.ty, r, f);
        }
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
        maybe_unsized: false,
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
        if is_item_kind(node.kind()) && !item_enabled(node, &mut Vec::new()) {
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
/// constant: neither an integer or `bool` literal nor a path.
pub(crate) fn is_anon_const(expr: ast::Expr<'_>) -> bool {
    match expr {
        ast::Expr::Literal(literal) => !matches!(
            literal.kind(),
            Some(ast::LiteralKind::Int | ast::LiteralKind::Bool(_) | ast::LiteralKind::Char)
        ),
        ast::Expr::PathExpr(_) => false,
        _ => true,
    }
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
                                if p.lifetime().is_some_and(|l| Symbol::intern(l.text()) == lifetime.name))
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
                names.push(lifetime.name);
            }
            _ => return None,
        }
    }
    Some(names)
}

/// Rewrites a `Fn`-family bound under a binder of one lifetime,
/// `for<'a> Fn(&'a T) -> &'a U`, as the elided `Fn(&T) -> &U` it means:
/// whether it could, as the lifetime is its parameters' only one and
/// appears only on references.
fn elide_fn_binder(path: &mut Path, names: &[Symbol], diagnostics: &mut Vec<Diagnostic>) -> bool {
    let [name] = names else {
        return names.is_empty();
    };
    let last = path.segments.len().saturating_sub(1);
    let Some(Some(args)) = path.args.get_mut(last) else {
        return false;
    };
    if !args.parenthesized {
        return false;
    }
    fn walk(ty: &mut TypeRef, name: Symbol, count: &mut u32, named: &mut bool) -> bool {
        match ty {
            TypeRef::Ref {
                lifetime, pointee, ..
            } => {
                match lifetime {
                    Some(l) if l.name == name => {
                        *lifetime = None;
                        *count += 1;
                        *named = true;
                    }
                    Some(_) => {}
                    None => *count += 1,
                }
                walk(pointee, name, count, named)
            }
            TypeRef::Tuple(list) => list.iter_mut().all(|t| walk(t, name, count, named)),
            TypeRef::Slice(t) | TypeRef::Ptr { pointee: t, .. } => walk(t, name, count, named),
            TypeRef::Array { elem, .. } => walk(elem, name, count, named),
            TypeRef::Path(path) => path.args.iter().flatten().all(|a| {
                a.lifetimes.iter().all(|l| l.name != name)
                    && a.types.iter().all(|t| {
                        matches!(t, TypeRef::Path(_)) || {
                            let mut inner = 0;
                            let mut t = t.clone();
                            walk(&mut t, name, &mut inner, &mut false) && inner == 0
                        }
                    })
            }),
            _ => true,
        }
    }
    let mut inputs = 0;
    let mut named_input = false;
    let ok_inputs = args
        .types
        .iter_mut()
        .all(|t| walk(t, *name, &mut inputs, &mut named_input));
    let ok_outputs = args.bindings.iter_mut().all(|b| {
        let mut named_output = false;
        let ok = walk(&mut b.ty, *name, &mut 0, &mut named_output);
        if named_output && !named_input {
            diagnostics.push(Diagnostic::new(
                b.ty_range,
                format!("binding for associated type `{}` references lifetime `{name}`, which does not appear in the trait input types", b.name),
            ));
        }
        ok
    });
    ok_inputs && ok_outputs && inputs == 1
}
