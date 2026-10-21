//! The items of a file and their signatures.

use flux_arena::{Arena, Idx};
use flux_intern::Symbol;
use flux_syntax::ast::{self, AstChildren, AstNode};
use flux_syntax::{NodeId, SyntaxTree, TextRange};

use crate::Diagnostic;

pub type FunctionId = Idx<Function>;

/// A file's items, without bodies.
#[derive(Debug, Default)]
pub struct ItemTree {
    pub functions: Arena<Function>,
    pub diagnostics: Vec<Diagnostic>,
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
    pub has_body: bool,
    /// The `FN` node, for lowering its body.
    pub node: NodeId,
    /// The name's range, for diagnostics.
    pub name_range: TextRange,
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
            ast::Item::Struct(_) => "structs",
            ast::Item::Trait(_) => "traits",
            ast::Item::TypeAlias(_) => "type aliases",
            ast::Item::Use(_) => "`use` declarations",
        };
        self.unsupported(item.syntax().text_range(), what);
    }

    fn collect_fn(&mut self, f: ast::Fn<'_>, in_extern_block: bool) {
        check_attrs(f.attrs(), &mut self.diagnostics);
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
        if let Some(list) = f.param_list() {
            if let Some(param) = list.self_param() {
                unsupported(diagnostics, param.syntax().text_range(), "methods");
            }
            for param in list.params() {
                check_attrs(param.attrs(), diagnostics);
                if let Some(dots) = param.dot3_token() {
                    unsupported(diagnostics, dots.text_range(), "variadic functions");
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
        self.functions.alloc(Function {
            name: Symbol::intern(name.text()),
            params: params.into(),
            ret,
            ret_range,
            is_unsafe: in_extern_block || f.unsafe_token().is_some(),
            is_extern: in_extern_block,
            has_body: has_body && !in_extern_block,
            node: f.syntax().id(),
            name_range: name.syntax().text_range(),
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

    fn check_duplicates(&mut self) {
        let mut seen: Vec<Symbol> = Vec::new();
        for function in self.functions.values() {
            if seen.contains(&function.name) {
                self.diagnostics.push(Diagnostic::new(
                    function.name_range,
                    format!("the name `{}` is defined multiple times", function.name),
                ));
            } else {
                seen.push(function.name);
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
        ast::Type::PtrType(_) => "raw pointer types",
        ast::Type::RefType(_) => "reference types",
        ast::Type::SliceType(_) => "slice types",
    };
    unsupported(diagnostics, ty.syntax().text_range(), what);
    TypeRef::Error
}
