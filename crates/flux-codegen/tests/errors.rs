//! Errors found only when code is generated: constants whose values
//! depend on generic arguments are evaluated for the instances that use
//! them, so as in rustc, one that fails is reported then, at its value.

/// The errors lowering `text` to keel IR reports, as `line:column:
/// message`; the program must check cleanly before that.
fn lowering_errors(text: &str) -> Vec<String> {
    let (db, root, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:#?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:#?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:#?}", mir.diagnostics);
    let Err(errors) = flux_codegen::lower_module(&hir, &typed, &mir) else {
        return Vec::new();
    };
    errors
        .iter()
        .map(|d| {
            let file = if d.file == flux_hir::FileId::UNKNOWN {
                root
            } else {
                d.file
            };
            let source = db.file(file);
            let before = &source.text()[..d.range.start() as usize];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
            format!("{line}:{column}: {}", d.message)
        })
        .collect()
}

#[test]
fn trait_constant_fails_for_one_instance() {
    let text = "\
trait Tr {
    const A: u8;
    const B: u8 = Self::A + 200;
}
struct S;
struct T;
impl Tr for S {
    const A: u8 = 10;
}
impl Tr for T {
    const A: u8 = 100;
}
fn get<X: Tr>() -> u8 {
    X::B
}
fn main() {
    let _ = get::<S>();
    let _ = get::<T>();
}
";
    assert_eq!(
        lowering_errors(text),
        ["3:19: attempt to compute `100_u8 + 200_u8`, which would overflow"]
    );
}

#[test]
fn failure_is_reported_where_it_happens() {
    // `C` fails because `B` does: rustc reports `B`.
    let text = "\
struct W<T>(T);
trait Size {
    const N: u8;
}
impl Size for i32 {
    const N: u8 = 4;
}
impl<T: Size> W<T> {
    const B: u8 = T::N * 100;
    const C: u8 = Self::B + 1;
}
fn get<T: Size>() -> u8 {
    W::<T>::C
}
fn main() {
    let _ = get::<i32>();
}
";
    assert_eq!(
        lowering_errors(text),
        ["9:19: attempt to compute `4_u8 * 100_u8`, which would overflow"]
    );
}

#[test]
fn instances_only_dead_code_needs_are_not_evaluated() {
    // As in rustc, instances are found from `main`.
    let text = "\
trait Tr {
    const A: u8;
    const B: u8 = Self::A + 200;
}
struct T;
impl Tr for T {
    const A: u8 = 100;
}
fn get<X: Tr>() -> u8 {
    X::B
}
fn unused() -> u8 {
    get::<T>()
}
fn main() {}
";
    assert_eq!(lowering_errors(text), Vec::<String>::new());
}

#[test]
fn endless_instances_are_an_error() {
    let text = "\
struct W<T>(T);
impl<T> W<T> {
    const A: u32 = W::<W<T>>::A;
}
fn get<T>() -> u32 {
    W::<T>::A
}
fn main() {
    let _ = get::<u8>();
}
";
    assert_eq!(
        lowering_errors(text),
        ["3:20: queries overflow the depth limit!"]
    );
}

#[test]
fn unused_instances_are_not_evaluated() {
    let text = "\
trait Tr {
    const A: u8;
    const B: u8 = Self::A + 200;
}
struct T;
impl Tr for T {
    const A: u8 = 100;
}
fn main() {
    let _ = T::A;
}
";
    assert_eq!(lowering_errors(text), Vec::<String>::new());
}

#[test]
#[cfg(target_arch = "aarch64")]
fn neon_constant_lanes_are_checked_after_substitution() {
    for (name, lane) in [
        ("vgetq_lane_u64", "2"),
        ("vgetq_lane_u64", "-1"),
        ("vget_lane_u64", "1"),
        ("vget_lane_u64", "-1"),
    ] {
        let vector = if name == "vgetq_lane_u64" {
            "vreinterpretq_u64_u8(vdupq_n_u8(0))"
        } else {
            "vreinterpret_u64_u8(*(&vdupq_n_u8(0) as *const uint8x16_t as *const uint8x8_t))"
        };
        let text = format!(
            "use core::arch::aarch64::*;\nunsafe fn lane<const N: i32>() -> u64 {{ unsafe {{ {name}::<N>({vector}) }} }}\nfn main() {{ unsafe {{ let _ = lane::<{lane}>(); }} }}"
        );
        let errors = lowering_errors(&text);
        assert_eq!(errors.len(), 1, "{name}::<{lane}>: {errors:?}");
        assert!(errors[0].ends_with("lane index out of range"), "{errors:?}");
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn neon_foundation_has_packed_aarch64_machine_lowering() {
    let text = "
use core::arch::aarch64::*;
unsafe fn run(p: *const u8, byte: u8) -> u64 {
    unsafe {
        let a = vld1q_u8(p);
        let b = vdupq_n_u8(byte);
        let c = vorrq_u8(vandq_u8(a, b), vceqq_u8(a, b));
        vgetq_lane_u64::<1>(vreinterpretq_u64_u8(c))
    }
}
fn main() { unsafe { let _ = run([1u8; 16].as_ptr(), 3); } }
";
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let asm = keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
    for instruction in [
        "dup\tv", "cmeq\tv", "and\tv", "orr\tv", "umov\tx", "ldr\tq", "str\tq",
    ] {
        assert!(asm.contains(instruction), "missing {instruction}:\n{asm}");
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn neon_reified_lane_is_checked_in_its_wrapper() {
    let text = "use core::arch::aarch64::*; fn main() { let f: unsafe fn(uint64x2_t) -> u64 = vgetq_lane_u64::<2>; unsafe { let _ = f(vreinterpretq_u64_u8(vdupq_n_u8(0))); } }";
    let errors = lowering_errors(text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].ends_with("lane index out of range"), "{errors:?}");
}

#[test]
#[cfg(target_arch = "aarch64")]
fn neon_narrow_shifts_are_checked_through_public_wrappers() {
    for shift in ["0", "9", "-1", "2147483647", "-2147483648"] {
        for call in [
            format!("vshrn_n_u16::<{shift}>(a)"),
            format!("generic::<{shift}>(a)"),
            format!(
                "{{ let f: unsafe fn(uint16x8_t) -> uint8x8_t = vshrn_n_u16::<{shift}>; f(a) }}"
            ),
            format!("invoke(generic::<{shift}>, a)"),
            format!("{{ let f: fn(uint16x8_t) -> uint8x8_t = generic::<{shift}>; f(a) }}"),
        ] {
            let text = format!(
                "use core::arch::aarch64::*;
fn generic<const N: i32>(a: uint16x8_t) -> uint8x8_t {{ unsafe {{ vshrn_n_u16::<N>(a) }} }}
fn invoke<F: Fn(uint16x8_t) -> uint8x8_t>(f: F, a: uint16x8_t) -> uint8x8_t {{ f(a) }}
fn main() {{ let a = unsafe {{ vreinterpretq_u16_u8(vdupq_n_u8(0)) }}; let _ = unsafe {{ {call} }}; }}"
            );
            let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
            assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
            let typed = flux_mir::check_file(&hir);
            assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
            // The diagnostic is in core's public wrapper, not main.rs.
            let errors: Vec<_> = flux_codegen::lower_module(&hir, &typed, &mir)
                .err()
                .unwrap_or_default()
                .into_iter()
                .map(|d| d.message)
                .collect();
            assert_eq!(errors.len(), 1, "{call}: {errors:?}");
            assert!(
                errors[0].ends_with("shift amount out of range: expected 1..=8"),
                "{errors:?}"
            );
        }
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn neon_memchr_next_lowering_preserves_store_width_and_unsigned_max() {
    for shift in 1..=8 {
        let text = "
use core::arch::aarch64::*;
fn main() {
    let a = unsafe { vdupq_n_u8(255) };
    let b = unsafe { vpmaxq_u8(a, vdupq_n_u8(128)) };
    let _ = unsafe { vshrn_n_u16::<3>(vreinterpretq_u16_u8(b)) };
}"
        .replace("::<3>", &format!("::<{shift}>"));
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let narrow = module
            .functions
            .iter()
            .find(|f| f.name.contains("vshrn_n_u16"))
            .unwrap();
        use keel_ir::{InstData, MemoryFlags, NeonIntrinsic, Type, VectorBinaryOp, VectorType};
        let mut loads = Vec::new();
        let mut stores = Vec::new();
        let mut shifts = Vec::new();
        for (_, inst) in narrow.layout_insts() {
            match narrow.inst_data(inst) {
                InstData::Load { ty, flags, .. } => {
                    assert_eq!(*flags, MemoryFlags::default());
                    loads.push(*ty);
                }
                InstData::Store { value, flags, .. } => {
                    assert_eq!(*flags, MemoryFlags::default());
                    stores.push(narrow.value_type(*value));
                }
                InstData::Neon { op, ty, args } => {
                    assert_eq!(*op, NeonIntrinsic::Shrn);
                    assert_eq!(*ty, Type::Vector(VectorType::I8x8));
                    assert_eq!(narrow.value_type(args[0]), Type::Vector(VectorType::I16x8));
                    assert_eq!(narrow.value_type(args[1]), Type::I32);
                    shifts.push(*op);
                }
                _ => {}
            }
        }
        assert_eq!(loads, [Type::Vector(VectorType::I16x8)]);
        assert_eq!(stores, [Type::Vector(VectorType::I8x8)]);
        assert_eq!(shifts, [NeonIntrinsic::Shrn]);
        let pair = module
            .functions
            .iter()
            .find(|f| f.name.contains("vpmaxq_u8"))
            .unwrap();
        let ops: Vec<_> = pair
            .layout_insts()
            .filter_map(|(_, inst)| {
                if let InstData::VectorBinary { op, .. } = pair.inst_data(inst) {
                    Some(*op)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ops, [VectorBinaryOp::Pumax]);
        for function in [narrow, pair] {
            assert!(function.sig.params.iter().all(|p| p.ty == Type::Ptr));
            assert!(function.sig.returns.is_empty());
        }
        let mut pair_loads = Vec::new();
        let mut pair_stores = Vec::new();
        for (_, inst) in pair.layout_insts() {
            match pair.inst_data(inst) {
                InstData::Load { ty, flags, .. } => {
                    assert_eq!(*flags, MemoryFlags::default());
                    pair_loads.push(*ty);
                }
                InstData::Store { value, flags, .. } => {
                    assert_eq!(*flags, MemoryFlags::default());
                    pair_stores.push(pair.value_type(*value));
                }
                _ => {}
            }
        }
        assert_eq!(pair_loads, [Type::Vector(VectorType::I8x16); 2]);
        assert_eq!(pair_stores, [Type::Vector(VectorType::I8x16)]);
        for tier in [
            keel_opt::OptimizationTier::None,
            keel_opt::OptimizationTier::Full,
        ] {
            keel_opt::optimize_with_tier(&mut module, tier).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let asm =
                keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
            for instruction in ["shrn\tv", "umaxp\tv", "ldr\tq", "str\td"] {
                assert!(asm.contains(instruction), "missing {instruction}:\n{asm}");
            }
            for instruction in ["ldrh\tw", "strb\tw", "cmhi\tv", "bsl\tv", "mrs\t", "msr\t"] {
                assert!(
                    !asm.contains(instruction),
                    "unexpected {instruction}:\n{asm}"
                );
            }
            assert!(
                asm.lines().any(|line| line.contains("shrn\tv")
                    && line.contains(".8b")
                    && line.contains(".8h")
                    && line.ends_with(&format!("#{shift}"))),
                "{asm}"
            );
            assert!(
                asm.lines()
                    .any(|line| line.contains("umaxp\tv") && line.matches(".16b").count() == 3),
                "{asm}"
            );
        }
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn neon_mask_apis_require_unsafe_calls_and_function_values() {
    for (params, call) in [
        ("a: uint16x8_t", "vshrn_n_u16::<1>(a)"),
        ("a: uint8x16_t, b: uint8x16_t", "vpmaxq_u8(a, b)"),
    ] {
        let text = format!(
            "use core::arch::aarch64::*; fn bad({params}) {{ let _ = {call}; }} fn main() {{}}"
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unsafe")),
            "{:?}",
            typed.diagnostics
        );
    }
    for value in [
        "let _: fn(uint16x8_t) -> uint8x8_t = vshrn_n_u16::<1>;",
        "let _: fn(uint8x16_t, uint8x16_t) -> uint8x16_t = vpmaxq_u8;",
        "invoke(vshrn_n_u16::<1>);",
        "invoke_pair(vpmaxq_u8);",
    ] {
        let text = format!(
            "use core::arch::aarch64::*;
fn invoke<F: Fn(uint16x8_t) -> uint8x8_t>(f: F) {{}}
fn invoke_pair<F: Fn(uint8x16_t, uint8x16_t) -> uint8x16_t>(f: F) {{}}
fn main() {{ {value} }}"
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(!typed.diagnostics.is_empty(), "accepted {value}");
    }
}
