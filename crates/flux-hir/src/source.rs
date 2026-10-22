//! Parsed compilation sources and the embedded core, alloc and std libraries.

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

    /// `std`'s root file, after `core` and `alloc`'s embedded files.
    pub fn std_root() -> FileId {
        FileId((CORE_FILES.len() + ALLOC_FILES.len()) as u32)
    }

    /// `alloc`'s root file, its `lib.rs`, after `core`'s files.
    pub fn alloc_root() -> FileId {
        FileId(CORE_FILES.len() as u32)
    }
}

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub edition: flux_syntax::Edition,
    /// The file's syntax tree, which holds its text.
    pub parse: Parse,
    /// For the items a macro call in item position expanded to, the
    /// file the call is in, which the tree's ranges are in.
    pub expansion_of: Option<FileId>,
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
/// Where `alloc`'s files appear to be.
pub const ALLOC_ROOT: &str = "<alloc>";
/// Where the bounded `std` facade's files appear to be.
pub const STD_ROOT: &str = "<std>";

const STD_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/std/src/lib.rs")),
    (
        "thread.rs",
        include_str!("../../../library/std/src/thread.rs"),
    ),
];

/// `library/alloc/src`, relative to [`ALLOC_ROOT`]: built into the
/// compiler, after `core`, as rustc's `alloc` is after its `core`.
const ALLOC_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/alloc/src/lib.rs")),
    (
        "macros.rs",
        include_str!("../../../library/alloc/src/macros.rs"),
    ),
    ("fmt.rs", include_str!("../../../library/alloc/src/fmt.rs")),
    (
        "alloc.rs",
        include_str!("../../../library/alloc/src/alloc.rs"),
    ),
    (
        "borrow.rs",
        include_str!("../../../library/alloc/src/borrow.rs"),
    ),
    (
        "boxed.rs",
        include_str!("../../../library/alloc/src/boxed.rs"),
    ),
    (
        "prelude.rs",
        include_str!("../../../library/alloc/src/prelude.rs"),
    ),
    (
        "raw_vec.rs",
        include_str!("../../../library/alloc/src/raw_vec.rs"),
    ),
    (
        "slice.rs",
        include_str!("../../../library/alloc/src/slice.rs"),
    ),
    ("str.rs", include_str!("../../../library/alloc/src/str.rs")),
    (
        "string.rs",
        include_str!("../../../library/alloc/src/string.rs"),
    ),
    ("vec.rs", include_str!("../../../library/alloc/src/vec.rs")),
];

/// `library/core/src`, relative to [`CORE_ROOT`].
const CORE_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/core/src/lib.rs")),
    (
        "macros.rs",
        include_str!("../../../library/core/src/macros.rs"),
    ),
    ("hint.rs", include_str!("../../../library/core/src/hint.rs")),
    ("ptr.rs", include_str!("../../../library/core/src/ptr.rs")),
    ("cell.rs", include_str!("../../../library/core/src/cell.rs")),
    ("hash.rs", include_str!("../../../library/core/src/hash.rs")),
    ("bool.rs", include_str!("../../../library/core/src/bool.rs")),
    (
        "borrow.rs",
        include_str!("../../../library/core/src/borrow.rs"),
    ),
    (
        "alloc.rs",
        include_str!("../../../library/core/src/alloc.rs"),
    ),
    (
        "ascii.rs",
        include_str!("../../../library/core/src/ascii.rs"),
    ),
    ("arch.rs", include_str!("../../../library/core/src/arch.rs")),
    ("char.rs", include_str!("../../../library/core/src/char.rs")),
    ("ffi.rs", include_str!("../../../library/core/src/ffi.rs")),
    ("str.rs", include_str!("../../../library/core/src/str.rs")),
    (
        "unicode.rs",
        include_str!("../../../library/core/src/unicode.rs"),
    ),
    (
        "array.rs",
        include_str!("../../../library/core/src/array.rs"),
    ),
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
    (
        "intrinsics.rs",
        include_str!("../../../library/core/src/intrinsics.rs"),
    ),
    (
        "default.rs",
        include_str!("../../../library/core/src/default.rs"),
    ),
    ("fmt.rs", include_str!("../../../library/core/src/fmt.rs")),
    (
        "fmt/builders.rs",
        include_str!("../../../library/core/src/fmt/builders.rs"),
    ),
    (
        "fmt/num.rs",
        include_str!("../../../library/core/src/fmt/num.rs"),
    ),
    (
        "fmt/rt.rs",
        include_str!("../../../library/core/src/fmt/rt.rs"),
    ),
    ("io.rs", include_str!("../../../library/core/src/io.rs")),
    ("iter.rs", include_str!("../../../library/core/src/iter.rs")),
    ("mem.rs", include_str!("../../../library/core/src/mem.rs")),
    ("num.rs", include_str!("../../../library/core/src/num.rs")),
    ("ops.rs", include_str!("../../../library/core/src/ops.rs")),
    (
        "ops/deref.rs",
        include_str!("../../../library/core/src/ops/deref.rs"),
    ),
    (
        "ops/function.rs",
        include_str!("../../../library/core/src/ops/function.rs"),
    ),
    (
        "ops/index.rs",
        include_str!("../../../library/core/src/ops/index.rs"),
    ),
    (
        "ops/range.rs",
        include_str!("../../../library/core/src/ops/range.rs"),
    ),
    (
        "ops/try_trait.rs",
        include_str!("../../../library/core/src/ops/try_trait.rs"),
    ),
    (
        "option.rs",
        include_str!("../../../library/core/src/option.rs"),
    ),
    (
        "panic.rs",
        include_str!("../../../library/core/src/panic.rs"),
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
    (
        "slice/sort.rs",
        include_str!("../../../library/core/src/slice/sort.rs"),
    ),
];

/// Embedded files in core, alloc, std order, parsed in
/// parallel the first time they are needed.
pub(crate) fn core_files() -> &'static [Arc<SourceFile>] {
    static FILES: OnceLock<Box<[Arc<SourceFile>]>> = OnceLock::new();
    FILES.get_or_init(|| {
        let core = CORE_FILES
            .iter()
            .map(|&(name, text)| (CORE_ROOT, name, text));
        let alloc = ALLOC_FILES
            .iter()
            .map(|&(name, text)| (ALLOC_ROOT, name, text));
        let std = STD_FILES.iter().map(|&(name, text)| (STD_ROOT, name, text));
        std::thread::scope(|scope| {
            let threads: Vec<_> = core
                .chain(alloc)
                .chain(std)
                .map(|(root, name, text)| {
                    scope.spawn(move || SourceFile {
                        path: Path::new(root).join(name),
                        parse: flux_syntax::parse_source_file(text),
                        edition: flux_syntax::Edition::E2024,
                        expansion_of: None,
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
    /// A database of all embedded library files and their item macro
    /// expansions, sharing the process-wide library cache.
    pub fn new() -> SourceDb {
        SourceDb::with_files(crate::Core::get().files.clone())
    }

    /// All embedded library sources before building the shared item tree.
    pub(crate) fn core_only() -> SourceDb {
        SourceDb::with_files(core_files().to_vec())
    }

    fn with_files(files: Vec<Arc<SourceFile>>) -> SourceDb {
        let by_path = (files.iter().enumerate())
            .filter(|(_, f)| f.expansion_of.is_none())
            .map(|(i, f)| (f.path.clone(), FileId(i as u32)))
            .collect();
        SourceDb { files, by_path }
    }

    /// Adds the items a macro call in item position in `file` expanded
    /// to, as a file of their own.
    pub(crate) fn add_expansion(&mut self, parse: flux_syntax::Parse, file: FileId) -> FileId {
        let id = FileId(self.files.len() as u32);
        let path = self.file(file).path.clone();
        self.files.push(Arc::new(SourceFile {
            path,
            parse,
            edition: self.file(file).edition,
            expansion_of: Some(file),
        }));
        id
    }

    /// The file `id` is part of: itself, or for a macro expansion, the
    /// file of the call it expands.
    pub fn real_file(&self, id: FileId) -> FileId {
        self.file(id).expansion_of.unwrap_or(id)
    }

    /// The files read so far, shared.
    pub(crate) fn all_files(&self) -> Vec<Arc<SourceFile>> {
        self.files.clone()
    }

    /// Parses `text` as the file at `path`.
    pub fn add(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> FileId {
        self.add_with_edition(path, text, flux_syntax::Edition::E2024)
    }

    /// Adds a file under its crate edition.
    pub fn add_with_edition(
        &mut self,
        path: impl Into<PathBuf>,
        text: impl Into<String>,
        edition: flux_syntax::Edition,
    ) -> FileId {
        let text: String = text.into();
        let parse = flux_syntax::parse_source_text_with_edition(text.into_boxed_str(), edition);
        let id = FileId(self.files.len() as u32);
        let path = path.into();
        self.by_path.entry(path.clone()).or_insert(id);
        self.files.push(Arc::new(SourceFile {
            path,
            parse,
            edition,
            expansion_of: None,
        }));
        id
    }

    /// The file at `path`, read and parsed if it was not already.
    pub fn load(&mut self, path: &Path) -> Result<FileId, String> {
        self.load_with_edition(path, flux_syntax::Edition::E2024)
    }

    /// Loads a source file, rejecting reuse under a different edition.
    pub fn load_with_edition(
        &mut self,
        path: &Path,
        edition: flux_syntax::Edition,
    ) -> Result<FileId, String> {
        if let Some(&id) = self.by_path.get(path) {
            if self.file(id).edition != edition {
                return Err(
                    "source file used under different editions is not supported yet".into(),
                );
            }
            return Ok(id);
        }
        // All embedded library files are here already.
        if path.starts_with(CORE_ROOT) || path.starts_with(ALLOC_ROOT) || path.starts_with(STD_ROOT)
        {
            return Err(format!(
                "{} is not part of the embedded libraries",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Ok(self.add_with_edition(path, text, edition))
    }

    /// Whether `path` exists on disk or in an embedded library.
    pub fn exists(&self, path: &Path) -> bool {
        let built_in = |root: &str, files: &[(&str, &str)]| {
            path.strip_prefix(root)
                .ok()
                .map(|relative| files.iter().any(|(name, _)| Path::new(name) == relative))
        };
        built_in(CORE_ROOT, CORE_FILES)
            .or_else(|| built_in(ALLOC_ROOT, ALLOC_FILES))
            .or_else(|| built_in(STD_ROOT, STD_FILES))
            .unwrap_or_else(|| path.is_file())
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
