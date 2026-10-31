//! Compare token parsing with the compiler-provided proc_macro API.
use flux_proc_macro::{TokenStream, TokenTree};
use std::fmt::Write;
use std::process::{Command, Output};

fn shape(stream: TokenStream) -> String {
    let mut output = String::new();
    for token in stream {
        match token {
            TokenTree::Group(group) => write!(
                output,
                "G{:?}[{}]",
                group.delimiter(),
                shape(group.stream())
            )
            .unwrap(),
            TokenTree::Ident(ident) => write!(output, "I{ident};").unwrap(),
            TokenTree::Literal(literal) => write!(output, "L{literal};").unwrap(),
            TokenTree::Punct(punct) => {
                write!(output, "P{}{:?};", punct.as_char(), punct.spacing()).unwrap()
            }
        }
    }
    output
}

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn checked(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn parsed_token_shapes_match_rustc() {
    let cases = [
        "e\u{301} r#e\u{301} 'e\u{301}",
        "\u{2118}x a\u{B7}b",
        "1.0.0",
        "1.foo",
        "1..=2",
        "0x1e+2",
        "0xffu8",
        "1e-2f32",
        "1.25e+12",
        "1.f32",
        "\"hello\"suffix",
        "'x'suffix",
        "br#\"raw\"#suffix",
        "b'a'",
        "c\"hi\"",
        "'lifetime + 'x'",
        "/// comment\nstruct S;",
        "/** block */struct S;",
        "1/* comment */+2",
        "#[attr]",
        "a \u{85} b",
        "a\u{200E}b",
        "1_",
        "42u128",
        "1E+3",
        "1.2.3f32",
        "0o123",
        "0b1101",
        "r#type::x",
        "-> => :: && ||",
        "fn café() {}",
        "각",
        "(a [b] {c})",
    ];
    let fixture =
        Fixture(std::env::temp_dir().join(format!("flux-token-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&fixture.0).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let library = fixture.0.join(if cfg!(target_os = "macos") {
        "libprobe.dylib"
    } else {
        "libprobe.so"
    });
    std::fs::write(
        fixture.0.join("probe.rs"),
        r########"
extern crate proc_macro;
use proc_macro::{TokenStream, TokenTree, Literal};
use std::fmt::Write;
fn shape(stream: TokenStream) -> String {
    let mut output=String::new();
    for token in stream { match token {
        TokenTree::Group(g)=>write!(output,"G{:?}[{}]",g.delimiter(),shape(g.stream())).unwrap(),
        TokenTree::Ident(i)=>write!(output,"I{i};").unwrap(),
        TokenTree::Literal(l)=>write!(output,"L{l};").unwrap(),
        TokenTree::Punct(p)=>write!(output,"P{}{:?};",p.as_char(),p.spacing()).unwrap(),
    }} output
}
#[proc_macro]
pub fn probe(input:TokenStream)->TokenStream {
    let text=input.to_string();
    let source=text.strip_prefix("r######\"").unwrap().strip_suffix("\"######").unwrap();
    let result=source.parse::<TokenStream>().map(shape).unwrap_or_else(|_|"ERROR".to_owned());
    TokenTree::Literal(Literal::string(&result)).into()
}
"########,
    )
    .unwrap();
    checked(
        Command::new(&rustc)
            .current_dir(&fixture.0)
            .args([
                "--edition=2024",
                "--crate-type=proc-macro",
                "--crate-name=probe",
                "probe.rs",
                "-o",
            ])
            .arg(&library)
            .output()
            .unwrap(),
    );
    let mut main = String::from("fn main() {\n");
    for source in cases {
        writeln!(
            main,
            "println!(\"{{}}\",probe::probe!(r######\"{source}\"######));"
        )
        .unwrap();
    }
    main.push_str("}\n");
    std::fs::write(fixture.0.join("main.rs"), main).unwrap();
    let executable = fixture.0.join("reference");
    checked(
        Command::new(&rustc)
            .current_dir(&fixture.0)
            .args(["--edition=2024", "main.rs", "--extern"])
            .arg(format!("probe={}", library.display()))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap(),
    );
    let output = checked(Command::new(executable).output().unwrap());
    let output = String::from_utf8(output.stdout).unwrap();
    let mut failures = Vec::new();
    for (source, expected) in cases.into_iter().zip(output.lines()) {
        let actual = source
            .parse::<TokenStream>()
            .map(shape)
            .unwrap_or_else(|_| "ERROR".to_owned());
        if actual != expected {
            failures.push(format!(
                "{source:?}:\nexpected {expected:?}\nactual   {actual:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn literal_constructor_spellings_match_rustc() {
    let fixture =
        Fixture(std::env::temp_dir().join(format!("flux-literal-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&fixture.0).unwrap();
    let library = fixture.0.join(if cfg!(target_os = "macos") {
        "libconstruct.dylib"
    } else {
        "libconstruct.so"
    });
    let body = r#"
fn spellings() -> String {
    let mut result=String::new();
    for byte in 0..=255u8 {result.push_str(&format!("{:?}\n",Literal::byte_character(byte).to_string()));}
    let bytes:Vec<u8>=(0..=255).collect();
    result.push_str(&format!("{:?}\n",Literal::byte_string(&bytes).to_string()));
    for bytes in [b"\0".as_slice(), b"'\"\\\n\t\r\x01\x7f\0", "café\u{301}\u{85}\0".as_bytes(), b"\xff\xf0\x9f\x98\x80\xc3\xa9\xe2\x82\0"] {
        let value=std::ffi::CStr::from_bytes_with_nul(bytes).unwrap();
        result.push_str(&format!("{:?}\n",Literal::c_string(value).to_string()));
    }
    result
}
"#;
    std::fs::write(fixture.0.join("construct.rs"), format!("extern crate proc_macro; use proc_macro::{{Literal,TokenStream,TokenTree}};\n{body}\n#[proc_macro] pub fn construct(_:TokenStream)->TokenStream{{assert!(proc_macro::is_available()); TokenTree::Literal(Literal::string(&spellings())).into()}}\n")).unwrap();
    checked(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=proc-macro",
                "--crate-name=construct",
            ])
            .arg(fixture.0.join("construct.rs"))
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap(),
    );
    std::fs::write(
        fixture.0.join("main.rs"),
        "fn main(){print!(\"{}\",construct::construct!());}",
    )
    .unwrap();
    let binary = fixture.0.join("reference");
    checked(
        Command::new("rustc")
            .arg("--edition=2024")
            .arg(fixture.0.join("main.rs"))
            .arg("--extern")
            .arg(format!("construct={}", library.display()))
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap(),
    );
    let expected = checked(Command::new(&binary).output().unwrap()).stdout;
    // Use the same inputs against the native token API in an ordinary process.
    let mut actual = String::new();
    for byte in 0..=255u8 {
        writeln!(
            actual,
            "{:?}",
            flux_proc_macro::Literal::byte_character(byte).to_string()
        )
        .unwrap();
    }
    let bytes: Vec<u8> = (0..=255).collect();
    writeln!(
        actual,
        "{:?}",
        flux_proc_macro::Literal::byte_string(&bytes).to_string()
    )
    .unwrap();
    for bytes in [
        b"\0".as_slice(),
        b"'\"\\\n\t\r\x01\x7f\0",
        "café\u{301}\u{85}\0".as_bytes(),
        b"\xff\xf0\x9f\x98\x80\xc3\xa9\xe2\x82\0",
    ] {
        let value = std::ffi::CStr::from_bytes_with_nul(bytes).unwrap();
        writeln!(
            actual,
            "{:?}",
            flux_proc_macro::Literal::c_string(value).to_string()
        )
        .unwrap();
    }
    assert_eq!(actual.as_bytes(), expected);
}
