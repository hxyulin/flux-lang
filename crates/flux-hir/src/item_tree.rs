//! The items of a file and their signatures.

use flux_arena::{Arena, Idx};
use flux_intern::Symbol;
use flux_syntax::ast::{self, AstChildren, AstNode};
use flux_syntax::{NodeId, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxTree, TextRange};

use crate::Diagnostic;

pub type FunctionId = Idx<Function>;
pub type StructId = Idx<Struct>;

/// A file's items, without bodies.
#[derive(Debug, Default)]
pub struct ItemTree {
    pub functions: Arena<Function>,
    pub structs: Arena<Struct>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A struct: `struct S { a: i32 }`, `struct P(i32);` or `struct U;`.
#[derive(Debug)]
pub struct Struct {
    pub name: Symbol,
    pub kind: StructKind,
    /// In declaration order; a tuple struct's are named `0`, `1`, ...
    pub fields: Box<[Field]>,
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
    pub params: Box<[Param]>,
    pub ret: TypeRef,
    /// The return type's range, or the name's when it is implicitly `()`.
    pub ret_range: TextRange,
    /// Declared `unsafe fn`, or inside an `unsafe extern` block: calling
    /// it needs an `unsafe` block.
    pub is_unsafe: bool,
    /// Declared in an `extern` block, so defined elsewhere (in C).
    pub is_extern: bool,
    /// Takes more arguments after `params`, like C's `printf(fmt, ...)`.
    pub is_variadic: bool,
    pub has_body: bool,
    /// The `FN` node, for lowering its body.
    pub node: NodeId,
    /// The name's range, for diagnostics.
    pub name_range: TextRange,
    /// From the first keyword to the end of the signature, where rustc
    /// reports errors about the whole function: `fn f(x: i32) -> i32`.
    pub def_range: TextRange,
}

#[derive(Debug)]
pub struct Param {
    pub ty: TypeRef,
    pub range: TextRange,
}

/// A type as written in the source, not yet resolved: `u8` is a path
/// until type checking finds the primitive it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// A single-segment path without generic arguments.
    Path(Symbol),
    /// `()` or `(A, B)`.
    Tuple(Box<[TypeRef]>),
    /// `!`
    Never,
    /// `*const T` or `*mut T`.
    Ptr {
        mutable: bool,
        pointee: Box<TypeRef>,
    },
    /// Unsupported or malformed; already reported.
    Error,
}

impl TypeRef {
    pub fn unit() -> TypeRef {
        TypeRef::Tuple(Box::new([]))
    }
}

impl ItemTree {
    pub fn new(tree: &SyntaxTree) -> ItemTree {
        let mut items = ItemTree::default();
        let file = ast::SourceFile::cast(tree.root()).expect("the root is a SOURCE_FILE");
        check_attrs(file.attrs(), &mut items.diagnostics);
        for item in file.items() {
            items.collect_item(item);
        }
        items.check_duplicates();
        items
    }

    pub fn function_by_name(&self, name: Symbol) -> Option<(FunctionId, &Function)> {
        self.functions.iter().find(|(_, f)| f.name == name)
    }

    pub fn struct_by_name(&self, name: Symbol) -> Option<(StructId, &Struct)> {
        self.structs.iter().find(|(_, s)| s.name == name)
    }

    fn collect_item(&mut self, item: ast::Item<'_>) {
        let what = match item {
            ast::Item::Fn(f) => {
                self.collect_fn(f, false);
                return;
            }
            ast::Item::ExternBlock(block) => {
                self.collect_extern_block(block);
                return;
            }
            ast::Item::Const(_) => "constants",
            ast::Item::Enum(_) => "enums",
            ast::Item::ExternCrate(_) => "`extern crate` declarations",
            ast::Item::Impl(_) => "`impl` blocks",
            ast::Item::MacroCall(_) => "macros",
            ast::Item::Module(_) => "modules",
            ast::Item::Static(_) => "statics",
            ast::Item::Struct(s) => {
                self.collect_struct(s);
                return;
            }
            ast::Item::Trait(_) => "traits",
            ast::Item::TypeAlias(_) => "type aliases",
            ast::Item::Use(_) => "`use` declarations",
        };
        self.unsupported(item.syntax().text_range(), what);
    }

    fn collect_fn(&mut self, f: ast::Fn<'_>, in_extern_block: bool) {
        check_attrs(f.attrs(), &mut self.diagnostics);
        check_visibility(f.visibility(), &mut self.diagnostics);
        let Some(name) = f.name() else {
            return;
        };
        let diagnostics = &mut self.diagnostics;
        if let Some(list) = f.generic_param_list() {
            unsupported(diagnostics, list.syntax().text_range(), "generic functions");
        }
        if let Some(clause) = f.where_clause() {
            unsupported(diagnostics, clause.syntax().text_range(), "`where` clauses");
        }
        for token in [f.const_token(), f.async_token()].into_iter().flatten() {
            let what = format!("`{}` functions", token.text());
            unsupported(diagnostics, token.text_range(), &what);
        }
        if let Some(abi) = f.abi() {
            // `extern "C" fn f() {}`: a Rust function callable from C.
            unsupported(diagnostics, abi.syntax().text_range(), "`extern` functions");
        }
        let mut params = Vec::new();
        let mut is_variadic = false;
        if let Some(list) = f.param_list() {
            if let Some(param) = list.self_param() {
                unsupported(diagnostics, param.syntax().text_range(), "methods");
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
        if in_extern_block && has_body {
            self.diagnostics.push(Diagnostic::new(
                name.syntax().text_range(),
                "functions in `extern` blocks cannot have bodies",
            ));
        } else if !in_extern_block && !has_body {
            self.diagnostics.push(Diagnostic::new(
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
        self.functions.alloc(Function {
            def_range: TextRange::new(def_start(f.syntax()), signature_end),
            name: Symbol::intern(name.text()),
            params: params.into(),
            ret,
            ret_range,
            is_unsafe: in_extern_block || f.unsafe_token().is_some(),
            is_extern: in_extern_block,
            is_variadic,
            has_body: has_body && !in_extern_block,
            node: f.syntax().id(),
            name_range: name.syntax().text_range(),
        });
    }

    fn collect_struct(&mut self, s: ast::Struct<'_>) {
        let diagnostics = &mut self.diagnostics;
        check_attrs(s.attrs(), diagnostics);
        check_visibility(s.visibility(), diagnostics);
        let Some(name) = s.name() else {
            return;
        };
        if let Some(list) = s.generic_param_list() {
            unsupported(diagnostics, list.syntax().text_range(), "generic structs");
        }
        if let Some(clause) = s.where_clause() {
            unsupported(diagnostics, clause.syntax().text_range(), "`where` clauses");
        }
        let mut fields = Vec::new();
        let kind = if let Some(list) = s.record_field_list() {
            for field in list.fields() {
                check_attrs(field.attrs(), diagnostics);
                check_visibility(field.visibility(), diagnostics);
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
                check_attrs(field.attrs(), diagnostics);
                check_visibility(field.visibility(), diagnostics);
                let range = field.syntax().text_range();
                fields.push(Field {
                    name: Symbol::intern(&i.to_string()),
                    ty: lower_type(field.ty(), diagnostics),
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
        self.structs.alloc(Struct {
            name: Symbol::intern(name.text()),
            kind,
            fields: fields.into(),
            name_range,
            def_range: TextRange::new(def_start(s.syntax()), name_range.end()),
        });
    }

    fn collect_extern_block(&mut self, block: ast::ExternBlock<'_>) {
        check_attrs(block.attrs(), &mut self.diagnostics);
        let abi = block.abi();
        if let Some(abi) = abi
            && abi.string_token().is_some_and(|s| s.text() != "\"C\"")
        {
            self.unsupported(abi.syntax().text_range(), "ABIs other than \"C\"");
        }
        if block.unsafe_token().is_none() {
            let range = abi.map_or(block.syntax().text_range(), |a| a.syntax().text_range());
            self.diagnostics.push(Diagnostic::new(
                range,
                "`extern` blocks must be `unsafe` in edition 2024",
            ));
        }
        let Some(list) = block.extern_item_list() else {
            return;
        };
        for item in list.items() {
            match item {
                ast::Item::Fn(f) => self.collect_fn(f, true),
                ast::Item::Static(_) => self.unsupported(item.syntax().text_range(), "statics"),
                _ => self.diagnostics.push(Diagnostic::new(
                    item.syntax().text_range(),
                    "only functions and statics can be declared in `extern` blocks",
                )),
            }
        }
    }

    /// Structs share the type namespace; functions and the constructors
    /// of tuple and unit structs share the value namespace.
    fn check_duplicates(&mut self) {
        let mut types: Vec<Symbol> = Vec::new();
        let mut values: Vec<Symbol> = Vec::new();
        let mut defined = Vec::new();
        for s in self.structs.values() {
            let in_values = s.kind != StructKind::Record;
            defined.push((s.def_range, s.name, true, in_values));
        }
        for f in self.functions.values() {
            defined.push((f.def_range, f.name, false, true));
        }
        defined.sort_by_key(|&(range, ..)| range.start());
        for (range, name, in_types, in_values) in defined {
            let duplicate =
                (in_types && types.contains(&name)) || (in_values && values.contains(&name));
            if duplicate {
                self.diagnostics.push(Diagnostic::new(
                    range,
                    format!("the name `{name}` is defined multiple times"),
                ));
                continue;
            }
            if in_types {
                types.push(name);
            }
            if in_values {
                values.push(name);
            }
        }
    }

    fn unsupported(&mut self, range: TextRange, what: &str) {
        unsupported(&mut self.diagnostics, range, what);
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
fn def_start(node: SyntaxNode<'_>) -> u32 {
    node.children_with_tokens()
        .find(|e| !e.kind().is_trivia() && e.kind() != SyntaxKind::ATTR)
        .map_or(node.text_range(), SyntaxElement::text_range)
        .start()
}

/// `pub` and `pub(crate)` mean nothing in a single-module crate.
fn check_visibility(vis: Option<ast::Visibility<'_>>, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(vis) = vis
        && (vis.l_paren_token().is_some() && vis.crate_token().is_none())
    {
        unsupported(
            diagnostics,
            vis.syntax().text_range(),
            "visibilities like this",
        );
    }
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

pub(crate) fn check_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    const IGNORED: &[&str] = &[
        "allow", "warn", "deny", "expect", "inline", "cold", "must_use", "doc",
    ];
    let name = attr
        .meta()
        .and_then(|m| m.path())
        .filter(|p| p.qualifier().is_none())
        .and_then(|p| p.segment())
        .and_then(|s| s.name_ref())
        .map(|n| n.text());
    if name.is_some_and(|n| IGNORED.contains(&n)) {
        return;
    }
    let message = match name {
        Some(name) => format!("the `{name}` attribute is not supported yet"),
        None => "attributes like this are not supported yet".to_owned(),
    };
    diagnostics.push(Diagnostic::new(attr.syntax().text_range(), message));
}

/// Lowers a type as written. A missing type was already reported by the
/// parser.
pub(crate) fn lower_type(ty: Option<ast::Type<'_>>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let Some(ty) = ty else {
        return TypeRef::Error;
    };
    let what = match ty {
        ast::Type::PathType(path) => {
            let segment = path
                .path()
                .filter(|p| p.qualifier().is_none())
                .and_then(|p| p.segment())
                .filter(|s| {
                    s.colon2_token().is_none()
                        && s.generic_arg_list().is_none()
                        && s.parenthesized_arg_list().is_none()
                });
            match segment.and_then(|s| s.name_ref()) {
                Some(name) => return TypeRef::Path(Symbol::intern(name.text())),
                None => "paths like this in types",
            }
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
        ast::Type::InferType(_) => "inferred types (`_`)",
        ast::Type::MacroType(_) => "macros",
        ast::Type::PtrType(ptr) => {
            let pointee = lower_type(ptr.ty(), diagnostics);
            return TypeRef::Ptr {
                mutable: ptr.mut_token().is_some(),
                pointee: Box::new(pointee),
            };
        }
        ast::Type::RefType(_) => "reference types",
        ast::Type::SliceType(_) => "slice types",
    };
    unsupported(diagnostics, ty.syntax().text_range(), what);
    TypeRef::Error
}
