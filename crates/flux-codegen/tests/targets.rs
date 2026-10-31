#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[path = "../../flux-cli/tests/support/process.rs"]
mod process;

use flux_codegen::{OptimizationTier, Target};

#[test]
fn full_tier_uses_target_selection_for_offset_loads() {
    let source = "func @probe(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = iconst.i64 8\n v2 = ptradd v0, v1\n v3 = load.i64 v2\n return v3\n}\n";
    for tier in [
        OptimizationTier::None,
        OptimizationTier::Fast,
        OptimizationTier::Basic,
        OptimizationTier::Full,
    ] {
        let mut module = keel_ir::parse(source).unwrap();
        let actual =
            flux_codegen::compile_object(&mut module, Target::Aarch64AppleDarwin, tier).unwrap();
        let baseline = keel_codegen::compile_program(&module, Target::Aarch64AppleDarwin).unwrap();
        let baseline = keel_asm::assemble_program(&baseline, Default::default()).unwrap();
        if tier == OptimizationTier::Full {
            let selected = keel_codegen::compile_program_with_pipeline(
                &module,
                Target::Aarch64AppleDarwin,
                &keel_codegen::CodegenPipeline {
                    selector: &keel_codegen::SelectionTier::Full,
                    allocator: &keel_codegen::regalloc::AllocationTier::Full,
                },
            )
            .unwrap();
            let selected = keel_asm::assemble_program(&selected, Default::default()).unwrap();
            assert!(
                selected.text.len() < baseline.text.len(),
                "the offset load must fold into one memory instruction"
            );
            assert_eq!(
                actual,
                selected.to_macho().unwrap(),
                "Full must use Keel's target selector"
            );
        } else {
            assert_eq!(
                actual,
                baseline.to_macho().unwrap(),
                "{tier:?} must retain baseline selection"
            );
        }
    }
}

#[test]
fn supported_object_formats_and_unimplemented_linux_target() {
    let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main() { println!(\"{}\", 42); }");
    assert!(hir.diagnostics.is_empty());
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty());
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty());
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [
        OptimizationTier::None,
        OptimizationTier::Fast,
        OptimizationTier::Basic,
        OptimizationTier::Full,
    ] {
        for (target, magic) in [
            (Target::Aarch64AppleDarwin, [0xcf, 0xfa, 0xed, 0xfe]),
            (Target::X86_64LinuxGnu, *b"\x7fELF"),
        ] {
            let object = flux_codegen::compile_object(&mut module.clone(), target, tier).unwrap();
            assert_eq!(object[..4], magic, "{target:?} {tier:?}");
        }
    }
    assert_eq!(
        flux_codegen::compile_object(
            &mut module.clone(),
            Target::Aarch64LinuxGnu,
            OptimizationTier::None,
        )
        .unwrap_err()
        .to_string(),
        "code generation for AArch64 Linux is not supported yet"
    );
    assert_eq!(
        flux_codegen::link_executable(
            &[],
            Target::Aarch64LinuxGnu,
            std::path::Path::new("unused"),
        )
        .unwrap_err().to_string(),
        "linking for AArch64 Linux is not supported yet"
    );
}

#[test]
fn opt_in_timings_preserve_objects_and_partial_verification_failure() {
    let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main() { println!(\"timed\"); }");
    let typed = flux_mir::check_file(&hir);
    let mir = flux_mir::build_file(&hir, &typed);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [OptimizationTier::None, OptimizationTier::Full] {
        let ordinary =
            flux_codegen::compile_object(&mut module.clone(), Target::Aarch64AppleDarwin, tier)
                .unwrap();
        let (timed, scopes) = flux_codegen::compile_object_with_timings(
            &mut module.clone(),
            Target::Aarch64AppleDarwin,
            tier,
        );
        assert_eq!(timed.unwrap(), ordinary);
        assert!(scopes.verify.is_some());
        assert!(scopes.optimize.is_some());
        assert!(scopes.codegen.is_some());
        assert!(scopes.assemble.is_some());
    }
    // An unterminated block must fail before optimization or native codegen.
    let mut invalid = keel_ir::Module::default();
    let mut function = keel_ir::Function::new("invalid", keel_ir::Signature::new(&[], &[]));
    function.create_block();
    invalid.functions.push(function);
    let ordinary = flux_codegen::compile_object(
        &mut invalid.clone(),
        Target::Aarch64AppleDarwin,
        OptimizationTier::None,
    )
    .unwrap_err();
    let (result, scopes) = flux_codegen::compile_object_with_timings(
        &mut invalid,
        Target::Aarch64AppleDarwin,
        OptimizationTier::None,
    );
    assert_eq!(result.unwrap_err().to_string(), ordinary.to_string());
    assert_eq!(ordinary.stage(), flux_codegen::Stage::Verify);
    assert!(scopes.verify.is_some());
    assert!(scopes.optimize.is_none());
    assert!(scopes.codegen.is_none());
    assert!(scopes.assemble.is_none());
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn linker_dead_strip_collects_sections_but_not_individual_functions() {
    let entry = keel_asm::assemble_object(".text\n.globl _main\n_main:\n stp x29, x30, [sp, #-16]!\n bl _live\n mov w0, #0\n ldp x29, x30, [sp], #16\n ret\n").unwrap();
    let shared = keel_asm::assemble_object(".text\n.globl _live\n_live:\n mov x0, #7\n ret\n.globl _dead_same\n_dead_same:\n mov x0, #11\n ret\n").unwrap();
    let unused = keel_asm::assemble_object(
        ".text\n.globl _dead_separate\n_dead_separate:\n mov x0, #13\n ret\n",
    )
    .unwrap();
    let inputs = [
        keel_link::Input {
            name: "entry",
            bytes: &entry,
        },
        keel_link::Input {
            name: "shared",
            bytes: &shared,
        },
        keel_link::Input {
            name: "unused",
            bytes: &unused,
        },
    ];
    let mut sizes = Vec::new();
    for dead_strip in [false, true] {
        let executable = keel_link::link(&inputs, native_link_options(dead_strip)).unwrap();
        let contains = |name: &[u8]| {
            executable
                .bytes
                .windows(name.len())
                .any(|bytes| bytes == name)
        };
        assert!(contains(b"_live\0"));
        assert!(
            contains(b"_dead_same\0"),
            "a live input text section keeps all its functions"
        );
        assert_eq!(contains(b"_dead_separate\0"), !dead_strip);
        sizes.push(executable.bytes.len());
    }
    assert!(
        sizes[1] < sizes[0],
        "discarding an entire unused section must shrink the executable"
    );
    eprintln!("dead_strip off/on bytes: {sizes:?}; unreachable definitions retained: 2/1");
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_link_options(dead_strip: bool) -> keel_link::Options {
    static SDK: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let sdk = SDK.get_or_init(|| {
        std::env::var_os("SDKROOT")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../out/flib-codegen")
                    .join(format!("{}-dead-strip-sdk", std::process::id()));
                std::fs::create_dir_all(&directory).unwrap();
                let output = process::run(
                    std::process::Command::new("xcrun").args([
                        "--sdk",
                        "macosx",
                        "--show-sdk-path",
                    ]),
                    &directory,
                    "sdk",
                    std::time::Duration::from_secs(30),
                )
                .completed();
                assert!(output.status.success());
                std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
            })
    });
    let system = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    keel_link::Options {
        dead_strip,
        dylibs: vec![system],
        ..keel_link::Options::default()
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn user_library_keeps_unused_exports_in_its_live_text_section() {
    use flux_codegen::{Compilation, OutputKind};
    use flux_intern::Symbol;
    let (mut db, _, mut library) = flux_hir::lower_text_bare(
        "library.rs",
        "pub fn live() -> u64 { 7 } pub fn dead() -> u64 { 11 } fn private_dead() -> u64 { 13 }",
    );
    let library_id = library.items.main_crate();
    let root = db.add("consumer.rs", "fn main() { let _value = a::live(); }");
    let mut hir = flux_hir::lower_source_on(
        &mut db,
        library.items,
        root,
        Symbol::intern("consumer"),
        vec![(Symbol::intern("a"), library_id)],
    );
    let functions: Vec<_> = library.bodies.iter().map(|(id, _)| id).collect();
    for id in functions {
        hir.bodies.insert(id, library.bodies.remove(id).unwrap());
    }
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let hashes = [(library_id, [113; 32]), (hir.items.main_crate(), [114; 32])];
    let compilation = Compilation {
        kind: OutputKind::Library(library_id),
        crate_hashes: &hashes,
        imported: &[],
        builtin_object: false,
        required_functions: &[],
        required_instances: &[],
    };
    let library =
        flux_codegen::lower_crate_with_requirements(&hir, &typed, &mir, compilation).unwrap();
    assert_eq!(
        library.module.functions.len(),
        2,
        "only both public exports belong to the library object"
    );
    let dead = library
        .module
        .functions
        .iter()
        .find(|function| function.name.contains("4dead"))
        .unwrap()
        .name
        .clone();
    assert!(
        !library
            .module
            .functions
            .iter()
            .any(|function| function.name.contains("private_dead"))
    );
    let consumer = flux_codegen::lower_crate(
        &hir,
        &typed,
        &mir,
        Compilation {
            kind: OutputKind::Executable,
            imported: &[library_id],
            required_instances: &library.required_instances,
            ..compilation
        },
    )
    .unwrap();
    for tier in [OptimizationTier::None, OptimizationTier::Full] {
        let library_object = flux_codegen::compile_object(
            &mut library.module.clone(),
            Target::Aarch64AppleDarwin,
            tier,
        )
        .unwrap();
        let consumer_object =
            flux_codegen::compile_object(&mut consumer.clone(), Target::Aarch64AppleDarwin, tier)
                .unwrap();
        let inputs = [
            keel_link::Input {
                name: "library",
                bytes: &library_object,
            },
            keel_link::Input {
                name: "consumer",
                bytes: &consumer_object,
            },
        ];
        let mut sizes = Vec::new();
        for dead_strip in [false, true] {
            let executable = keel_link::link(&inputs, native_link_options(dead_strip)).unwrap();
            assert!(
                executable
                    .bytes
                    .windows(dead.len())
                    .any(|bytes| bytes == dead.as_bytes()),
                "the unreachable exported function shares the live library text section"
            );
            sizes.push(executable.bytes.len());
        }
        eprintln!(
            "user library {tier:?}, dead_strip off/on executable bytes: {sizes:?}; unreachable emitted exports: 1/1; private dead definitions: 0/0"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn linux_native_link_preserves_output_on_failure() {
    let directory = std::env::temp_dir().join(format!("flux-linux-link-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let output = directory.join("program");
    std::fs::write(&output, b"previous executable").unwrap();
    let error =
        flux_codegen::link_executable(b"invalid ELF", Target::X86_64LinuxGnu, &output).unwrap_err();
    assert_eq!(error.stage(), flux_codegen::Stage::Link);
    assert_eq!(std::fs::read(&output).unwrap(), b"previous executable");
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}
