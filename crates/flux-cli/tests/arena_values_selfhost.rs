//! Executes the unchanged Values iterator excerpt, not the complete arena crate.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;

#[test]
fn actual_arena_values_matches_rustc() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-arena/src/typed.rs");
    let original = std::fs::read_to_string(&file).unwrap();
    let start = original.find("struct Values<'a, T>").unwrap();
    let end = original[start..].find("impl<T> Index<Idx<T>>").unwrap() + start;
    let excerpt = &original[start..end];
    assert!(excerpt.contains("fn next_back(&mut self) -> Option<Self::Item>"));
    let dir = std::env::temp_dir().join(format!("flux-arena-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.rs");
    let driver = r#"
fn main() {
    let a = [2, 3, 5, 7];
    let b = [11, 13, 17, 19];
    for left in 0..=4 {
        for right in 0..=4 {
            let mut v = Values { frozen: a[..left].iter(), data: b[..right].iter() };
            println!("{} {} {:?} {:?} {:?} {:?} {:?} {}", left, right,
                v.size_hint(), v.next_back(), v.next(), v.nth(1), v.next_back(), v.len());
            for x in v { println!("remaining {}", x); }
        }
    }
}
"#;
    std::fs::write(&path, format!("{excerpt}\n{driver}")).unwrap();
    let mut expected = None;
    for flux in [false, true] {
        for optimized in [false, true] {
            let exe = dir.join(format!("values-{flux}-{optimized}"));
            let mut command = Command::new(if flux {
                env!("CARGO_BIN_EXE_flux")
            } else {
                "rustc"
            });
            if flux {
                command.arg("build");
            } else {
                command.args(["--edition=2024", "-C", "panic=abort"]);
            }
            command.arg(&path).arg("-o").arg(&exe);
            if optimized {
                command.arg("-O");
            }
            let build = command.output().unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            let output = Command::new(&exe).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            match &expected {
                None => expected = Some(output.stdout),
                Some(reference) => assert_eq!(&output.stdout, reference),
            }
        }
    }
    assert_eq!(std::fs::read_to_string(file).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}
