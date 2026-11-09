//! Uninitialized storage is valid only behind MaybeUninit's validity boundary.
fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-const-uninit-{name}-{accepted}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    let mut errors = hir.diagnostics.clone();
    if errors.is_empty() {
        let typed = flux_mir::check_file(&hir);
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
    }
    assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn storage_and_initialized_payloads() {
    for (name, source) in [
        (
            "scalar",
            "const X:core::mem::MaybeUninit<u32>=core::mem::MaybeUninit::uninit();fn main(){let _=X;}",
        ),
        (
            "reference",
            "const X:core::mem::MaybeUninit<&'static u32>=core::mem::MaybeUninit::uninit();fn main(){let _=X;}",
        ),
        (
            "array",
            "const X:[core::mem::MaybeUninit<u32>;8]=[core::mem::MaybeUninit::uninit();8];fn main(){let _=X;}",
        ),
        (
            "static_cell",
            "struct Slot<T>(core::cell::UnsafeCell<core::mem::MaybeUninit<T>>);unsafe impl<T:Send+Sync> Sync for Slot<T>{}impl<T> Slot<T>{const fn new()->Self{Self(core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()))}}static X:Slot<Vec<usize>>=Slot::new();fn main(){let _=&X;}",
        ),
        (
            "initialized",
            "const X:u32=unsafe{core::mem::MaybeUninit::new(42).assume_init()};fn main(){assert_eq!(X,42);}",
        ),
    ] {
        compare(name, source, true);
    }
}
#[test]
fn uninitialized_reads_are_rejected() {
    for (name, source) in [
        (
            "integer",
            "const X:u32=unsafe{core::mem::MaybeUninit::<u32>::uninit().assume_init()};fn main(){}",
        ),
        (
            "reference",
            "const X:&u32=unsafe{core::mem::MaybeUninit::<&u32>::uninit().assume_init()};fn main(){}",
        ),
        (
            "nested",
            "const X:Option<u32>=Some(unsafe{core::mem::MaybeUninit::<u32>::uninit().assume_init()});fn main(){}",
        ),
        (
            "reinterpret",
            "const X:bool=unsafe{core::mem::transmute(core::mem::MaybeUninit::<bool>::uninit())};fn main(){}",
        ),
    ] {
        compare(name, source, false);
    }
}

#[test]
fn partial_initialization_and_live_storage() {
    compare(
        "write_method",
        r###"use core::mem::MaybeUninit as M;const X:u32={let mut x=M::uninit();x.write(42);unsafe{x.assume_init()}};fn main(){println!("{}",X);}"###,
        true,
    );
    compare(
        "raw_array_write",
        r###"use core::mem::MaybeUninit as M;const A:[u32;2]={let mut x=M::<[u32;2]>::uninit();let p=x.as_mut_ptr().cast::<u32>();unsafe{p.write(1);p.add(1).write(2);x.assume_init()}};fn main(){println!("{:?}",A);}"###,
        true,
    );
    compare(
        "helper_write",
        r###"use core::mem::MaybeUninit as M;const fn put(x:&mut M<u32>){x.write(42);}const X:u32={let mut x=M::uninit();put(&mut x);unsafe{x.assume_init()}};fn main(){println!("{}",X);}"###,
        true,
    );
    compare(
        "alias_read",
        r###"use core::mem::MaybeUninit as M;const X:u32={let mut x=M::<u32>::uninit();let p=x.as_mut_ptr();let q=p;unsafe{p.write(1);q.write(42);p.read()}};fn main(){println!("{}",X);}"###,
        true,
    );
    compare(
        "partial_pair",
        r###"use core::mem::MaybeUninit as M;struct Pair{a:u32,b:u32}const X:Pair={let mut x=M::<Pair>::uninit();let p=x.as_mut_ptr();unsafe{(&raw mut (*p).b).write(7);(&raw mut (*p).a).write(35);x.assume_init()}};fn main(){println!("{}",X.a+X.b);}"###,
        true,
    );
    compare(
        "partial_export",
        r###"use core::mem::MaybeUninit as M;struct Pair{a:u32,b:u32}const X:M<Pair>={let mut x=M::<Pair>::uninit();let p=x.as_mut_ptr();unsafe{(&raw mut (*p).a).write(35)};x};fn main(){let mut x=X;unsafe{(&raw mut (*x.as_mut_ptr()).b).write(7);let x=x.assume_init();println!("{}",x.a+x.b);}}"###,
        true,
    );
    compare(
        "zero_sized",
        r###"const X:()=unsafe{core::mem::MaybeUninit::<()>::uninit().assume_init()};fn main(){println!("{:?}",X);}"###,
        true,
    );
    compare(
        "zero_struct",
        r###"struct Z((),[u32;0]);const X:Z=unsafe{core::mem::MaybeUninit::<Z>::uninit().assume_init()};fn main(){println!("{}",core::mem::size_of_val(&X));}"###,
        true,
    );
    compare(
        "zero_array",
        r###"enum Void{}const X:[Void;0]=unsafe{core::mem::MaybeUninit::<[Void;0]>::uninit().assume_init()};fn main(){println!("{}",X.len());}"###,
        true,
    );
}

#[test]
fn invalid_frame_accesses_are_rejected() {
    compare(
        "uninhabited",
        r###"enum Void{}const X:Void=unsafe{core::mem::MaybeUninit::<Void>::uninit().assume_init()};fn main(){}"###,
        false,
    );
    compare(
        "out_of_bounds",
        r###"const X:u32={let mut x=0u32;let p=&raw mut x;unsafe{p.add(1).write(42)};x};fn main(){}"###,
        false,
    );
    compare(
        "unaligned",
        r###"const X:[u32;2]={let mut x=[0u32;2];let p=(&raw mut x).cast::<u8>();unsafe{p.add(1).cast::<u32>().write(42)};x};fn main(){}"###,
        false,
    );
    compare(
        "dangling",
        r###"const fn bad()->*mut u32{let mut x=1u32;&raw mut x}const X:u32=unsafe{bad().read()};fn main(){}"###,
        false,
    );
    compare(
        "shared_cast_write",
        r###"const X:u32={let mut x=1u32;let p=&x as *const u32 as *mut u32;unsafe{p.write(42)};x};fn main(){}"###,
        false,
    );
    compare(
        "shared_reborrow_write",
        r###"const X:u32={let mut x=1u32;let p=&mut x;let q=&*p as *const u32 as *mut u32;unsafe{q.write(42)};x};fn main(){}"###,
        false,
    );
    compare(
        "escaping",
        r###"const X:*mut u32={let mut x=1u32;&raw mut x};fn main(){}"###,
        false,
    );
}

#[test]
fn reordered_struct_fields_follow_physical_offsets() {
    compare(
        "reordered",
        r#"use core::mem::MaybeUninit as M;struct P{a:u8,b:u64,c:u16}const X:P={let mut x=M::<P>::uninit();let p=x.as_mut_ptr();unsafe{(&raw mut (*p).c).write(7);(&raw mut (*p).a).write(3);(&raw mut (*p).b).write(32);x.assume_init()}};fn main(){assert_eq!(X.a as u64+X.b+X.c as u64,42);}"#,
        true,
    );
}
