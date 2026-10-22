mod hidden {
    fn secret() {}
    pub fn public() {}
}
fn outer() {}
macro_rules! import_private { () => {{ use crate::hidden::secret; secret(); }}; }
macro_rules! import_public { () => {{ use crate::hidden::public as scoped; scoped(); }}; }
macro_rules! duplicate { () => { use crate::hidden::public as same; }; }
macro_rules! generated_fn { () => {{ fn unsupported() {} }}; }
fn main() {
    import_private!();
    import_public!();
    scoped();
    { scoped(); }
    {
        use crate::outer as same;
        duplicate!();
    }
    generated_fn!();
}

// Preserve the invalid import-only tail-expression reproducer.
macro_rules! tail_use { () => { use crate::hidden::public; }; }
macro_rules! forward_tail_use { () => { tail_use!{} }; }
fn invalid_tail() { tail_use!() }
fn invalid_nested_tail() { forward_tail_use!() }
