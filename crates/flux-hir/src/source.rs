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
    /// The file's syntax tree, which holds its text.
    pub parse: Parse,
}

impl SourceFile {
    pub fn text(&self) -> &str {
        self.parse.tree().text()
    }
}

/// Every file read so far. `core`'s files, under [`CORE_ROOT`], come
/// from the copy of `library/core` built into flux rather than from
/// disk. They are parsed once per process and are the first files of
/// every `SourceDb`, so their ids are the same in every compilation.
#[derive(Debug)]
pub struct SourceDb {
    files: Vec<Arc<SourceFile>>,
    /// The first file at each path: crates can have thousands of files.
    by_path: rustc_hash::FxHashMap<PathBuf, FileId>,
}

/// Where `core`'s files appear to be, in paths and messages.
pub const CORE_ROOT: &str = "<core>";

/// `library/core/src`, relative to [`CORE_ROOT`].
const CORE_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/core/src/lib.rs")),
    (
        "ascii.rs",
        include_str!("../../../library/core/src/ascii.rs"),
    ),
    ("char.rs", include_str!("../../../library/core/src/char.rs")),
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
    ("iter.rs", include_str!("../../../library/core/src/iter.rs")),
    ("mem.rs", include_str!("../../../library/core/src/mem.rs")),
    ("num.rs", include_str!("../../../library/core/src/num.rs")),
    ("ops.rs", include_str!("../../../library/core/src/ops.rs")),
    (
        "ops/function.rs",
        include_str!("../../../library/core/src/ops/function.rs"),
    ),
    (
        "ops/range.rs",
        include_str!("../../../library/core/src/ops/range.rs"),
    ),
    (
        "option.rs",
        include_str!("../../../library/core/src/option.rs"),
    ),
    (
        "panicking.rs",
        include_str!("../../../library/core/src/panicking.rs"),
    ),
    (
        "prelude.rs",
        include_str!("../../../library/core/src/prelude.rs"),
    ),
    (
        "result.rs",
        include_str!("../../../library/core/src/result.rs"),
    ),
    (
        "slice.rs",
        include_str!("../../../library/core/src/slice.rs"),
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
        let files = core_files().to_vec();
        let by_path = (files.iter().enumerate())
            .map(|(i, f)| (f.path.clone(), FileId(i as u32)))
            .collect();
        SourceDb { files, by_path }
    }

    /// Parses `text` as the file at `path`.
    pub fn add(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> FileId {
        let text: String = text.into();
        let parse = flux_syntax::parse_source_text(text.into_boxed_str());
        let id = FileId(self.files.len() as u32);
        let path = path.into();
        self.by_path.entry(path.clone()).or_insert(id);
        self.files.push(Arc::new(SourceFile { path, parse }));
        id
    }

    /// The file at `path`, read and parsed if it was not already.
    pub fn load(&mut self, path: &Path) -> Result<FileId, String> {
        if let Some(&id) = self.by_path.get(path) {
            return Ok(id);
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
