// The lookahead finds `Y<..>:`, but the arguments stop at `+`: `&Z + W`
// is ambiguous.
type A = X<Y<&Z + W>: V>;
