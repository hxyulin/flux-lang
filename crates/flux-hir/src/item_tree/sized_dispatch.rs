//! Trusted compile-time dispatch metadata. Only resolved definitions reach mono.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizedDispatch {
    pub selector: u32,
    pub parameters: u32,
    pub sized: FunctionId,
    pub fallback: FunctionId,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Declaration {
    pub selector: u32,
    pub sized: Symbol,
    pub fallback: Symbol,
}

pub(super) fn parse(attr: ast::Attr<'_>) -> Option<Declaration> {
    let meta = attr.meta()?;
    if meta.unsafe_token().is_some() || meta.eq_token().is_some() {
        return None;
    }
    let mut tokens = Vec::new();
    macros::tt::tokens_of(meta.token_tree()?.syntax(), &mut tokens);
    if tokens.first().is_none_or(|t| t.kind != T!['('])
        || tokens.last().is_none_or(|t| t.kind != T![')'])
    {
        return None;
    }
    let [index, comma, sized, comma2, fallback] = macros::strip_delimiters(&tokens) else {
        return None;
    };
    if index.kind != SyntaxKind::INT_NUMBER
        || comma.kind != T![,]
        || comma2.kind != T![,]
        || sized.kind != SyntaxKind::IDENT
        || fallback.kind != SyntaxKind::IDENT
    {
        return None;
    }
    let name = |token: &macros::tt::Tok| {
        Symbol::intern(
            token
                .text
                .as_str()
                .strip_prefix("r#")
                .unwrap_or(token.text.as_str()),
        )
    };
    Some(Declaration {
        selector: index.text.as_str().parse().ok()?,
        sized: name(sized),
        fallback: name(fallback),
    })
}

impl ItemTree {
    pub(super) fn resolve_sized_dispatch(&mut self, from: ItemCounts) {
        let declarations: Vec<_> = self
            .functions
            .iter()
            .skip(from.functions as usize)
            .filter_map(|(id, function)| {
                (function.intrinsic == Some(crate::Intrinsic::SizedDispatch)).then_some(id)
            })
            .collect();
        for id in declarations {
            let data = &self.functions[id];
            let metadata = self.sized_dispatch_declarations.get(&id).copied();
            let resolve = |name| match self.resolve(
                data.loc.module,
                &Path::single(name),
                crate::def_map::Ns::Values,
            ) {
                Ok(crate::def_map::Resolved::Def(crate::def_map::Def::Function(function))) => {
                    Some(function)
                }
                _ => None,
            };
            let dispatch = metadata.and_then(|metadata| {
                Some(SizedDispatch {
                    selector: metadata.selector,
                    parameters: data.generics.params.len() as u32,
                    sized: resolve(metadata.sized)?,
                    fallback: resolve(metadata.fallback)?,
                })
            });
            if let Some(dispatch) = dispatch {
                self.sized_dispatch.insert(id, dispatch);
            } else {
                self.diagnostics.push(Diagnostic {
                    file: data.loc.file,
                    range: data.sig_range,
                    message:
                        "sized dispatch requires metadata naming two resolvable helper functions"
                            .into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_has_a_fixed_schema() {
        for (arguments, expected) in [
            ("(0, sized, fallback)", true),
            ("(1, r#type, fallback)", true),
            ("[0, sized, fallback]", false),
            ("(0usize, sized, fallback)", false),
            ("(0, sized)", false),
            ("(0, sized, fallback, extra)", false),
            ("(true, sized, fallback)", false),
            ("(0, 3, fallback)", false),
        ] {
            let source = format!("#[rustc_sized_dispatch{arguments}]fn carrier(){{}}");
            let syntax = flux_syntax::parse_source_file(&source);
            let attr = syntax
                .tree()
                .root()
                .descendants()
                .find_map(ast::Attr::cast)
                .unwrap();
            assert_eq!(parse(attr).is_some(), expected, "{arguments}");
        }
    }
}
