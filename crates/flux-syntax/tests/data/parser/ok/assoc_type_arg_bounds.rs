// The lookahead finds `Y<..>:`, and `Z + W` is a bare trait object.
type A = X<Y<Z + W>: V>;
type B = X<Y<Z>: V>;
