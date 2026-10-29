//! Standard-reader ownership and thread-bound stdio guard contracts.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;

#[test]
fn stream_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-io-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "borrowed_take",
            "use std::io::{Read,BufRead,BufReader,Cursor};fn take<R:Read+?Sized>(r:&mut R){let mut t=r.take(3);let mut b=Vec::new();t.read_to_end(&mut b).unwrap();}fn main(){let mut b=BufReader::new(Cursor::new(b\"bytes\"));take(&mut b);}",
            true,
        ),
        (
            "stdin_static_lock",
            "fn lock()->std::io::StdinLock<'static>{std::io::stdin().lock()}fn main(){let _=lock();}",
            true,
        ),
        (
            "stdout_static_lock",
            "fn lock()->std::io::StdoutLock<'static>{let out=std::io::stdout();out.lock()}fn main(){let _=lock();}",
            true,
        ),
        (
            "handles_send_sync",
            "fn yes<T:Send+Sync>(){}fn main(){yes::<std::io::Stdin>();yes::<std::io::Stdout>();yes::<std::io::Stderr>();}",
            true,
        ),
        (
            "stdin_guard_sync",
            "fn yes<T:Sync>(){}fn main(){yes::<std::io::StdinLock<'static>>();}",
            true,
        ),
        (
            "stdout_guard_not_sync",
            "fn yes<T:Sync>(){}fn main(){yes::<std::io::StdoutLock<'static>>();}",
            false,
        ),
        (
            "stderr_guard_not_sync",
            "fn yes<T:Sync>(){}fn main(){yes::<std::io::StderrLock<'static>>();}",
            false,
        ),
        (
            "buffered_send",
            "fn yes<T:Send+Sync>(){}fn main(){yes::<std::io::BufReader<std::io::Cursor<Vec<u8>>>>();}",
            true,
        ),
        (
            "stdin_not_send",
            "fn yes<T:Send>(){}fn main(){yes::<std::io::StdinLock<'static>>();}",
            false,
        ),
        (
            "stdout_not_send",
            "fn yes<T:Send>(){}fn main(){yes::<std::io::StdoutLock<'static>>();}",
            false,
        ),
        (
            "stderr_not_send",
            "fn yes<T:Send>(){}fn main(){yes::<std::io::StderrLock<'static>>();}",
            false,
        ),
        (
            "buffer_escape",
            "use std::io::{BufRead,BufReader,Cursor};fn bad(mut r:BufReader<Cursor<Vec<u8>>>)->&'static[u8]{r.fill_buf().unwrap()}fn main(){}",
            false,
        ),
        (
            "buffer_mutation_conflict",
            "use std::io::{BufRead,BufReader,Cursor};fn main(){let mut r=BufReader::new(Cursor::new(b\"abc\"));let b=r.fill_buf().unwrap();r.consume(1);assert_eq!(b.len(),3);}",
            false,
        ),
        (
            "take_moves_owner",
            "use std::io::{Read,Cursor};fn main(){let c=Cursor::new(b\"abc\");let _r=c.take(1);let _=c.position();}",
            false,
        ),
        (
            "bytes_result_items",
            "use std::io::{Read,Cursor};fn main(){let _:Vec<u8>=Cursor::new(b\"abc\").bytes().collect();}",
            false,
        ),
        (
            "take_requires_bufread",
            "use std::io::{Read,BufRead};struct R;impl Read for R{fn read(&mut self,_:&mut[u8])->std::io::Result<usize>{Ok(0)}}fn main(){let mut r=R.take(1);let _=r.fill_buf();}",
            false,
        ),
        (
            "terminal_sealed",
            "struct Fake;impl std::io::IsTerminal for Fake{fn is_terminal(&self)->bool{true}}fn main(){}",
            false,
        ),
        (
            "terminal_reference_bound",
            "fn yes<T:std::io::IsTerminal>(){}fn main(){yes::<&std::fs::File>();}",
            false,
        ),
        (
            "bufreader_not_clone",
            "use std::io::{BufReader,Cursor};fn main(){let r=BufReader::new(Cursor::new(b\"x\"));let _=r.clone();}",
            false,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
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
    }
    std::fs::remove_dir_all(root).unwrap();
}
