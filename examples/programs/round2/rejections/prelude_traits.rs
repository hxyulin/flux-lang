//! MISSING FEATURE, REPORTED AS A PLAIN ERROR. `FromIterator`, `Extend`
//! and `AsRef` are in rustc's 2021/2024 prelude.
//!
//! rustc: compiles; exits with status 6.
//! flux: "cannot find trait `FromIterator` in this scope" (also `Extend`,
//! and `AsRef` in round2/hashing.rs): rustc's E0405 wording on valid code,
//! where "not supported yet" is expected. Blocks round2/fixed_vec.rs
//! (`collect()` into a custom container) and round2/hashing.rs.

struct Sum(i32);

impl FromIterator<i32> for Sum {
    fn from_iter<I: IntoIterator<Item = i32>>(iter: I) -> Self {
        let mut s = 0;
        for x in iter {
            s += x;
        }
        Sum(s)
    }
}

impl Extend<i32> for Sum {
    fn extend<I: IntoIterator<Item = i32>>(&mut self, iter: I) {
        for x in iter {
            self.0 += x;
        }
    }
}

fn bytes_len<T: AsRef<[u8]>>(t: T) -> usize {
    t.as_ref().len()
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let mut s: Sum = (1..=2).collect();
    s.extend([1]);
    unsafe { exit(s.0 + bytes_len([0u8; 2]) as i32) }
}
