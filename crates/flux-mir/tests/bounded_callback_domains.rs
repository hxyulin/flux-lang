//! Input WF bounds retain their domain when an independent value is static.
#[test]
fn bounded_callback_domain_controls_match_rustc() {
    let positive = include_str!("../../../tests/run/bounded_callback_domains.rs");
    let pointer = positive.replace("parser:newline,", "parser:newline as fn(&mut &[u8]),");
    for (name, source, accepted) in [
        ("item", positive, true),
        ("pointer", pointer.as_str(), true),
        (
            "static_input",
            "fn require<I,F:FnMut(&mut I)>(_:F){}fn only_static(_:&mut &'static[u8]){}fn bad<'a>(_: &'a[u8]){require::<&'a[u8],_>(only_static);}fn main(){}",
            false,
        ),
        (
            "static_output",
            "fn bad<'a>(input:&'a u8)->&'static u8{let f:for<'b>fn(&'b u8)->&'static u8=|value:&u8|value;f(input)}fn main(){}",
            false,
        ),
        (
            "nested_universals",
            "fn require<F:for<'a,'b>Fn((&'a &'b u8,&'a &'static u8))->&'a u8>(_:F){}fn bad<'s>(saved:&'s u8){require(|_:(&&u8,&&'static u8)|saved);}fn main(){}",
            false,
        ),
        (
            "independent_inputs",
            "struct Input<'a,'b>(&'a u8,&'b u8);fn all(_:&mut Input<'_,'_>){}fn require<I,F:FnMut(&mut I)>(_:F){}fn check<'a,'b>(_:Input<'a,'b>){require::<Input<'a,'b>,_>(all);}fn main(){}",
            true,
        ),
        (
            "covered_parent",
            "struct Input<'a,'b>(&'a u8,&'b u8);fn require<'a,'b,F:for<'r>Fn(&'r Input<'a,'b>)->&'r u8>(_:&'a u8,_:&'b u8,_:F){}fn good<'a:'b,'b>(a:&'a u8,b:&'b u8){require(a,b,move|_:&Input<'a,'b>|a);}fn main(){}",
            true,
        ),
        (
            "incomparable_capture",
            "struct Input<'a,'b>(&'a u8,&'b u8);fn require<'a,'b,F:for<'r>Fn(&'r Input<'a,'b>)->&'r u8>(_:&'a u8,_:&'b u8,_:F){}fn bad<'a,'b>(a:&'a u8,b:&'b u8){require(a,b,move|_:&Input<'a,'b>|a);}fn main(){}",
            false,
        ),
        (
            "uncovered_parent",
            "struct Input<'a,'b>(&'a u8,&'b u8);fn require<'a,'b,F:for<'r>Fn(&'r Input<'a,'b>)->&'r u8>(_:&'a u8,_:&'b u8,_:F){}fn bad<'a,'b,'c>(a:&'a u8,b:&'b u8,c:&'c u8){require(a,b,move|_:&Input<'a,'b>|c);}fn main(){}",
            false,
        ),
        (
            "one_static_input",
            "struct Input<'a,'b>(&'a u8,&'b u8);fn restricted(_:&mut Input<'_, 'static>){}fn require<I,F:FnMut(&mut I)>(_:F){}fn check<'a,'b>(_:Input<'a,'b>){require::<Input<'a,'b>,_>(restricted);}fn main(){}",
            false,
        ),
        (
            "owner_ties_receiver_callback",
            "struct Node<'a>(&'a str);impl<'a> Node<'a>{fn tied(&'a self)->bool{self.0.is_empty()}} fn bad<'a>(node:Option<Node<'a>>)->Option<Node<'a>>{node.filter(Node::tied)}fn main(){}",
            false,
        ),
        (
            "owner_ties_receiver_pointer",
            "struct Node<'a>(&'a str);impl<'a> Node<'a>{fn tied(&'a self)->bool{self.0.is_empty()}} fn bad<'a>()->fn(&Node<'a>)->bool{Node::tied}fn main(){}",
            false,
        ),
        (
            "receiver_cannot_supply_owner_output",
            "struct Node<'a>(&'a str);impl<'a> Node<'a>{fn borrowed(&self)->&str{self.0}} fn bad<'a>()->fn(&Node<'a>)->&'a str{Node::borrowed}fn main(){}",
            false,
        ),
    ] {
        let directory = std::env::temp_dir().join(format!(
            "flux-bounded-callback-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&directory)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
        if matches!(
            name,
            "owner_ties_receiver_callback" | "owner_ties_receiver_pointer"
        ) {
            assert!(
                errors
                    .iter()
                    .any(|d| d.message.contains("not general enough")),
                "flux {name}: {errors:?}"
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
