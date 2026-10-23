//! Execute unchanged owned-arena excerpts; shared/Arc arenas remain excluded.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;

#[test]
fn actual_owned_arena_excerpts_match_rustc() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-arena/src/typed.rs");
    let original = std::fs::read_to_string(&file).unwrap();
    let first_start = original.find("pub struct Idx<T>").unwrap();
    let first_end = original
        .find("/// An [`Arena<T>`] whose first values")
        .unwrap();
    let map_start = original.find("/// Extra data for some values").unwrap();
    let map_end = original[map_start..].find("#[cfg(test)]").unwrap() + map_start;
    let first = &original[first_start..first_end];
    let map = &original[map_start..map_end];
    let driver = r#"
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};
struct Digest(u64);
impl Hasher for Digest {
    fn finish(&self) -> u64 { self.0 }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes { self.0 = self.0.wrapping_mul(131).wrapping_add(*byte as u64); }
    }
}
struct NotClone;
struct Dropped<'a>(&'a std::cell::Cell<u32>);
impl Drop for Dropped<'_> {
    fn drop(&mut self) { self.0.set(self.0.get()+1); }
}
fn main() {
    let mut arena=Arena::new();
    for i in 0..40 { arena.alloc(format!("value{}", i)); }
    let a=Idx::from_raw(3);
    arena[a].push('!');
    assert_eq!(arena[a],"value3!");
    assert_eq!(arena.next_idx().into_raw(),40);
    println!("index {:?} length {}",a,arena.len());
    for value in arena.values_mut().take(2) { value.push('?'); }
    let copy=arena.clone();
    assert!(arena==copy);
    assert_eq!(arena.iter().rev().next().unwrap().0.into_raw(),39);
    arena.shrink_to_fit();
    println!("first {:?} last {:?}",arena.get(Idx::from_raw(0)),arena.get(Idx::from_raw(39)));
    let mut map=ArenaMap::<String,String>::new();
    for raw in [1003,1001,999,1005] { map.insert(Idx::from_raw(raw), format!("at{}",raw)); }
    assert_eq!(map.insert(Idx::from_raw(1001),String::from("updated")),Some(String::from("at1001")));
    map.get_mut(Idx::from_raw(999)).unwrap().push('!');
    assert_eq!(map.remove(Idx::from_raw(1003)),Some(String::from("at1003")));
    assert!(!map.contains(Idx::from_raw(1003)));
    assert!(map.get(Idx::from_raw(998)).is_none());
    let cloned=map.clone();
    assert!(cloned==map);
    println!("map {:?}",map);
    println!("values {}",copy.into_values().count());
    let raw=Idx::<NotClone>::from_raw(3);
    let copied=raw.clone();
    assert!(raw==copied);
    assert!(raw<Idx::from_raw(4));
    let mut digest=Digest(0);
    copied.hash(&mut digest);
    println!("unbounded index {:?} hash {}",copied,digest.finish());
    let drops=std::cell::Cell::new(0);
    {
        let mut owned=Arena::new();
        for _ in 0..7 { owned.alloc(Dropped(&drops)); }
        assert_eq!(drops.get(),0);
    }
    println!("drops {}",drops.get());
    assert_eq!(drops.get(),7);
    assert_eq!(std::mem::size_of::<Idx<String>>(),4);
    let mut constant: Arena<u8> = const { Arena::new() };
    let idx=constant.alloc(17);
    println!("constant {:?} {}",idx,constant[idx]);
}
"#;
    let dir = std::env::temp_dir().join(format!("flux-owned-arenas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.rs");
    std::fs::write(&path, format!("{first}\n{map}\n{driver}")).unwrap();
    let mut expected = None;
    for flux in [false, true] {
        for optimized in [false, true] {
            let executable = dir.join(format!("arena-{flux}-{optimized}"));
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
            command.arg(&path).arg("-o").arg(&executable);
            if optimized {
                command.arg("-O");
            }
            let build = command.output().unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = Command::new(&executable).output().unwrap();
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            assert!(run.stderr.is_empty());
            match &expected {
                None => expected = Some(run.stdout),
                Some(output) => assert_eq!(&run.stdout, output),
            }
        }
    }
    assert_eq!(std::fs::read_to_string(file).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}
