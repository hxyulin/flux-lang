// Reduced production AstNode shape. This remains rejected by Flux until
// obligations and method instantiation consume trait lifetime arguments.
#[derive(Clone, Copy)]
struct SyntaxNode<'t> {
    value: &'t i32,
}

trait AstNode<'t>: Copy {
    fn cast(syntax: SyntaxNode<'t>) -> Option<Self>;
    fn syntax(self) -> SyntaxNode<'t>;
}

#[derive(Clone, Copy)]
struct SourceFile<'t> {
    syntax: SyntaxNode<'t>,
}

impl<'t> AstNode<'t> for SourceFile<'t> {
    fn cast(syntax: SyntaxNode<'t>) -> Option<Self> {
        Some(Self { syntax })
    }
    fn syntax(self) -> SyntaxNode<'t> {
        self.syntax
    }
}

fn cast<'t, N: AstNode<'t>>(node: SyntaxNode<'t>) -> Option<N> {
    N::cast(node)
}

fn read<'t, N>(node: N) -> i32
where
    N: AstNode<'t>,
{
    *node.syntax().value
}

fn main() {
    let value = 37;
    let node = SyntaxNode { value: &value };
    match cast::<SourceFile>(node) {
        Some(file) => assert_eq!(read(file), 37),
        None => panic!("cast failed"),
    }
}
