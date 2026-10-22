// Appended inside the unchanged tree module to construct its real NodeData.
fn sample_tree() -> SyntaxTree {
    SyntaxTree::new(
        Box::from("a >>= b"),
        vec![
            SyntaxKind::IDENT,
            SyntaxKind::WHITESPACE,
            SyntaxKind::SHREQ,
            SyntaxKind::WHITESPACE,
            SyntaxKind::IDENT,
        ],
        vec![0, 1, 2, 5, 6, 7],
        vec![
            NodeData {
                kind: SyntaxKind::SOURCE_FILE,
                first_token: 0,
                end_token: 5,
                descendants: 3,
            },
            NodeData {
                kind: SyntaxKind::PATH_EXPR,
                first_token: 0,
                end_token: 1,
                descendants: 1,
            },
            NodeData {
                kind: SyntaxKind::NAME_REF,
                first_token: 0,
                end_token: 1,
                descendants: 0,
            },
            NodeData {
                kind: SyntaxKind::PATH_EXPR,
                first_token: 4,
                end_token: 5,
                descendants: 0,
            },
        ],
    )
}

// Records hash inputs without printing process-dependent pointer addresses.
#[derive(Default)]
struct NodeHash {
    words: Vec<usize>,
    bytes: Vec<u8>,
}
impl std::hash::Hasher for NodeHash {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    fn write_usize(&mut self, word: usize) {
        self.words.push(word);
    }
}
fn node_hash(node: SyntaxNode<'_>) -> NodeHash {
    let mut hash = NodeHash::default();
    std::hash::Hash::hash(&node, &mut hash);
    hash
}

pub fn exercise_tree() {
    let mut tree = sample_tree();
    assert!(tree.validate().is_ok());
    assert_eq!(size_of::<NodeData>(), 16);
    println!(
        "tree {} {} {}",
        tree.text(),
        tree.token_count(),
        tree.node_count()
    );
    let root = tree.root();
    assert!(root.parent().is_none());
    assert_eq!(root.children().count(), 2);
    assert_eq!(root.descendants().len(), 4);
    let first = root.first_child().unwrap();
    assert_eq!(first.parent(), Some(root));
    assert_eq!(first.first_child().unwrap().parent(), Some(first));
    let mut lossless = String::new();
    for token in root.tokens() {
        lossless.push_str(token.text());
        println!(
            "token {:?} {:?} {:?} {}",
            token.kind(),
            token.raw_range(),
            token.text_range(),
            token.ctxt()
        );
        assert!(std::ptr::eq(token.tree(), &tree));
    }
    assert_eq!(lossless, tree.text());
    for element in root.children_with_tokens() {
        println!(
            "element {:?} {:?} {}",
            element.kind(),
            element.raw_range(),
            element.into_node().is_some()
        );
    }
    let mut children = root.children();
    assert_eq!(children.next(), Some(first));
    let mut copied_children = children.clone();
    assert_eq!(children.next(), copied_children.next());
    assert!(children.next().is_none());
    assert!(children.next().is_none());
    // Independent iterators and stable node identity within one tree.
    let h1 = node_hash(first);
    let h2 = node_hash(tree.node(first.id()));
    assert_eq!(h1.words, h2.words);
    assert_eq!(h1.bytes, h2.bytes);
    assert_eq!(h1.words[0], &tree as *const SyntaxTree as usize);
    let other = tree.clone();
    assert_eq!(tree, other);
    assert_ne!(tree.root(), other.root());
    assert_ne!(tree.root().tokens().next(), other.root().tokens().next());
    assert_ne!(
        node_hash(tree.root()).words[0],
        node_hash(other.root()).words[0]
    );
    assert_ne!(node_hash(root).bytes, node_hash(first).bytes);
    println!("navigation identity clone ok");
    assert!(!tree.is_expansion());
    assert_eq!(
        tree.map_text_range(TextRange::new(1, 2)),
        TextRange::new(1, 2)
    );
    assert_eq!(tree.token_origin(0), None);
    let origin = |start, end, group, ctxt| TokenOrigin {
        range: TextRange::new(start, end),
        group,
        ctxt,
    };
    let call = TextRange::new(100, 110);
    // Three single-character origins compose the production >>= token.
    let origins = vec![
        (0, origin(10, 11, 1, 7)),
        (2, origin(12, 13, 1, 8)),
        (3, origin(13, 14, 1, 8)),
        (4, origin(14, 15, 1, 8)),
        (6, origin(16, 17, 1, 9)),
    ];
    tree.set_origins(&origins, call);
    assert!(tree.is_expansion());
    assert!(tree.validate().is_ok());
    assert_eq!(tree.root().text_range(), TextRange::new(10, 17));
    assert_eq!(tree.root().text(), "a >>= b");
    assert_eq!(tree.node(NodeId(3)).ctxt(), 9);
    assert_eq!(tree.token_origin(2).unwrap().range, TextRange::new(12, 15));
    assert_eq!(
        tree.map_text_range(TextRange::empty(1)),
        TextRange::new(12, 15)
    );
    assert_eq!(
        tree.map_text_range(TextRange::empty(7)),
        TextRange::new(16, 17)
    );
    assert_eq!(tree, tree.clone());
    println!(
        "expansion {:?} {} {:?}",
        tree.root().text_range(),
        tree.node(NodeId(1)).ctxt(),
        tree.token_origin(2)
    );
    // Unordered groups and synthetic tokens fall back to the call site.
    tree.set_origins(
        &[(0, origin(40, 41, 1, 3)), (6, origin(20, 21, 2, 4))],
        call,
    );
    assert_eq!(tree.root().text_range(), call);
    assert_eq!(tree.token_origin(2).unwrap().group, TokenOrigin::SYNTHETIC);
    assert_eq!(tree.node(NodeId(1)).text_range(), TextRange::new(40, 41));
    assert_eq!(tree, tree.clone());
    println!(
        "fallback {:?} {:?}",
        tree.root().text_range(),
        tree.token_origin(2)
    );
    // A composite whose parts are out of order uses the first part's
    // context and the call-site range, even with no missing origins.
    tree.set_origins(
        &[
            (0, origin(10, 11, 1, 7)),
            (2, origin(30, 31, 1, 8)),
            (3, origin(20, 21, 1, 9)),
            (4, origin(21, 22, 1, 9)),
            (6, origin(40, 41, 1, 7)),
        ],
        call,
    );
    assert_eq!(tree.token_origin(2).unwrap().range, call);
    assert_eq!(tree.token_ctxt(2), 8);
    assert_eq!(tree.root().text_range(), call);
    // A trivia-only expansion has no significant origin to map to.
    let mut trivia = SyntaxTree::new(
        Box::from(" "),
        vec![SyntaxKind::WHITESPACE],
        vec![0, 1],
        vec![NodeData {
            kind: SyntaxKind::SOURCE_FILE,
            first_token: 0,
            end_token: 1,
            descendants: 0,
        }],
    );
    trivia.set_origins(&[], call);
    assert!(trivia.validate().is_ok());
    assert_eq!(trivia.root().text_range(), TextRange::empty(100));
    assert_eq!(
        trivia.map_text_range(TextRange::empty(1)),
        TextRange::empty(100)
    );
    assert_eq!(trivia, trivia.clone());
    println!("trivia mapping {:?}", trivia.root().text_range());
    // Exercise an invariant failure, rather than only valid construction.
    let mut invalid = sample_tree();
    invalid.starts[2] = 0;
    assert_eq!(
        invalid.validate().unwrap_err(),
        "tokens do not tile the text"
    );
    println!("tree validation ok");
}
