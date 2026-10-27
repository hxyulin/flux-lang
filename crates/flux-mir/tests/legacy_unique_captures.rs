//! Legacy captures retain exclusive access to mutable-reference bindings.
use flux_hir::SourceDb;
use flux_intern::Symbol;
use flux_syntax::Edition;

#[test]
fn legacy_unique_captures_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-legacy-unique-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "drop_compaction",
            "fn f(r:&mut u32)->Result<(),String>{let value=String::from(\"owned\");drop(value);Ok::<(),String>(())?;for _ in 0..2{let mut c=||*r+=1;c()}Ok(())}fn main(){}",
            true,
        ),
        (
            "receiver",
            "struct S(u32);impl S{fn inc(&mut self){self.0+=1}fn f(&mut self){(||self.inc())()}}fn main(){}",
            true,
        ),
        (
            "dynamic_inherent",
            "trait W{fn bytes(&mut self,x:u32);}impl dyn W+'_ {fn write<T>(&mut self,_:&T){self.bytes(1)}}fn f(buffer:&mut dyn W){let mut c=||buffer.write(&1);c()}fn main(){}",
            true,
        ),
        (
            "dynamic",
            "trait W{fn write(&mut self,x:u32);}fn f(buffer:&mut dyn W){let mut c=||buffer.write(1);c()}fn main(){}",
            true,
        ),
        (
            "parameter",
            "fn f(r:&mut u32){let mut c=||*r+=1;c()}fn main(){}",
            true,
        ),
        (
            "nested",
            "fn f(r:&mut u32){let mut c=||{let mut d=||*r+=1;d()};c()}fn main(){}",
            true,
        ),
        (
            "borrow_binding",
            "fn f(r:&mut u32){let mut c=||{let _=&mut r;};c()}fn main(){}",
            false,
        ),
        (
            "assign_binding",
            "fn f<'a>(r:&'a mut u32,s:&'a mut u32){let mut c=||r=s;c()}fn main(){}",
            false,
        ),
        (
            "shared_conflict",
            "fn f(r:&mut u32){let mut c=||*r+=1;let x=&r;c();let _=x;}fn main(){}",
            false,
        ),
        (
            "mutable_conflict",
            "fn f(r:&mut u32){let mut c=||*r+=1;*r=2;c()}fn main(){}",
            false,
        ),
        (
            "two_closures",
            "fn f(r:&mut u32){let mut c=||*r+=1;let mut d=||*r+=2;c();d()}fn main(){}",
            false,
        ),
        (
            "shared_reference",
            "fn f(r:&u32){let mut c=||*r+=1;c()}fn main(){}",
            false,
        ),
        (
            "owned_binding",
            "fn f(){let r=1u32;let mut c=||r+=1;c()}fn main(){}",
            false,
        ),
    ] {
        for (edition, text) in [
            (Edition::E2015, "2015"),
            (Edition::E2018, "2018"),
            (Edition::E2021, "2021"),
            (Edition::E2024, "2024"),
        ] {
            let path = root.join(format!("{name}.rs"));
            std::fs::write(&path, source).unwrap();
            let oracle = std::process::Command::new("rustc")
                .args(["--edition", text, "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(root.join("oracle.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                oracle.status.success(),
                accepted,
                "{name}/{text}: {}",
                String::from_utf8_lossy(&oracle.stderr)
            );
            let mut db = SourceDb::new();
            let file = db.add_with_edition(&path, source, edition);
            let hir = flux_hir::lower(&mut db, file, Symbol::intern("probe"));
            let mut errors = hir.diagnostics.clone();
            if errors.is_empty() {
                let typed = flux_mir::check_file(&hir);
                errors.extend(typed.diagnostics.clone());
                if errors.is_empty() {
                    errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
                }
            }
            assert_eq!(errors.is_empty(), accepted, "{name}/{text}: {errors:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
