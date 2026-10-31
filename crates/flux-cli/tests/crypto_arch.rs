//! Scalar Arm SHA operations preserve the host instruction results.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
#[test]
fn crypto_lanes_match_rustc_off_and_on() {
    assert!(
        std::arch::is_aarch64_feature_detected!("sha2"),
        "native SHA2 oracle unavailable"
    );
    assert!(
        std::arch::is_aarch64_feature_detected!("sha3"),
        "native SHA512 oracle unavailable"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/crypto-arch/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/run/crypto_arch.rs");
    let reference = root.join("rustc");
    let result = support::process::run(
        Command::new("rustc")
            .args(["--edition=2024", "-Cpanic=abort"])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "reference-build",
        Duration::from_secs(90),
    )
    .completed();
    support::oracle::success(&result, "Rust SHA instruction oracle build");
    let expected = support::process::run(
        &mut Command::new(&reference),
        &root,
        "reference-run",
        Duration::from_secs(30),
    )
    .completed();
    support::oracle::success(&expected, "Rust SHA instruction oracle");
    for optimize in [false, true] {
        let target = root.join(format!("flux-{optimize}"));
        let mut build = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        build.arg("build").arg(&source).arg("-o").arg(&target);
        if optimize {
            build.arg("-O");
        }
        let result = support::process::run(
            &mut build,
            &root,
            &format!("build-{optimize}"),
            Duration::from_secs(90),
        )
        .completed();
        support::oracle::success(&result, "Flux SHA build");
        let actual = support::process::run(
            &mut Command::new(&target),
            &root,
            &format!("run-{optimize}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}

#[test]
fn target_feature_contracts_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/target-feature-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let shared = "#[target_feature(enable=\"sha2\")]fn sha()->u32{7} #[target_feature(enable=\"neon\")]fn neon()->u32{3} fn plain()->u32{2} fn invoke<F:Fn()->u32>(f:F)->u32{f()}";
    for (name, extra, body, accepts) in [
        ("outside_call", "", "let _=sha();", false),
        ("unsafe_call", "", "let _=unsafe{sha()};", true),
        ("outside_neon", "", "let _=neon();", false),
        (
            "same_features",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=sha();}",
            "",
            true,
        ),
        (
            "feature_implication",
            "#[target_feature(enable=\"sha3\")]fn f(){let _=sha();let _=neon();}",
            "",
            true,
        ),
        (
            "feature_neon_implication",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=neon();}",
            "",
            true,
        ),
        (
            "missing_crc",
            "#[target_feature(enable=\"crc\")]fn crc(){}#[target_feature(enable=\"sha3\")]fn f(){crc();}",
            "",
            false,
        ),
        (
            "weaker_context",
            "#[target_feature(enable=\"neon\")]fn f(){let _=sha();}",
            "",
            false,
        ),
        (
            "nested_closure",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=(||sha())();}",
            "",
            true,
        ),
        (
            "nested_function",
            "#[target_feature(enable=\"sha2\")]fn f(){fn inner(){let _=sha();}inner();}",
            "",
            false,
        ),
        ("safe_pointer_outside", "", "let _:fn()->u32=sha;", false),
        (
            "safe_pointer_unsafe_block",
            "",
            "unsafe{let _:fn()->u32=sha;}",
            false,
        ),
        (
            "unsafe_pointer_outside",
            "",
            "let _:unsafe fn()->u32=sha;",
            true,
        ),
        (
            "safe_pointer_inside",
            "#[target_feature(enable=\"sha2\")]fn f(){let p:fn()->u32=sha;let _=p();}",
            "",
            true,
        ),
        (
            "safe_pointer_cast_outside",
            "",
            "let _=sha as fn()->u32;",
            false,
        ),
        (
            "safe_pointer_clone_outside",
            "",
            "let _:fn()->u32=sha.clone();",
            false,
        ),
        (
            "safe_pointer_cast_inside",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=sha as fn()->u32;}",
            "",
            true,
        ),
        (
            "safe_pointer_return_outside",
            "fn f()->fn()->u32{sha}",
            "",
            false,
        ),
        (
            "safe_pointer_return_inside",
            "#[target_feature(enable=\"sha2\")]fn f()->fn()->u32{sha}",
            "",
            true,
        ),
        (
            "pointer_call_unsafe",
            "",
            "let p:unsafe fn()->u32=sha;let _=p();",
            false,
        ),
        (
            "item_local_unsafe",
            "",
            "let f=sha;let _=unsafe{f()};",
            true,
        ),
        ("item_local_safe", "", "let f=sha;let _=f();", false),
        (
            "item_local_inside",
            "#[target_feature(enable=\"sha2\")]fn f(){let s=sha;let _=s();}",
            "",
            true,
        ),
        ("item_fn_trait_outside", "", "let _=invoke(sha);", false),
        (
            "item_fn_trait_inside",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=invoke(sha);}",
            "",
            false,
        ),
        (
            "pointer_fn_trait_inside",
            "#[target_feature(enable=\"sha2\")]fn f(){let p:fn()->u32=sha;let _=invoke(p);}",
            "",
            true,
        ),
        (
            "method_call",
            "struct S;impl S{#[target_feature(enable=\"sha2\")]fn f(&self){}}",
            "S.f();",
            false,
        ),
        (
            "method_call_inside",
            "struct S;impl S{#[target_feature(enable=\"sha2\")]fn f(&self){}}#[target_feature(enable=\"sha2\")]fn f(){S.f();}",
            "",
            true,
        ),
        (
            "safe_trait_method",
            "trait T{#[target_feature(enable=\"sha2\")]fn f(){}}",
            "",
            false,
        ),
        (
            "safe_trait_impl_method",
            "trait T{fn f();}struct S;impl T for S{#[target_feature(enable=\"sha2\")]fn f(){}}",
            "",
            false,
        ),
        (
            "unsafe_trait_method",
            "trait T{#[target_feature(enable=\"sha2\")]unsafe fn f(){}}",
            "",
            true,
        ),
        (
            "const_features",
            "#[target_feature(enable=\"sha2\")]const fn f()->u32{7}const C:u32=unsafe{f()};",
            "assert_eq!(C,7);",
            true,
        ),
        (
            "const_safe_call",
            "#[target_feature(enable=\"sha2\")]const fn f()->u32{7}const C:u32=f();",
            "",
            false,
        ),
        (
            "inline_const_safe_pointer",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=const{let p:fn()->u32=sha;p};}",
            "",
            false,
        ),
        (
            "inline_const_unsafe_pointer",
            "#[target_feature(enable=\"sha2\")]fn f(){let _=const{let p:unsafe fn()->u32=sha;p};}",
            "",
            true,
        ),
        (
            "inline_const_call",
            "#[target_feature(enable=\"sha2\")]const fn c()->u32{7}#[target_feature(enable=\"sha2\")]fn f(){let _=const{c()};}",
            "",
            true,
        ),
        (
            "const_fn_pointer",
            "#[target_feature(enable=\"sha2\")]const fn f()->fn()->u32{sha}",
            "",
            true,
        ),
        (
            "dyn_function_item",
            "#[target_feature(enable=\"sha2\")]fn f(){let _:&dyn Fn()->u32=&sha;}",
            "",
            false,
        ),
        (
            "inline_always",
            "#[target_feature(enable=\"sha2\")]#[inline(always)]fn f(){}",
            "",
            false,
        ),
        (
            "empty_attribute_call",
            "#[target_feature()]fn f(){}",
            "f();let _:fn()=f;",
            true,
        ),
        (
            "empty_attribute_fn_trait",
            "#[target_feature()]fn f(){}fn call<F:Fn()>(_:F){}",
            "call(f);",
            false,
        ),
        (
            "empty_feature",
            "#[target_feature(enable=\"\")]fn f(){}",
            "",
            false,
        ),
        (
            "malformed_feature",
            "#[target_feature(enable=7)]fn f(){}",
            "",
            false,
        ),
        (
            "repeated_features",
            "#[target_feature(enable=\"sha2,sha2\")]#[target_feature(enable=\"neon\")]fn f(){let _=sha();let _=neon();}",
            "",
            true,
        ),
        (
            "missing_neon_on_pointer",
            "",
            "let _:fn()->u32=neon;",
            false,
        ),
        (
            "extract_trailing_one",
            "",
            "unsafe{let a=core::arch::aarch64::vld1q_u64([1u64,2].as_ptr());let _=core::arch::aarch64::vextq_u64(a,a,1);}",
            true,
        ),
        (
            "extract_trailing_too_large",
            "",
            "unsafe{let a=core::arch::aarch64::vld1q_u64([1u64,2].as_ptr());let _=core::arch::aarch64::vextq_u64(a,a,2);}",
            false,
        ),
        (
            "extract_negative",
            "",
            "unsafe{let a=core::arch::aarch64::vld1q_u64([1u64,2].as_ptr());let _=core::arch::aarch64::vextq_u64::<-1>(a,a);}",
            false,
        ),
        (
            "extract_extra_argument",
            "",
            "unsafe{let a=core::arch::aarch64::vld1q_u64([1u64,2].as_ptr());let _=core::arch::aarch64::vextq_u64(a,a,0,1);}",
            false,
        ),
        (
            "user_legacy_attribute",
            "#[rustc_legacy_const_generics(2)]fn f<const N:i32>(_:u64,_:u64){}",
            "",
            false,
        ),
        ("main_feature", "", "", false),
        ("empty_main_feature", "", "", false),
    ] {
        if !support::cases::selected(name) {
            continue;
        }
        let main_attribute = match name {
            "main_feature" => "#[target_feature(enable=\"sha2\")]",
            "empty_main_feature" => "#[target_feature()]",
            _ => "",
        };
        let source = format!("{shared}\n{extra}\n{main_attribute}fn main(){{{body}}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = root.join(format!("{name}-rustc"));
        let expected = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "-Awarnings"])
                .arg(&path)
                .arg("-o")
                .arg(&reference),
            &root,
            &format!("{name}-rustc"),
            Duration::from_secs(60),
        )
        .completed();
        assert_eq!(
            expected.status.success(),
            accepts,
            "Rust {name}: {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let actual = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("build")
                .arg(&path)
                .arg("-o")
                .arg(root.join(format!("{name}-flux"))),
            &root,
            &format!("{name}-flux"),
            Duration::from_secs(60),
        )
        .completed();
        assert_eq!(
            actual.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
    }
}

#[test]
fn unchanged_sha2_compressors_match_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/crypto-sha2-source/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let metadata = support::process::run(
        Command::new("cargo").current_dir(&repo).args([
            "metadata",
            "--offline",
            "--locked",
            "--format-version=1",
        ]),
        &root,
        "metadata",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&metadata, "locked source metadata");
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "sha2")
        .unwrap();
    assert_eq!(package["version"], "0.11.1");
    let directory = Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let files = [
        directory.join("src/consts.rs"),
        directory.join("src/sha256/aarch64_sha2.rs"),
        directory.join("src/sha512/aarch64_sha3.rs"),
    ];
    let original = files
        .iter()
        .map(|p| (p.clone(), std::fs::read(p).unwrap()))
        .collect::<Vec<_>>();
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec_pretty(&original).unwrap(),
    )
    .unwrap();
    let source = root.join("consumer.rs");
    std::fs::write(
        &source,
        format!(
            "#[path={:?}]mod consts;#[path={:?}]mod sha256_impl;#[path={:?}]mod sha512_impl;\n{}",
            files[0],
            files[1],
            files[2],
            include_str!("fixtures/crypto_arch/consumer.rs.txt")
        ),
    )
    .unwrap();
    let reference = root.join("rustc");
    let output = support::process::run(
        Command::new("rustc")
            .args(["--edition=2024", "-Cpanic=abort", "-Awarnings"])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "reference-build",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&output, "original SHA source reference build");
    let expected = support::process::run(
        &mut Command::new(&reference),
        &root,
        "reference-run",
        Duration::from_secs(30),
    )
    .completed();
    support::oracle::success(&expected, "original SHA source reference");
    for optimize in [false, true] {
        let target = root.join(format!("flux-{optimize}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(&source).arg("-o").arg(&target);
        if optimize {
            command.arg("-O");
        }
        let output = support::process::run(
            &mut command,
            &root,
            &format!("build-{optimize}"),
            Duration::from_secs(90),
        )
        .completed();
        support::oracle::success(&output, "original SHA source Flux build");
        let actual = support::process::run(
            &mut Command::new(&target),
            &root,
            &format!("run-{optimize}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    let changes = original
        .iter()
        .filter(|(p, b)| std::fs::read(p).unwrap() != *b)
        .map(|(p, _)| p)
        .collect::<Vec<_>>();
    std::fs::write(
        root.join("source-integrity.json"),
        serde_json::to_vec(&changes).unwrap(),
    )
    .unwrap();
    assert!(changes.is_empty(), "upstream sources changed");
}
