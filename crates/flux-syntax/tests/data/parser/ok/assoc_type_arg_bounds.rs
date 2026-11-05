// The lookahead finds `Y<..>:`, but the arguments stop at `+`.
type A = X<Y<Z + W>: V>;
type B = X<Y<Z>: V>;
