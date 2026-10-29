// A lone `:` can start `::a`, but not here.
fn slice() { let [:] = x; }
fn tuple_struct() { let S(:) = x; }
fn param([:]: T) {}
fn call() { f(:); }
type G = X<:>;
