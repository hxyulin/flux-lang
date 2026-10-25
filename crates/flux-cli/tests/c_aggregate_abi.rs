//! C aggregate parameters, return values and callbacks use the native ABI.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;
fn success(output: &std::process::Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
#[test]
fn c_aggregates_match_native_rustc_off_and_on() {
    let dir = std::env::temp_dir().join(format!("flux-c-aggregate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let native = dir.join("native.rs");
    std::fs::write(&native, include_str!("fixtures/c_aggregate_abi/native.rs.txt")).unwrap();
    let library = dir.join("libnative.dylib");
    success(&Command::new("rustc").args(["--edition=2024", "--crate-type=cdylib", "-C", "panic=abort"]).arg(&native).arg("-o").arg(&library).output().unwrap());
    let source = dir.join("main.rs");
    let text = include_str!("fixtures/c_aggregate_abi/consumer.rs.txt").replace("__LIBRARY__", library.to_str().unwrap());
    std::fs::write(&source, text).unwrap();
    let reference = dir.join("reference");
    success(&Command::new("rustc").args(["--edition=2024", "-C", "panic=abort"]).arg(&source).arg("-o").arg(&reference).output().unwrap());
    let expected = Command::new(&reference).output().unwrap(); success(&expected);
    for optimized in [false, true] {
        let executable = dir.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized { command.arg("-O"); }
        success(&command.output().unwrap());
        let actual = Command::new(&executable).output().unwrap(); success(&actual);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsupported_c_shapes_fail_before_machine_code() {
    for (name, source) in [
        ("generic_rust_repr", "struct Bad{words:[u64;2]} extern \"C\" fn identity<T>(x:T)->T{x} fn main(){let _=identity(Bad{words:[1,2]});}"),
        ("indirect_rust_repr", "struct Bad{words:[u64;2]} fn main(){let f:unsafe extern \"C\" fn(Bad)->u64=unsafe{std::mem::transmute(4096usize)};let _=unsafe{f(Bad{words:[1,2]})};}"),
        ("overaligned_declaration", "#[repr(C,align(32))]struct Bad{word:u64}unsafe extern \"C\"{fn take(x:Bad);} fn main(){}"),
        ("tuple", "extern \"C\" fn identity<T>(x:T)->T{x}fn main(){let _=identity((1u64,2u64));}"),
        ("generic_reference", "extern \"C\" fn identity<T>(x:T)->T{x}fn main(){let n=7u64;let _=identity(&n);}"),
        ("neon_vector", "extern \"C\" fn identity<T>(x:T)->T{x}fn main(){let _=identity(unsafe{core::arch::aarch64::vdup_n_u8(7)});}"),
        ("wide_pointer", "fn main(){let f:unsafe extern \"C\" fn(*const [u8])->u64=unsafe{std::mem::transmute(4096usize)};let s:&[u8]=&[];let _=unsafe{f(s as *const [u8])};}"),
    ] {
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{name}: {:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{name}: {:?}", mir.diagnostics);
        let errors = match flux_codegen::lower_module(&hir, &typed, &mir) {
            Err(errors) => errors,
            Ok(_) => panic!("{name}: emitted an unsupported C ABI"),
        };
        assert!(errors.iter().all(|error| error.message == "this type layout in the C ABI is not supported yet"), "{name}: {errors:?}");
    }
}

#[test]
fn direct_foreign_declarations_match_native_rustc_off_and_on() {
    let dir = std::env::temp_dir().join(format!("flux-c-aggregate-direct-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let native = dir.join("native.rs");
    std::fs::write(&native, include_str!("fixtures/c_aggregate_abi/native.rs.txt")).unwrap();
    success(&Command::new("rustc").args(["--edition=2024", "--crate-type=cdylib", "-C", "panic=abort"]).arg(&native).arg("-o").arg(dir.join("libnative.dylib")).output().unwrap());
    let source = "#[repr(C)]struct Words{words:[u64;2]} #[repr(C)]struct Padded{lead:u8,payload:u64,tail:u16}
        unsafe extern \"C\" {fn sum_words(x:Words)->u64;fn padded(x:Padded)->Padded;}
        extern \"C\" fn wrapper()->u64{unsafe{let p=padded(Padded{lead:3,payload:1000,tail:7});sum_words(Words{words:[17,25]})+p.payload+p.lead as u64+p.tail as u64}}
        fn main(){let _=wrapper();}";
    let (_, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let symbol = original.functions.iter().find(|f| f.name.contains("wrapper")).unwrap().name.clone();
    let driver = dir.join("driver.rs");
    std::fs::write(&driver, format!("unsafe extern \"C\" {{#[link_name={symbol:?}]fn wrapper()->u64;}}fn main(){{assert_eq!(unsafe{{wrapper()}},1077);}}")).unwrap();
    for (name, tier) in [("off", flux_codegen::OptimizationTier::None), ("on", flux_codegen::OptimizationTier::Full)] {
        let mut module = original.clone();
        module.functions.retain(|f| f.name != "main");
        module.functions.iter_mut().find(|f| f.name == symbol).unwrap().local = false;
        let object = flux_codegen::compile_object(&mut module, flux_codegen::Target::Aarch64AppleDarwin, tier).unwrap();
        let object_path = dir.join(format!("{name}.o"));
        std::fs::write(&object_path, object).unwrap();
        let executable = dir.join(name);
        success(&Command::new("rustc").args(["--edition=2024", "-C", "panic=abort"]).arg(&driver)
            .arg("-L").arg(&dir).args(["-l", "dylib=native"])
            .arg("-C").arg(format!("link-arg={}", object_path.display()))
            .arg("-C").arg(format!("link-arg=-Wl,-rpath,{}", dir.display()))
            .arg("-o").arg(&executable).output().unwrap());
        success(&Command::new(&executable).output().unwrap());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
