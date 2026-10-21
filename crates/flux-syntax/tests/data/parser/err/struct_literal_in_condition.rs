fn f() {
    // Like rustc, a struct literal needs parentheses here: the `{` opens
    // the body of the `if`.
    if p == Point { x: 1 } {}
}
