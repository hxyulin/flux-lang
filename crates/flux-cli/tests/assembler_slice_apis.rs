//! Slice, vector and iterator contracts used by the unchanged assembler graph.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn assembler_slice_apis_match_rustc() {
    support::oracle::compare_fixture("assembler_slice_apis");
}

#[test]
fn production_retain_mut_preserves_unvisited_values_on_unwind() {
    use std::path::Path;
    use std::process::Command;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(repo.join("library/alloc/src/vec.rs")).unwrap();
    let method_start = source.find("    pub fn retain_mut<").unwrap();
    let method_end = method_start + source[method_start..].find("    /// Adds `value`").unwrap();
    let guard_start = source.find("struct RetainGuard<'a, T>").unwrap();
    let guard_end = guard_start
        + source[guard_start..]
            .find("/// Drops the values at `base`")
            .unwrap();
    let method = &source[method_start..method_end];
    let guard = &source[guard_start..guard_end];
    // Compile the production method and guard unchanged. The host wrapper
    // supplies only Vec storage so unwinding can exercise their cleanup.
    let program = format!(
        r#"
use std::ptr;
struct Vec<T> {{ storage: std::mem::ManuallyDrop<std::vec::Vec<T>>, len: usize }}
impl<T> Vec<T> {{
    fn new(storage: std::vec::Vec<T>) -> Self {{ let len = storage.len(); Self {{ storage: std::mem::ManuallyDrop::new(storage), len }} }}
    fn as_mut_ptr(&mut self) -> *mut T {{ self.storage.as_mut_ptr() }}
    fn as_slice(&self) -> &[T] {{ unsafe {{ std::slice::from_raw_parts(self.storage.as_ptr(), self.len) }} }}
{method}
}}
impl<T> Drop for Vec<T> {{ fn drop(&mut self) {{ unsafe {{ self.storage.set_len(self.len); std::mem::ManuallyDrop::drop(&mut self.storage); }} }} }}
{guard}
use std::cell::Cell;
use std::panic::{{catch_unwind, AssertUnwindSafe}};
struct Item<'a> {{ n: usize, drops: &'a Cell<usize>, bomb: &'a Cell<bool> }}
impl Drop for Item<'_> {{ fn drop(&mut self) {{ self.drops.set(self.drops.get()+1); if self.n==1 && self.bomb.replace(false) {{ panic!("drop once"); }} }} }}
fn main() {{
    let drops=Cell::new(0); let bomb=Cell::new(false);
    let mut v=Vec::new((0..5).map(|n|Item{{n,drops:&drops,bomb:&bomb}}).collect());
    assert!(catch_unwind(AssertUnwindSafe(||v.retain_mut(|x|{{if x.n==2{{panic!("predicate");}}if x.n==1{{x.n=11;true}}else{{false}}}}))).is_err());
    assert_eq!(v.as_slice().iter().map(|x|x.n).collect::<std::vec::Vec<_>>(),[11,2,3,4]);
    assert_eq!(drops.get(),1); drop(v); assert_eq!(drops.get(),5);
    drops.set(0); bomb.set(true);
    let mut v=Vec::new((0..5).map(|n|Item{{n,drops:&drops,bomb:&bomb}}).collect());
    assert!(catch_unwind(AssertUnwindSafe(||v.retain_mut(|_|false))).is_err());
    assert_eq!(v.as_slice().iter().map(|x|x.n).collect::<std::vec::Vec<_>>(),[2,3,4]);
    assert_eq!(drops.get(),2); drop(v); assert_eq!(drops.get(),5);
}}
"#
    );
    let directory = repo.join(format!("out/retain-mut-unwind/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("driver.rs");
    let executable = directory.join("driver");
    std::fs::write(&path, program).unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2024", "-O"])
        .arg(&path)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(executable).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
