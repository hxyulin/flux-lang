//! Explicit operands preserve register identity and Rust's template restrictions.
use std::process::Command;

#[test]
fn explicit_register_operands_match_rustc() {
    let (register, alias, reserved, foreign) = if cfg!(target_arch = "aarch64") {
        ("x0", "w0", "x29", "rax")
    } else {
        ("rax", "eax", "rsp", "x0")
    };
    let root = std::env::temp_dir().join(format!("flux-explicit-registers-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, accepted, assembly) in [
        ("input", true, format!("\"\", in(\"{register}\") 1usize")),
        ("alias_input", true, format!("\"\", in(\"{alias}\") 1u32")),
        (
            "overlap",
            false,
            format!("\"\", in(\"{register}\") 1usize, in(\"{alias}\") 2u32"),
        ),
        (
            "reserved",
            false,
            format!("\"\", in(\"{reserved}\") 1usize"),
        ),
        ("foreign", false, format!("\"\", in(\"{foreign}\") 1usize")),
        (
            "placeholder",
            false,
            format!("\"{{0}}\", in(\"{register}\") 1usize"),
        ),
        (
            "named",
            false,
            format!("\"\", value = in(\"{register}\") 1usize"),
        ),
    ] {
        let source = format!("use core::arch::asm; fn main() {{ unsafe {{ asm!({assembly}); }} }}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert_eq!(
            hir.diagnostics.is_empty(),
            accepted,
            "Flux {name}: {:?}",
            hir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_register_identity_survives_metadata() {
    use flux_hir::AsmRegister;
    use flux_hir::flib::{Decode, Decoder, Encode, Encoder};
    use flux_intern::Symbol;
    for value in [
        AsmRegister::InputFixed(Symbol::intern("x0")),
        AsmRegister::Fixed(Symbol::intern("v1")),
        AsmRegister::OutputFixed(Symbol::intern("rax")),
    ] {
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.into_bytes();
        let mut decoder = Decoder::new(&bytes);
        assert_eq!(AsmRegister::decode(&mut decoder).unwrap(), value);
        decoder.finish().unwrap();
    }
}
