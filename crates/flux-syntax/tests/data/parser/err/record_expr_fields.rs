fn attrs() {
    S { #[a] { : 0 } };
    S { #[a] };
}

fn base(s: S) {
    S { ..s b: 3 };
    S { ..s, };
    S { a: 1, ..s };
}
