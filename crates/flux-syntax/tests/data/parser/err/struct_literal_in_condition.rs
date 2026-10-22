fn f() {
    // A struct literal needs parentheses here, or the `{` opens the body
    // of the `if`. As rustc does, one that starts `{ name:` is reported
    // and read as one.
    if p == Point { x: 1 } {}
    match a::S { x: 1 } {
        _ => {}
    }
}
