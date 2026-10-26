//! Exact source inventories and physical diagnostic locations.
use flux_hir::{Diagnostic, SourceDb};
use std::path::{Path, PathBuf};
pub type Files = Vec<(PathBuf, Vec<u8>)>;

pub fn collect(directory: &Path, files: &mut Files) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .expect("read source directory")
        .map(|entry| entry.expect("read source entry"))
        .collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if entry.file_type().expect("read source file type").is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let bytes = std::fs::read(&path).expect("read original source bytes");
            files.push((path, bytes));
        }
    }
}

pub fn changed(files: &[(PathBuf, Vec<u8>)]) -> Vec<&PathBuf> {
    files
        .iter()
        .filter(|(path, bytes)| std::fs::read(path).ok().as_ref() != Some(bytes))
        .map(|(path, _)| path)
        .collect()
}

pub fn report(root: &Path, label: &str, db: &SourceDb, errors: &[Diagnostic]) {
    use std::fmt::Write;
    let mut text = format!("count={}\n", errors.len());
    for error in errors {
        let file = db.file(error.file);
        let bytes = file.text().as_bytes();
        let line = bytes[..(error.range.start() as usize).min(bytes.len())]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count()
            + 1;
        writeln!(
            text,
            "{}:{line}:{:?}: {}",
            file.path.display(),
            error.range,
            error.message
        )
        .expect("write diagnostic string");
    }
    std::fs::write(root.join(format!("{label}.diagnostics")), text)
        .expect("write diagnostic artifact");
}
