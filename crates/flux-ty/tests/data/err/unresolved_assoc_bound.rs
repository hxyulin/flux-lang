// A trait an associated type's bound names does not resolve: rustc
// stops before type checking, so the methods it would have given are
// not reported missing.

trait Digest: Default {
    type Output: Missing<[u8]> + Copy;
    fn finalize(self) -> Self::Output;
    fn digest(data: &[u8]) -> Self::Output {
        let _ = data;
        Self::default().finalize()
    }
}

fn check<D: Digest>(input: &[u8]) -> usize {
    let out = D::digest(input);
    let bytes = out.as_ref();
    bytes.len()
}

fn main() {}
