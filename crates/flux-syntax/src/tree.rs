//! A flat, lossless syntax tree.
//!
//! A [`SyntaxTree`] owns the source text, its tokens and its nodes, each in
//! one array. Tokens are a kind and a start offset, trivia included. Nodes
//! are stored in preorder: each covers a contiguous range of tokens, and its
//! descendants follow it directly, so a node's subtree is one slice of the
//! node array and a whole file is three allocations.
//!
//! [`SyntaxNode`] and [`SyntaxToken`] are `Copy` handles: a reference to the
//! tree and a 32-bit index. Parents are not stored; [`SyntaxNode::parent`]
//! walks down from the root.

use std::fmt;
use std::iter::FusedIterator;

use crate::SyntaxKind;

/// A byte range in the source text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextRange {
    start: u32,
    end: u32,
}

impl TextRange {
    pub fn new(start: u32, end: u32) -> TextRange {
        assert!(start <= end, "invalid range {start}..{end}");
        TextRange { start, end }
    }

    pub fn empty(offset: u32) -> TextRange {
        TextRange::new(offset, offset)
    }

    pub fn start(self) -> u32 {
        self.start
    }

    pub fn end(self) -> u32 {
        self.end
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }
}

impl fmt::Debug for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl std::ops::Index<TextRange> for str {
    type Output = str;

    fn index(&self, range: TextRange) -> &str {
        &self[range.start as usize..range.end as usize]
    }
}

/// The index of a node in its [`SyntaxTree`]. The root is always 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub const ROOT: NodeId = NodeId(0);

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One node: 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeData {
    pub(crate) kind: SyntaxKind,
    /// The first token inside the node, and the one after its last.
    pub(crate) first_token: u32,
    pub(crate) end_token: u32,
    /// How many nodes follow this one in its subtree.
    pub(crate) descendants: u32,
}

const _: () = assert!(size_of::<NodeData>() == 16);

/// A parsed file: its text, tokens and nodes.
#[derive(Clone, PartialEq, Eq)]
pub struct SyntaxTree {
    text: Box<str>,
    kinds: Vec<SyntaxKind>,
    /// Start offset of each token, plus one final entry at `text.len()`.
    starts: Vec<u32>,
    nodes: Vec<NodeData>,
}

impl SyntaxTree {
    pub(crate) fn new(
        text: Box<str>,
        kinds: Vec<SyntaxKind>,
        starts: Vec<u32>,
        nodes: Vec<NodeData>,
    ) -> SyntaxTree {
        debug_assert_eq!(kinds.len() + 1, starts.len());
        debug_assert_eq!(starts.last().copied(), Some(text.len() as u32));
        debug_assert!(!nodes.is_empty());
        SyntaxTree {
            text,
            kinds,
            starts,
            nodes,
        }
    }

    pub fn root(&self) -> SyntaxNode<'_> {
        self.node(NodeId::ROOT)
    }

    pub fn node(&self, id: NodeId) -> SyntaxNode<'_> {
        assert!(id.index() < self.nodes.len(), "{id:?} is out of range");
        SyntaxNode { tree: self, id }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Number of tokens, trivia included.
    pub fn token_count(&self) -> usize {
        self.kinds.len()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Bytes of heap memory used by the tree, including the text.
    pub fn heap_size(&self) -> usize {
        self.text.len()
            + self.kinds.capacity() * size_of::<SyntaxKind>()
            + self.starts.capacity() * size_of::<u32>()
            + self.nodes.capacity() * size_of::<NodeData>()
    }

    fn data(&self, id: NodeId) -> &NodeData {
        &self.nodes[id.index()]
    }

    fn token_range(&self, index: u32) -> TextRange {
        let i = index as usize;
        TextRange::new(self.starts[i], self.starts[i + 1])
    }

    fn range_of_tokens(&self, first: u32, end: u32) -> TextRange {
        TextRange::new(self.starts[first as usize], self.starts[end as usize])
    }

    /// Checks the invariants that make the tree lossless: every token
    /// belongs to the root, every node lies inside its parent, and a
    /// node's children and tokens tile its range in order.
    pub fn validate(&self) -> Result<(), String> {
        if self.starts.first() != Some(&0)
            || self.starts.last() != Some(&(self.text.len() as u32))
            || self.starts.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("tokens do not tile the text".into());
        }
        let root = &self.nodes[0];
        if root.first_token != 0 || root.end_token as usize != self.kinds.len() {
            return Err("root does not cover every token".into());
        }
        if root.descendants as usize != self.nodes.len() - 1 {
            return Err("root does not contain every node".into());
        }
        for (i, node) in self.nodes.iter().enumerate() {
            let id = NodeId(i as u32);
            if node.first_token > node.end_token {
                return Err(format!("{id:?} ends before it starts"));
            }
            if i + node.descendants as usize >= self.nodes.len() {
                return Err(format!("{id:?} has descendants past the end"));
            }
            let mut pos = node.first_token;
            let mut child = i + 1;
            let end_child = i + 1 + node.descendants as usize;
            while child < end_child {
                let c = &self.nodes[child];
                if c.first_token < pos || c.end_token > node.end_token {
                    return Err(format!(
                        "{:?} is out of order or outside {id:?}",
                        NodeId(child as u32)
                    ));
                }
                pos = c.end_token;
                child += 1 + c.descendants as usize;
            }
            if child != end_child {
                return Err(format!("{id:?} has a child crossing its end"));
            }
        }
        Ok(())
    }
}

impl fmt::Debug for SyntaxTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.root(), f)
    }
}

/// A node in a [`SyntaxTree`].
#[derive(Clone, Copy)]
pub struct SyntaxNode<'t> {
    tree: &'t SyntaxTree,
    id: NodeId,
}

impl<'t> SyntaxNode<'t> {
    pub fn id(self) -> NodeId {
        self.id
    }

    pub fn tree(self) -> &'t SyntaxTree {
        self.tree
    }

    pub fn kind(self) -> SyntaxKind {
        self.data().kind
    }

    pub fn text_range(self) -> TextRange {
        let data = self.data();
        self.tree.range_of_tokens(data.first_token, data.end_token)
    }

    pub fn text(self) -> &'t str {
        &self.tree.text[self.text_range()]
    }

    /// Child nodes, without tokens.
    pub fn children(self) -> Children<'t> {
        Children {
            tree: self.tree,
            next: self.id.0 + 1,
            end: self.id.0 + 1 + self.data().descendants,
        }
    }

    /// Child nodes and tokens, in source order.
    pub fn children_with_tokens(self) -> ChildrenWithTokens<'t> {
        let data = self.data();
        ChildrenWithTokens {
            tree: self.tree,
            next_token: data.first_token,
            end_token: data.end_token,
            next_child: self.id.0 + 1,
            end_child: self.id.0 + 1 + data.descendants,
        }
    }

    /// This node and every node below it, in preorder.
    pub fn descendants(self) -> impl ExactSizeIterator<Item = SyntaxNode<'t>> + 't {
        let tree = self.tree;
        let start = self.id.0;
        (start..start + 1 + self.data().descendants).map(move |i| SyntaxNode {
            tree,
            id: NodeId(i),
        })
    }

    /// The tokens of this node, trivia included.
    pub fn tokens(self) -> impl ExactSizeIterator<Item = SyntaxToken<'t>> + 't {
        let tree = self.tree;
        let data = self.data();
        (data.first_token..data.end_token).map(move |index| SyntaxToken { tree, index })
    }

    pub fn first_child(self) -> Option<SyntaxNode<'t>> {
        self.children().next()
    }

    /// The node directly containing this one. Parents are not stored, so
    /// this walks down from the root: its cost grows with depth and with
    /// the number of siblings on the way.
    pub fn parent(self) -> Option<SyntaxNode<'t>> {
        let target = self.id.0;
        let mut node = self.tree.root();
        if target == 0 {
            return None;
        }
        loop {
            let child = node
                .children()
                .find(|c| c.id.0 <= target && target <= c.id.0 + c.data().descendants)
                .expect("a node lies inside one of the root's children");
            if child.id.0 == target {
                return Some(node);
            }
            node = child;
        }
    }

    fn data(self) -> &'t NodeData {
        self.tree.data(self.id)
    }
}

impl PartialEq for SyntaxNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.tree, other.tree) && self.id == other.id
    }
}

impl Eq for SyntaxNode<'_> {}

impl std::hash::Hash for SyntaxNode<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(self.tree, state);
        self.id.hash(state);
    }
}

impl fmt::Display for SyntaxNode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

/// `KIND@start..end`; with `{:#?}`, the whole subtree, one element per line
/// and indented by depth.
impl fmt::Debug for SyntaxNode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}@{:?}", self.kind(), self.text_range())?;
        if !f.alternate() {
            return Ok(());
        }
        writeln!(f)?;
        let mut stack = vec![self.children_with_tokens()];
        while let Some(children) = stack.last_mut() {
            let Some(element) = children.next() else {
                stack.pop();
                continue;
            };
            let depth = stack.len();
            write!(f, "{:width$}", "", width = 2 * depth)?;
            match element {
                SyntaxElement::Node(node) => {
                    writeln!(f, "{node:?}")?;
                    stack.push(node.children_with_tokens());
                }
                SyntaxElement::Token(token) => writeln!(f, "{token:?}")?,
            }
        }
        Ok(())
    }
}

/// A token in a [`SyntaxTree`]. Composite punctuation (`::`, `>>=`) is one
/// token here, though the lexer produced one per character.
#[derive(Clone, Copy)]
pub struct SyntaxToken<'t> {
    tree: &'t SyntaxTree,
    index: u32,
}

impl<'t> SyntaxToken<'t> {
    pub fn index(self) -> usize {
        self.index as usize
    }

    pub fn kind(self) -> SyntaxKind {
        self.tree.kinds[self.index as usize]
    }

    pub fn text_range(self) -> TextRange {
        self.tree.token_range(self.index)
    }

    pub fn text(self) -> &'t str {
        &self.tree.text[self.text_range()]
    }
}

impl PartialEq for SyntaxToken<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.tree, other.tree) && self.index == other.index
    }
}

impl Eq for SyntaxToken<'_> {}

impl fmt::Display for SyntaxToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

/// `KIND@start..end "text"`, with text past 21 bytes elided.
impl fmt::Debug for SyntaxToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}@{:?} ", self.kind(), self.text_range())?;
        let text = self.text();
        if text.len() < 25 {
            return write!(f, "{text:?}");
        }
        let cut = (21..25).find(|&i| text.is_char_boundary(i)).unwrap();
        write!(f, "{:?}", format!("{} ...", &text[..cut]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxElement<'t> {
    Node(SyntaxNode<'t>),
    Token(SyntaxToken<'t>),
}

impl<'t> SyntaxElement<'t> {
    pub fn kind(self) -> SyntaxKind {
        match self {
            SyntaxElement::Node(node) => node.kind(),
            SyntaxElement::Token(token) => token.kind(),
        }
    }

    pub fn text_range(self) -> TextRange {
        match self {
            SyntaxElement::Node(node) => node.text_range(),
            SyntaxElement::Token(token) => token.text_range(),
        }
    }

    pub fn into_node(self) -> Option<SyntaxNode<'t>> {
        match self {
            SyntaxElement::Node(node) => Some(node),
            SyntaxElement::Token(_) => None,
        }
    }

    pub fn into_token(self) -> Option<SyntaxToken<'t>> {
        match self {
            SyntaxElement::Node(_) => None,
            SyntaxElement::Token(token) => Some(token),
        }
    }
}

/// See [`SyntaxNode::children`].
#[derive(Clone)]
pub struct Children<'t> {
    tree: &'t SyntaxTree,
    next: u32,
    end: u32,
}

impl<'t> Iterator for Children<'t> {
    type Item = SyntaxNode<'t>;

    fn next(&mut self) -> Option<SyntaxNode<'t>> {
        if self.next >= self.end {
            return None;
        }
        let id = NodeId(self.next);
        self.next += 1 + self.tree.data(id).descendants;
        Some(SyntaxNode {
            tree: self.tree,
            id,
        })
    }
}

impl FusedIterator for Children<'_> {}

/// See [`SyntaxNode::children_with_tokens`]. Walks the node's tokens and
/// its child nodes side by side; a child is yielded when the walk reaches
/// its first token, and the walk then resumes after the child's last.
#[derive(Clone)]
pub struct ChildrenWithTokens<'t> {
    tree: &'t SyntaxTree,
    next_token: u32,
    end_token: u32,
    next_child: u32,
    end_child: u32,
}

impl<'t> Iterator for ChildrenWithTokens<'t> {
    type Item = SyntaxElement<'t>;

    fn next(&mut self) -> Option<SyntaxElement<'t>> {
        if self.next_child < self.end_child {
            let id = NodeId(self.next_child);
            let child = self.tree.data(id);
            if child.first_token <= self.next_token {
                self.next_child += 1 + child.descendants;
                self.next_token = child.end_token;
                return Some(SyntaxElement::Node(SyntaxNode {
                    tree: self.tree,
                    id,
                }));
            }
        }
        if self.next_token < self.end_token {
            let index = self.next_token;
            self.next_token += 1;
            return Some(SyntaxElement::Token(SyntaxToken {
                tree: self.tree,
                index,
            }));
        }
        None
    }
}

impl FusedIterator for ChildrenWithTokens<'_> {}
