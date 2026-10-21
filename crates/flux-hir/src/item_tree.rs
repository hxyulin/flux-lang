//! The items of a compilation and their signatures: every crate's
//! modules, read from their files, and the functions, structs, `impl`
//! blocks and imports in them.

use std::path::{Path as FsPath, PathBuf};

use flux_arena::{Arena, Idx};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::ast::{self, AstChildren, AstNode};
use flux_syntax::{NodeId, SyntaxElement, SyntaxKind, SyntaxNode, TextRange};

use crate::Diagnostic;
use crate::def_map::DefMap;
use crate::source::{FileId, SourceDb};

pub type FunctionId = Idx<Function>;
pub type StructId = Idx<Struct>;
pub type ImplId = Idx<Impl>;
pub type ModuleId = Idx<Module>;
pub type ConstId = Idx<Const>;
pub type TraitId = Idx<Trait>;

/// A crate, an index into [`ItemTree::crates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CrateId(pub u32);

/// Every item of every crate, without bodies.
#[derive(Debug, Default)]
pub struct ItemTree {
    pub crates: Vec<CrateData>,
    pub modules: Arena<Module>,
    /// Free functions, functions in `extern` blocks, and the associated
    /// functions of `impl` blocks.
    pub functions: Arena<Function>,
    pub structs: Arena<Struct>,
    pub traits: Arena<Trait>,
    pub impls: Arena<Impl>,
    /// Free and associated constants.
    pub consts: Arena<Const>,
    pub uses: Vec<Use>,
    /// What each module's names mean, imports included.
    pub def_map: DefMap,
    /// The associated items of each type's inherent `impl` blocks, by
    /// name, in source order.
    pub assoc: rustc_hash::FxHashMap<(ImplTarget, Symbol), Vec<AssocItem>>,
    /// The `impl` blocks of each trait, in source order.
    pub trait_impls: rustc_hash::FxHashMap<TraitId, Vec<ImplId>>,
    /// The traits with a function of each name.
    pub trait_fns: rustc_hash::FxHashMap<Symbol, Vec<TraitId>>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug)]
pub struct CrateData {
    pub name: Symbol,
    pub root: ModuleId,
    /// flux's own `core`, which may define methods on primitive types.
    pub is_core: bool,
}

/// A module: a crate's root, `mod m { ... }` or `mod m;`.
#[derive(Debug)]
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
#[derive(Debug)]
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
}

/// A constant: `const N: i32 = 5;`, free or in an `impl` block.
#[derive(Debug)]
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
#[derive(Debug)]
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
    pub functions: Vec<FunctionId>,
    /// The names of its items flux does not support (reported), which
    /// paths to them do not report again.
    pub unsupported: Vec<Symbol>,
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

/// A struct: `struct S { a: i32 }`, `struct P(i32);` or `struct U;`.
#[derive(Debug)]
pub struct Struct {
    pub name: Symbol,
    pub generics: Generics,
    pub kind: StructKind,
    /// In declaration order; a tuple struct's are named `0`, `1`, ...
    pub fields: Box<[Field]>,
    pub loc: ItemLoc,
    pub name_range: TextRange,
    /// `struct S`, where rustc reports errors about the whole struct.
    pub def_range: TextRange,
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

#[derive(Debug)]
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
}

/// A function: one with a body, or one declared in an `extern` block.
#[derive(Debug)]
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
    /// The whole item, body included.
    pub item_range: TextRange,
    /// `(x: i32)`
    pub param_list: TextRange,
}

#[derive(Debug)]
pub struct Param {
    pub ty: TypeRef,
    /// The type's range; `self`'s whole.
    pub range: TextRange,
    /// `x: i32`
    pub whole: TextRange,
}

/// An item's generic parameters, `<T, U>`, and the bounds on them, from
/// the list and its `where` clause. Only type parameters are supported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Generics {
    pub params: Box<[GenericParam]>,
    /// In source order: `T: A + B` in the list, then the `where` clause.
    pub predicates: Box<[WherePredicate]>,
    /// From `<` to `>`; empty if there is no list.
    pub range: TextRange,
    /// Had lifetime parameters (reported as not supported).
    pub has_lifetimes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Symbol,
    pub range: TextRange,
}

impl std::fmt::Display for Generics {
    /// `<T, U>`, or nothing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.params.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = self.params.iter().map(|p| p.name.to_string()).collect();
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
#[derive(Debug)]
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

/// Generic arguments as written: `<A, B>` or `::<A, B>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericArgs {
    pub types: Box<[TypeRef]>,
    /// Each type's range.
    pub ranges: Box<[TextRange]>,
    /// From `<` to `>`.
    pub range: TextRange,
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
                for (j, ty) in args.types.iter().enumerate() {
                    if j > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{ty}")?;
                }
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
    /// `&T` or `&mut T`.
    Ref {
        mutable: bool,
        pointee: Box<TypeRef>,
    },
    /// `_`: a type to infer, in a body.
    Infer,
    /// Unsupported or malformed; already reported.
    Error,
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
            TypeRef::Ptr { mutable, pointee } => {
                let m = if *mutable { "mut" } else { "const" };
                write!(f, "*{m} {pointee}")
            }
            TypeRef::Ref { mutable, pointee } => {
                let m = if *mutable { "mut " } else { "" };
                write!(f, "&{m}{pointee}")
            }
            TypeRef::Infer => f.write_str("_"),
            TypeRef::Error => f.write_str("<error>"),
        }
    }
}

impl ItemTree {
    /// Collects the items of `core` and of the crate whose root is
    /// `root`, reading the files of their modules, and resolves their
    /// names.
    pub fn new(db: &mut SourceDb, root: FileId, crate_name: Symbol, with_core: bool) -> ItemTree {
        let mut items = ItemTree::default();
        let core = with_core.then(|| db.load_core());
        let mut collector = Collector {
            db,
            items: &mut items,
        };
        if let Some(core) = core {
            collector.collect_crate(core, sym::core, true);
        }
        collector.collect_crate(root, crate_name, false);
        crate::def_map::resolve(&mut items);
        items
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
}

/// Reads crates' files and collects their items.
struct Collector<'a> {
    db: &'a mut SourceDb,
    items: &'a mut ItemTree,
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
            ast::Item::Enum(_) => "enums",
            ast::Item::ExternCrate(_) => "`extern crate` declarations",
            ast::Item::MacroCall(_) => "macros",
            ast::Item::Static(_) => "statics",
            ast::Item::Trait(t) => {
                self.collect_trait(ctx, t);
                return;
            }
            ast::Item::TypeAlias(_) => "type aliases",
        };
        unsupported(
            &mut self.items.diagnostics,
            item.syntax().text_range(),
            what,
        );
    }

    fn visibility(&mut self, ctx: Ctx, vis: Option<ast::Visibility<'_>>) -> Visibility {
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

    fn collect_trait(&mut self, ctx: Ctx, t: ast::Trait<'_>) {
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        for attr in t.attrs() {
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
        let generics =
            collect_generics(t.generic_param_list(), t.where_clause(), true, diagnostics);
        let mut supertraits = Vec::new();
        if let Some(list) = t.type_bound_list() {
            lower_bounds(list, &mut supertraits, diagnostics);
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
            functions: Vec::new(),
            unsupported: Vec::new(),
            name_range,
            def_range: TextRange::new(def_start(t.syntax()), name_range.end()),
        });
        let Some(list) = t.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        for item in list.items() {
            let what = match item {
                ast::Item::Fn(f) => {
                    if let Some(function) = self.collect_fn(ctx, f, false, Some(Owner::Trait(id))) {
                        let data = &self.items.functions[function];
                        let duplicate = self.items.traits[id]
                            .functions
                            .iter()
                            .any(|&g| self.items.functions[g].name == data.name);
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
                    if let Some(name) = c.name() {
                        self.items.traits[id]
                            .unsupported
                            .push(Symbol::intern(name.text()));
                    }
                    "associated constants in traits"
                }
                ast::Item::TypeAlias(t) => {
                    if let Some(name) = t.name() {
                        self.items.traits[id]
                            .unsupported
                            .push(Symbol::intern(name.text()));
                    }
                    "associated types"
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
            false,
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
        });
        let Some(list) = imp.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        for item in list.items() {
            let what = match item {
                ast::Item::Fn(f) => {
                    if let Some(function) = self.collect_fn(ctx, f, false, Some(Owner::Impl(id))) {
                        self.items.impls[id].functions.push(function);
                    }
                    continue;
                }
                ast::Item::Const(_) if is_trait_impl => {
                    "associated constants in trait implementations"
                }
                ast::Item::Const(c) => {
                    if let Some(konst) = self.collect_const(ctx, c, Some(id)) {
                        self.items.impls[id].consts.push(konst);
                    }
                    continue;
                }
                ast::Item::TypeAlias(_) => "associated types",
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

    fn collect_fn(
        &mut self,
        ctx: Ctx,
        f: ast::Fn<'_>,
        in_extern_block: bool,
        owner: Option<Owner>,
    ) -> Option<FunctionId> {
        let is_core = self.items.is_core(ctx.module);
        let mut inherits_overflow_checks = false;
        for attr in f.attrs() {
            if is_core && attr_name(attr) == Some("rustc_inherit_overflow_checks") {
                inherits_overflow_checks = true;
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.item_visibility(ctx, f.visibility(), owner);
        let name = f.name()?;
        let outer: Vec<Symbol> = match f.generic_param_list() {
            Some(_) => self
                .items
                .owner_generics(owner)
                .map_or(Vec::new(), |g| g.params.iter().map(|p| p.name).collect()),
            None => Vec::new(),
        };
        let diagnostics = &mut self.items.diagnostics;
        let generics =
            collect_generics(f.generic_param_list(), f.where_clause(), false, diagnostics);
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
                check_attrs(param.attrs(), diagnostics);
                if owner.is_none() {
                    diagnostics.push(Diagnostic::new(
                        range,
                        "`self` parameter is only allowed in associated functions",
                    ));
                } else if param.ty().is_some() {
                    unsupported(diagnostics, range, "`self` parameters with a type");
                } else if let Some(lifetime) = param.lifetime() {
                    unsupported(diagnostics, lifetime.syntax().text_range(), "lifetimes");
                } else {
                    let self_ty = TypeRef::Path(Path::single(kw::SelfUpper));
                    let ty = match param.amp_token() {
                        Some(_) => TypeRef::Ref {
                            mutable: param.mut_token().is_some(),
                            pointee: Box::new(self_ty),
                        },
                        None => self_ty,
                    };
                    params.push(Param {
                        ty,
                        range,
                        whole: range,
                    });
                    has_self = true;
                }
            }
            let count = list.params().count();
            for (i, param) in list.params().enumerate() {
                check_attrs(param.attrs(), diagnostics);
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
                params.push(Param {
                    ty: lower_type(param.ty(), diagnostics),
                    range: param
                        .ty()
                        .map_or(param.syntax().text_range(), |t| t.syntax().text_range()),
                    whole: param.syntax().text_range(),
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
        Some(
            self.items.functions.alloc(Function {
                def_range: TextRange::new(def_start(f.syntax()), signature_end),
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
                is_variadic,
                owner,
                has_self,
                has_body: has_body && !in_extern_block,
                node: f.syntax().id(),
                name_range: name.syntax().text_range(),
            }),
        )
    }

    fn collect_const(
        &mut self,
        ctx: Ctx,
        c: ast::Const<'_>,
        owner: Option<ImplId>,
    ) -> Option<ConstId> {
        check_attrs(c.attrs(), &mut self.items.diagnostics);
        let vis = self.item_visibility(ctx, c.visibility(), owner.map(Owner::Impl));
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

        if let Some(owner) = owner
            && !self.items.impls[owner].generics.is_empty()
        {
            unsupported(
                diagnostics,
                c.syntax().text_range(),
                "associated constants in generic `impl` blocks",
            );
        }
        let has_body = c.body().is_some();
        let def_start = def_start(c.syntax());
        let header_end = c.ty().map_or(name_end, |t| t.syntax().text_range().end());
        if !has_body {
            let message = match owner {
                Some(_) => "associated constant in `impl` without body",
                None => "free constant item without body",
            };
            let range = TextRange::new(def_start, c.syntax().text_range().end());
            diagnostics.push(Diagnostic::new(range, message));
        }
        Some(
            self.items.consts.alloc(Const {
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
                owner: owner.map(Owner::Impl),
                node: c.syntax().id(),
                has_body,
                def_range: TextRange::new(def_start, header_end),
            }),
        )
    }

    fn collect_struct(&mut self, ctx: Ctx, s: ast::Struct<'_>) {
        check_attrs(s.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, s.visibility());
        let Some(name) = s.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let generics = collect_generics(s.generic_param_list(), None, true, diagnostics);
        if let Some(clause) = s.where_clause() {
            unsupported(
                diagnostics,
                clause.syntax().text_range(),
                "`where` clauses on structs",
            );
        }
        if let Some(p) = generics.predicates.first() {
            let range = TextRange::new(
                p.bound.range.start(),
                generics
                    .predicates
                    .last()
                    .map_or(p.bound.range, |p| p.bound.range)
                    .end(),
            );
            unsupported(diagnostics, range, "trait bounds on structs");
        }
        let mut fields = Vec::new();
        let kind = if let Some(list) = s.record_field_list() {
            for field in list.fields() {
                check_attrs(field.attrs(), &mut self.items.diagnostics);
                let field_vis = self.visibility(ctx, field.visibility());
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
                    vis: field_vis,
                    range: field
                        .ty()
                        .map_or(field_name_range, |t| t.syntax().text_range()),
                    name_range: field_name_range,
                    def_range,
                });
            }
            StructKind::Record
        } else if let Some(list) = s.tuple_field_list() {
            for (i, field) in list.fields().enumerate() {
                check_attrs(field.attrs(), &mut self.items.diagnostics);
                let field_vis = self.visibility(ctx, field.visibility());
                let range = field.syntax().text_range();
                fields.push(Field {
                    name: Symbol::intern(&i.to_string()),
                    ty: lower_type(field.ty(), &mut self.items.diagnostics),
                    vis: field_vis,
                    range: field.ty().map_or(range, |t| t.syntax().text_range()),
                    name_range: range,
                    def_range: range,
                });
            }
            StructKind::Tuple
        } else {
            StructKind::Unit
        };
        let name_range = name.syntax().text_range();
        self.items.structs.alloc(Struct {
            name: Symbol::intern(name.text()),
            generics,
            kind,
            fields: fields.into(),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range: TextRange::new(def_start(s.syntax()), name_range.end()),
        });
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

pub(crate) fn check_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    const IGNORED: &[&str] = &[
        "allow", "warn", "deny", "expect", "inline", "cold", "must_use", "doc",
    ];
    let name = attr_name(attr);
    if name.is_some_and(|n| IGNORED.contains(&n)) {
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
    let mut segments = Vec::new();
    let mut current = Some(path);
    while let Some(p) = current {
        let segment = p.segment()?;
        if let Some(list) = segment.parenthesized_arg_list() {
            unsupported(
                diagnostics,
                list.syntax().text_range(),
                "parenthesized generic arguments",
            );
            return None;
        }
        segments.push(segment);
        current = p.qualifier();
    }
    segments.reverse();
    // `<T as Tr>::f`: the qualified segment comes first.
    let mut qself = None;
    if segments[0].name_ref().is_none() && segments[0].l_angle_token().is_some() {
        let segment = segments.remove(0);
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
    let (mut names, mut ranges, mut anchor_ranges) = (Vec::new(), Vec::new(), Vec::new());
    let mut args = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let name = segment.name_ref()?;
        let range = name.syntax().text_range();
        let list = segment.generic_arg_list();
        if let Some(list) = list
            && (name.crate_token().is_some()
                || name.super_token().is_some()
                || (name.self_token().is_some() && count > 1))
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
        if qself.is_some()
            && (name.crate_token().is_some()
                || name.super_token().is_some()
                || name.self_token().is_some())
        {
            misplaced("start position", diagnostics);
            return None;
        }
        if name.crate_token().is_some() {
            if i != 0 || anchor != PathAnchor::Plain {
                misplaced("start position", diagnostics);
                return None;
            }
            anchor = PathAnchor::Crate;
            anchor_ranges.push(range);
        } else if name.super_token().is_some() {
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
            anchor_ranges.push(range);
        } else if name.self_token().is_some() && i == 0 && count > 1 {
            anchor = PathAnchor::SelfModule;
            anchor_ranges.push(range);
        } else if name.self_token().is_some() && i != 0 && i + 1 != count {
            misplaced("start position or last position", diagnostics);
            return None;
        } else {
            names.push(Symbol::intern(name.text()));
            ranges.push(range);
            // Most paths have no arguments, and get no list of them.
            if let Some(list) = list {
                args.resize(names.len() - 1, None);
                args.push(Some(lower_generic_args(list, diagnostics)));
            }
        }
    }
    if !args.is_empty() {
        args.resize(names.len(), None);
    }
    Some(Path {
        anchor,
        qself,
        segments: names.into(),
        ranges: ranges.into(),
        anchor_ranges: anchor_ranges.into(),
        args: args.into(),
    })
}

/// `<A, B>`: type arguments only so far.
pub(crate) fn lower_generic_args(
    list: ast::GenericArgList<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> GenericArgs {
    let mut types = Vec::new();
    let mut ranges = Vec::new();
    for arg in list.args() {
        let range = arg.syntax().text_range();
        let what = match arg {
            ast::GenericArg::TypeArg(arg) => {
                types.push(lower_type(arg.ty(), diagnostics));
                ranges.push(range);
                continue;
            }
            ast::GenericArg::LifetimeArg(_) => "lifetime arguments",
            ast::GenericArg::ConstArg(_) => "const arguments",
            ast::GenericArg::AssocTypeArg(_) => "associated type arguments",
        };
        unsupported(diagnostics, range, what);
        types.push(TypeRef::Error);
        ranges.push(range);
    }
    GenericArgs {
        types: types.into(),
        ranges: ranges.into(),
        range: list.syntax().text_range(),
    }
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
    let path = lower_generic_path(path.path()?, diagnostics)?;
    Some(Bound { path, range })
}

/// The trait bounds in `list`, `A + B`; others are reported.
fn lower_bounds(
    list: ast::TypeBoundList<'_>,
    out: &mut Vec<Bound>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for bound in list.bounds() {
        let range = bound.syntax().text_range();
        let what = if bound.lifetime().is_some() {
            "lifetime bounds"
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
            if let Some(path) = lower_generic_path(path, diagnostics) {
                out.push(Bound { path, range });
            }
            continue;
        };
        unsupported(diagnostics, range, what);
    }
}

/// An item's generic parameter list and `where` clause: type parameters
/// without defaults, with distinct names, and trait bounds on types.
fn collect_generics(
    list: Option<ast::GenericParamList<'_>>,
    where_clause: Option<ast::WhereClause<'_>>,
    defaults_allowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Generics {
    let mut predicates = Vec::new();
    let where_predicates = |predicates: &mut Vec<WherePredicate>,
                            diagnostics: &mut Vec<Diagnostic>| {
        let Some(clause) = where_clause else {
            return;
        };
        for pred in clause.predicates() {
            let range = pred.syntax().text_range();
            if let Some(lifetime) = pred.lifetime() {
                unsupported(
                    diagnostics,
                    lifetime.syntax().text_range(),
                    "lifetime bounds",
                );
                continue;
            }
            if pred.for_token().is_some() {
                unsupported(diagnostics, range, "higher-ranked bounds");
                continue;
            }
            let ty_range = pred.ty().map_or(range, |t| t.syntax().text_range());
            let ty = lower_type(pred.ty(), diagnostics);
            let mut bounds = Vec::new();
            if let Some(list) = pred.type_bound_list() {
                lower_bounds(list, &mut bounds, diagnostics);
            }
            predicates.extend(bounds.into_iter().map(|bound| WherePredicate {
                ty: ty.clone(),
                ty_range,
                bound,
            }));
        }
    };
    let Some(list) = list else {
        where_predicates(&mut predicates, diagnostics);
        return Generics {
            predicates: predicates.into(),
            ..Generics::default()
        };
    };
    let mut params: Vec<GenericParam> = Vec::new();
    let mut has_lifetimes = false;
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
                    lower_bounds(list, &mut bounds, diagnostics);
                    let ty = TypeRef::Path(Path::single(Symbol::intern(name.text())));
                    let ty_range = name.syntax().text_range();
                    predicates.extend(bounds.into_iter().map(|bound| WherePredicate {
                        ty: ty.clone(),
                        ty_range,
                        bound,
                    }));
                }
                if let Some(default) = param.default_type() {
                    if defaults_allowed {
                        unsupported(
                            diagnostics,
                            default.syntax().text_range(),
                            "defaults for type parameters",
                        );
                    } else {
                        diagnostics.push(Diagnostic::new(
                            range,
                            "defaults for generic parameters are not allowed here",
                        ));
                    }
                }
                let name_range = name.syntax().text_range();
                let name = Symbol::intern(name.text());
                if params.iter().any(|p| p.name == name) {
                    diagnostics.push(Diagnostic::new(
                        name_range,
                        format!(
                            "the name `{name}` is already used for a generic parameter in this item's generic parameters"
                        ),
                    ));
                    continue;
                }
                params.push(GenericParam {
                    name,
                    range: name_range,
                });
                continue;
            }
            ast::GenericParam::LifetimeParam(_) => {
                has_lifetimes = true;
                "lifetime parameters"
            }
            ast::GenericParam::ConstParam(_) => "const generics",
        };
        unsupported(diagnostics, range, what);
    }
    where_predicates(&mut predicates, diagnostics);
    Generics {
        params: params.into(),
        predicates: predicates.into(),
        range: list.syntax().text_range(),
        has_lifetimes,
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
        ast::Type::ArrayType(_) => "array types",
        ast::Type::DynTraitType(_) => "trait objects",
        ast::Type::FnPtrType(_) => "function pointer types",
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
            if let Some(lifetime) = r.lifetime() {
                unsupported(diagnostics, lifetime.syntax().text_range(), "lifetimes");
            }
            let pointee = lower_type(r.ty(), diagnostics);
            return TypeRef::Ref {
                mutable: r.mut_token().is_some(),
                pointee: Box::new(pointee),
            };
        }
        ast::Type::SliceType(_) => "slice types",
    };
    unsupported(diagnostics, ty.syntax().text_range(), what);
    TypeRef::Error
}

/// Whether `name` is a primitive type's.
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
