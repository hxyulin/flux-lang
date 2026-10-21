//! The source files of a compilation, parsed, and flux's own `core`
//! library, which is built into the compiler.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use flux_syntax::Parse;

/// A file of the compilation, an index into [`SourceDb`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

impl FileId {
    /// Not yet known: diagnostics are made with this and given their file
    /// by whoever knows it.
    pub const UNKNOWN: FileId = FileId(u32::MAX);

    /// `core`'s root file, `lib.rs`, the first file of every [`SourceDb`].
    pub const CORE_ROOT: FileId = FileId(0);
}

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    pub parse: Parse,
}

/// Every file read so far. `core`'s files, under [`CORE_ROOT`], come
/// from the copy of `library/core` built into flux rather than from
/// disk. They are parsed once per process and are the first files of
/// every `SourceDb`, so their ids are the same in every compilation.
#[derive(Debug)]
pub struct SourceDb {
    files: Vec<Arc<SourceFile>>,
}

/// Where `core`'s files appear to be, in paths and messages.
pub const CORE_ROOT: &str = "<core>";

/// `library/core/src`, relative to [`CORE_ROOT`].
const CORE_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/core/src/lib.rs")),
    (
        "clone.rs",
        include_str!("../../../library/core/src/clone.rs"),
    ),
    ("cmp.rs", include_str!("../../../library/core/src/cmp.rs")),
    (
        "convert.rs",
        include_str!("../../../library/core/src/convert.rs"),
    ),
    (
        "marker.rs",
        include_str!("../../../library/core/src/marker.rs"),
    ),
    ("mem.rs", include_str!("../../../library/core/src/mem.rs")),
    ("num.rs", include_str!("../../../library/core/src/num.rs")),
    ("ops.rs", include_str!("../../../library/core/src/ops.rs")),
    (
        "prelude.rs",
        include_str!("../../../library/core/src/prelude.rs"),
    ),
];

/// `core`'s files, in [`CORE_FILES`] order, parsed in parallel the first
/// time they are needed.
pub(crate) fn core_files() -> &'static [Arc<SourceFile>] {
    static FILES: OnceLock<Box<[Arc<SourceFile>]>> = OnceLock::new();
    FILES.get_or_init(|| {
        let root = Path::new(CORE_ROOT);
        std::thread::scope(|scope| {
            let threads: Vec<_> = CORE_FILES
                .iter()
                .map(|&(name, text)| {
                    scope.spawn(move || SourceFile {
                        path: root.join(name),
                        text: text.to_owned(),
                        parse: flux_syntax::parse_source_file(text),
                    })
                })
                .collect();
            threads
                .into_iter()
                .map(|t| Arc::new(t.join().expect("parsing never panics")))
                .collect()
        })
    })
}

impl Default for SourceDb {
    fn default() -> SourceDb {
        SourceDb::new()
    }
}

impl SourceDb {
    /// A database of `core`'s files only.
    pub fn new() -> SourceDb {
        SourceDb {
            files: core_files().to_vec(),
        }
    }

    /// Parses `text` as the file at `path`.
    pub fn add(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> FileId {
        let text = text.into();
        let parse = flux_syntax::parse_source_file(&text);
        self.files.push(Arc::new(SourceFile {
            path: path.into(),
            text,
            parse,
        }));
        FileId(self.files.len() as u32 - 1)
    }

    /// The file at `path`, read and parsed if it was not already.
    pub fn load(&mut self, path: &Path) -> Result<FileId, String> {
        if let Some(id) = self.files.iter().position(|f| f.path == path) {
            return Ok(FileId(id as u32));
        }
        // `core`'s files are all here already.
        if path.starts_with(CORE_ROOT) {
            return Err(format!("{} is not part of core", path.display()));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Ok(self.add(path, text))
    }

    /// Whether `path` exists, on disk or in `core`.
    pub fn exists(&self, path: &Path) -> bool {
        match path.strip_prefix(CORE_ROOT) {
            Ok(relative) => CORE_FILES
                .iter()
                .any(|(name, _)| Path::new(name) == relative),
            Err(_) => path.is_file(),
        }
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    /// The file, shared, so it can be read while more files are loaded.
    pub fn shared(&self, id: FileId) -> Arc<SourceFile> {
        Arc::clone(&self.files[id.0 as usize])
    }

    /// Every file's syntax errors.
    pub fn syntax_errors(&self) -> Vec<crate::Diagnostic> {
        self.files()
            .flat_map(|(id, f)| {
                f.parse.errors().iter().map(move |e| crate::Diagnostic {
                    file: id,
                    range: e.range,
                    message: e.message.clone(),
                })
            })
            .collect()
    }

    pub fn files(&self) -> impl Iterator<Item = (FileId, &SourceFile)> {
        self.files
            .iter()
            .enumerate()
            .map(|(i, f)| (FileId(i as u32), &**f))
    }
}
