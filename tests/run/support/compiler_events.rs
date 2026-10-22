// Appended inside the unchanged parser::event module by the host harness.
pub(crate) fn run_selfhost_checks() {
    assert_eq!(std::mem::size_of::<Event>(), 8);
    assert_eq!(std::mem::size_of::<Option<NonZeroU32>>(), 4);
    let mut output = Output::default();
    assert!(output.events.is_empty());
    assert!(output.errors.is_empty());
    output.events.push(Event::tombstone());
    output.events.push(Event::Start {
        kind: SyntaxKind::SOURCE_FILE,
        forward_parent: NonZeroU32::new(1),
    });
    output.events.push(Event::Start {
        kind: SyntaxKind::BIN_EXPR,
        forward_parent: NonZeroU32::new(u32::MAX),
    });
    output.events.push(Event::Token {
        kind: SyntaxKind::COLON2,
        n_raw_tokens: 2,
    });
    output.events.push(Event::Finish);
    output.errors.push(String::from("expected a type"));
    output.events.push(Event::Error { index: 0 });
    let mut copied = output.clone();
    assert_eq!(copied.events, output.events);
    assert_eq!(copied.errors, output.errors);
    copied.events.push(Event::Finish);
    copied.errors[0].push('!');
    assert_eq!(output.events.len(), 6);
    assert_eq!(output.errors[0], "expected a type");
    for event in &output.events {
        let event_copy = *event;
        assert_eq!(event_copy.clone(), *event);
        match event_copy {
            Event::Start {
                kind,
                forward_parent,
            } => {
                println!("event start {kind:?} {forward_parent:?}");
            }
            Event::Token { kind, n_raw_tokens } => {
                println!("event token {kind:?} {n_raw_tokens}");
            }
            Event::Finish => println!("event finish"),
            Event::Error { index } => println!("event error {}", output.errors[index as usize]),
        }
    }
    assert_eq!(
        output.events[0],
        Event::Start {
            kind: SyntaxKind::TOMBSTONE,
            forward_parent: None,
        }
    );
    assert_ne!(output.events[0], output.events[1]);
    println!("output {output:?}");
}
