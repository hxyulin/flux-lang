//! Parsed compilation sources and the embedded core, alloc and std libraries.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use flux_syntax::Parse;

/// Extra predicates for one source crate. Features use ordinary key/value
/// entries, for example `(Symbol::intern("feature"), Symbol::intern("std"))`.
/// Target predicates remain the compiler's fixed target configuration.
#[derive(Debug, Clone, Default)]
pub struct CrateCfg {
    pub names: Vec<flux_intern::Symbol>,
    pub values: Vec<(flux_intern::Symbol, flux_intern::Symbol)>,
}

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

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub(crate) diagnostic_text: Option<Arc<str>>,
    pub path: PathBuf,
    pub edition: flux_syntax::Edition,
    /// Included fragments occupy distinct coordinates while semantic passes
    /// combine them with their caller's body. Diagnostics resolve these back
    /// to byte offsets in this physical file.
    pub(crate) range_base: u32,
    /// The file's syntax tree, which holds its text.
    pub parse: Parse,
    /// For the items a macro call in item position expanded to, the
    /// file the call is in, which the tree's ranges are in.
    pub expansion_of: Option<FileId>,
}

impl SourceFile {
    pub(crate) fn contains_range(&self, range: flux_syntax::TextRange) -> bool {
        self.range_base != 0
            && range.start() >= self.range_base
            && range.end() <= self.range_base + self.text().len() as u32
    }

    pub(crate) fn local_range(&self, range: flux_syntax::TextRange) -> flux_syntax::TextRange {
        flux_syntax::TextRange::new(
            range.start() - self.range_base,
            range.end() - self.range_base,
        )
    }

    pub fn text(&self) -> &str {
        self.diagnostic_text
            .as_deref()
            .unwrap_or_else(|| self.parse.tree().text())
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
    next_include_offset: u32,
    compilation_dirs: rustc_hash::FxHashMap<FileId, PathBuf>,
    proc_macros: rustc_hash::FxHashMap<FileId, Vec<crate::ProcMacro>>,
    metadata_proc_macros: bool,
    environments: rustc_hash::FxHashMap<FileId, Arc<std::collections::BTreeMap<String, String>>>,
    configured: rustc_hash::FxHashMap<(FileId, Option<Arc<flux_syntax::CfgOptions>>), FileId>,
    /// Applied to crate roots configured after it is set; on by default.
    debug_assertions: bool,
    panic_strategy: flux_syntax::PanicStrategy,
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
        "panic.rs",
        include_str!("../../../library/std/src/panic.rs"),
    ),
    (
        "io/buffered.rs",
        include_str!("../../../library/std/src/io/buffered.rs"),
    ),
    (
        "io/stdio.rs",
        include_str!("../../../library/std/src/io/stdio.rs"),
    ),
    ("net.rs", include_str!("../../../library/std/src/net.rs")),
    ("os.rs", include_str!("../../../library/std/src/os.rs")),
    (
        "process.rs",
        include_str!("../../../library/std/src/process.rs"),
    ),
    (
        "process/termination.rs",
        include_str!("../../../library/std/src/process/termination.rs"),
    ),
    (
        "process/command.rs",
        include_str!("../../../library/std/src/process/command.rs"),
    ),
    ("arch.rs", include_str!("../../../library/std/src/arch.rs")),
    (
        "float.rs",
        include_str!("../../../library/std/src/float.rs"),
    ),
    ("hash.rs", include_str!("../../../library/std/src/hash.rs")),
    ("env.rs", include_str!("../../../library/std/src/env.rs")),
    ("ffi.rs", include_str!("../../../library/std/src/ffi.rs")),
    (
        "ffi/os_str.rs",
        include_str!("../../../library/std/src/ffi/os_str.rs"),
    ),
    ("path.rs", include_str!("../../../library/std/src/path.rs")),
    ("fs.rs", include_str!("../../../library/std/src/fs.rs")),
    ("io.rs", include_str!("../../../library/std/src/io.rs")),
    ("time.rs", include_str!("../../../library/std/src/time.rs")),
    (
        "collections.rs",
        include_str!("../../../library/std/src/collections.rs"),
    ),
    (
        "collections/hash_map.rs",
        include_str!("../../../library/std/src/collections/hash_map.rs"),
    ),
    (
        "collections/hash_set.rs",
        include_str!("../../../library/std/src/collections/hash_set.rs"),
    ),
    (
        "collections/vec_deque.rs",
        include_str!("../../../library/std/src/collections/vec_deque.rs"),
    ),
    (
        "macros.rs",
        include_str!("../../../library/std/src/macros.rs"),
    ),
    (
        "thread/local.rs",
        include_str!("../../../library/std/src/thread/local.rs"),
    ),
    (
        "thread/scoped.rs",
        include_str!("../../../library/std/src/thread/scoped.rs"),
    ),
    (
        "thread.rs",
        include_str!("../../../library/std/src/thread.rs"),
    ),
    (
        "sync/once.rs",
        include_str!("../../../library/std/src/sync/once.rs"),
    ),
    (
        "sync/once_lock.rs",
        include_str!("../../../library/std/src/sync/once_lock.rs"),
    ),
    (
        "sync/lazy_lock.rs",
        include_str!("../../../library/std/src/sync/lazy_lock.rs"),
    ),
    (
        "sync/mutex.rs",
        include_str!("../../../library/std/src/sync/mutex.rs"),
    ),
    (
        "sync/rwlock.rs",
        include_str!("../../../library/std/src/sync/rwlock.rs"),
    ),
    (
        "sync/poison.rs",
        include_str!("../../../library/std/src/sync/poison.rs"),
    ),
    (
        "sync/mpsc.rs",
        include_str!("../../../library/std/src/sync/mpsc.rs"),
    ),
];

/// `library/alloc/src`, relative to [`ALLOC_ROOT`]: built into the
/// compiler, after `core`, as rustc's `alloc` is after its `core`.
const ALLOC_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/alloc/src/lib.rs")),
    (
        "collections/vec_deque.rs",
        include_str!("../../../library/alloc/src/collections/vec_deque.rs"),
    ),
    ("ffi.rs", include_str!("../../../library/alloc/src/ffi.rs")),
    ("rc.rs", include_str!("../../../library/alloc/src/rc.rs")),
    (
        "collections.rs",
        include_str!("../../../library/alloc/src/collections.rs"),
    ),
    (
        "collections/btree_map.rs",
        include_str!("../../../library/alloc/src/collections/btree_map.rs"),
    ),
    (
        "collections/btree_set.rs",
        include_str!("../../../library/alloc/src/collections/btree_set.rs"),
    ),
    (
        "collections/binary_heap.rs",
        include_str!("../../../library/alloc/src/collections/binary_heap.rs"),
    ),
    (
        "collections/linked_list.rs",
        include_str!("../../../library/alloc/src/collections/linked_list.rs"),
    ),
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
        "str/lowercase.rs",
        include_str!("../../../library/alloc/src/str/lowercase.rs"),
    ),
    (
        "str/lowercase_data.rs",
        include_str!("../../../library/alloc/src/str/lowercase_data.rs"),
    ),
    (
        "string.rs",
        include_str!("../../../library/alloc/src/string.rs"),
    ),
    ("vec.rs", include_str!("../../../library/alloc/src/vec.rs")),
    (
        "sync.rs",
        include_str!("../../../library/alloc/src/sync.rs"),
    ),
    (
        "task.rs",
        include_str!("../../../library/alloc/src/task.rs"),
    ),
];

/// `library/core/src`, relative to [`CORE_ROOT`].
const CORE_FILES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../library/core/src/lib.rs")),
    ("time.rs", include_str!("../../../library/core/src/time.rs")),
    (
        "num/saturating.rs",
        include_str!("../../../library/core/src/num/saturating.rs"),
    ),
    (
        "num/wrapping.rs",
        include_str!("../../../library/core/src/num/wrapping.rs"),
    ),
    (
        "float/decimal.rs",
        include_str!("../../../library/core/src/float/decimal.rs"),
    ),
    (
        "float.rs",
        include_str!("../../../library/core/src/float.rs"),
    ),
    (
        "char_properties.rs",
        include_str!("../../../library/core/src/char_properties.rs"),
    ),
    (
        "sync/mod.rs",
        include_str!("../../../library/core/src/sync/mod.rs"),
    ),
    (
        "sync/atomic.rs",
        include_str!("../../../library/core/src/sync/atomic.rs"),
    ),
    ("any.rs", include_str!("../../../library/core/src/any.rs")),
    (
        "macros.rs",
        include_str!("../../../library/core/src/macros.rs"),
    ),
    ("hint.rs", include_str!("../../../library/core/src/hint.rs")),
    ("pin.rs", include_str!("../../../library/core/src/pin.rs")),
    (
        "future.rs",
        include_str!("../../../library/core/src/future.rs"),
    ),
    ("task.rs", include_str!("../../../library/core/src/task.rs")),
    (
        "error.rs",
        include_str!("../../../library/core/src/error.rs"),
    ),
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
    (
        "runtime.rs",
        include_str!("../../../library/core/src/runtime.rs"),
    ),
    (
        "arch/x86_64.rs",
        include_str!("../../../library/core/src/arch/x86_64.rs"),
    ),
    (
        "char/case.rs",
        include_str!("../../../library/core/src/char/case.rs"),
    ),
    (
        "char/case_data.rs",
        include_str!("../../../library/core/src/char/case_data.rs"),
    ),
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
                        diagnostic_text: None,
                        path: Path::new(root).join(name),
                        parse: flux_syntax::parse_source_file(text),
                        edition: flux_syntax::Edition::E2024,
                        expansion_of: None,
                        range_base: 0,
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

    /// An empty database for explicitly trusted, precompiled library metadata.
    pub fn empty() -> SourceDb {
        SourceDb::with_files(Vec::new())
    }

    /// Forks the database while sharing immutable diagnostic sources without parsing.
    pub fn fork(&self) -> SourceDb {
        SourceDb {
            files: self.files.clone(),
            by_path: self.by_path.clone(),
            next_include_offset: self.next_include_offset,
            compilation_dirs: self.compilation_dirs.clone(),
            proc_macros: self.proc_macros.clone(),
            metadata_proc_macros: self.metadata_proc_macros,
            environments: self.environments.clone(),
            configured: self.configured.clone(),
            debug_assertions: self.debug_assertions,
            panic_strategy: self.panic_strategy,
        }
    }

    /// All embedded library sources before building the shared item tree.
    pub(crate) fn core_only() -> SourceDb {
        SourceDb::with_files(core_files().to_vec())
    }

    pub(crate) fn with_files(files: Vec<Arc<SourceFile>>) -> SourceDb {
        let by_path = (files.iter().enumerate())
            .filter(|(_, f)| f.expansion_of.is_none())
            .map(|(i, f)| (f.path.clone(), FileId(i as u32)))
            .collect();
        let next_include_offset = files
            .iter()
            .map(|f| f.range_base + f.text().len() as u32 + 1)
            .max()
            .unwrap_or(0)
            .max(1 << 30);
        SourceDb {
            files,
            by_path,
            next_include_offset,
            compilation_dirs: Default::default(),
            proc_macros: Default::default(),
            metadata_proc_macros: false,
            environments: Default::default(),
            configured: Default::default(),
            debug_assertions: true,
            panic_strategy: flux_syntax::PanicStrategy::Abort,
        }
    }

    /// Sets `cfg(debug_assertions)`, rustc's `-C debug-assertions`, for every
    /// crate root configured afterwards with [`SourceDb::set_crate_cfg`].
    /// Unconfigured roots and the embedded libraries keep it on.
    pub fn set_debug_assertions(&mut self, enabled: bool) {
        self.debug_assertions = enabled;
    }

    /// Sets `cfg(panic)` for crate roots configured afterwards.
    pub fn set_panic_strategy(&mut self, strategy: flux_syntax::PanicStrategy) {
        self.panic_strategy = strategy;
    }

    /// Sets the compiler invocation directory used by host procedural macros.
    /// This is distinct from a package's manifest directory and include paths.
    pub fn set_crate_directory(&mut self, root: FileId, directory: PathBuf) -> Result<(), String> {
        if self.files.get(root.0 as usize).is_none() {
            return Err("unknown crate source".into());
        }
        if root.0 < FileId::std_root().0 || self.file(root).path.starts_with(STD_ROOT) {
            return Err("expected a physical crate root".into());
        }
        self.compilation_dirs.insert(root, directory);
        Ok(())
    }
    pub(crate) fn crate_directory(&self, root: FileId) -> Option<PathBuf> {
        self.compilation_dirs.get(&root).cloned()
    }

    /// Registers a host-only macro crate. Its exports are already compiled;
    /// the dylib is not parsed or presented as target Rust source.
    pub fn add_proc_macro_crate(
        &mut self,
        path: PathBuf,
        exports: Vec<crate::ProcMacro>,
    ) -> FileId {
        let root = self.add(path, "");
        self.proc_macros.insert(root, exports);
        root
    }

    pub(crate) fn note_metadata_proc_macros(&mut self) {
        self.metadata_proc_macros = true;
    }

    pub(crate) fn has_proc_macros(&self) -> bool {
        self.metadata_proc_macros || !self.proc_macros.is_empty()
    }

    pub(crate) fn proc_macros(&self, root: FileId) -> Option<&[crate::ProcMacro]> {
        self.proc_macros.get(&root).map(Vec::as_slice)
    }

    /// Sets an isolated compile-time environment for one physical crate root.
    /// An explicit empty map hides the process environment. Without a map,
    /// standalone compilations retain their process-environment behavior.
    /// Call this before lowering; modules, includes and macro invocations use
    /// the calling crate's snapshot, not the macro definition's environment.
    pub fn set_crate_env(
        &mut self,
        root: FileId,
        environment: std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        let source = self
            .files
            .get(root.0 as usize)
            .ok_or_else(|| "unknown crate source".to_owned())?;
        if source.expansion_of.is_some()
            || source.path.starts_with(CORE_ROOT)
            || source.path.starts_with(ALLOC_ROOT)
            || source.path.starts_with(STD_ROOT)
        {
            return Err("expected a physical crate root".into());
        }
        self.environments.insert(root, Arc::new(environment));
        Ok(())
    }

    pub(crate) fn crate_env(
        &self,
        root: FileId,
    ) -> Option<Arc<std::collections::BTreeMap<String, String>>> {
        self.environments.get(&root).cloned()
    }

    /// Sets a physical root's compilation inputs before lowering its source
    /// graph. Embedded libraries keep their own configuration and cannot be
    /// changed through a user crate's inputs.
    pub fn set_crate_cfg(&mut self, root: FileId, cfg: CrateCfg) -> Result<(), String> {
        let source = self
            .files
            .get(root.0 as usize)
            .ok_or_else(|| "unknown crate source".to_owned())?;
        if source.expansion_of.is_some()
            || source.path.starts_with(CORE_ROOT)
            || source.path.starts_with(ALLOC_ROOT)
            || source.path.starts_with(STD_ROOT)
        {
            return Err("expected a physical crate root".into());
        }
        let mut names: Vec<Box<str>> = cfg.names.iter().map(|name| name.as_str().into()).collect();
        let mut values: Vec<(Box<str>, Box<str>)> = cfg
            .values
            .iter()
            .map(|(name, value)| (name.as_str().into(), value.as_str().into()))
            .collect();
        names.sort();
        names.dedup();
        values.sort();
        values.dedup();
        let cfg = (!names.is_empty()
            || !values.is_empty()
            || !self.debug_assertions
            || self.panic_strategy != flux_syntax::PanicStrategy::Abort)
            .then(|| {
                Arc::new(flux_syntax::CfgOptions {
                    names: names.into(),
                    values: values.into(),
                    debug_assertions: self.debug_assertions,
                    panic_strategy: self.panic_strategy,
                })
            });
        let mut source = (**source).clone();
        source.parse = source.parse.with_cfg_options(cfg);
        self.files[root.0 as usize] = Arc::new(source);
        self.configured.retain(|(file, _), _| *file != root);
        Ok(())
    }

    /// A module shared physically by several crates needs one syntax identity
    /// for each configuration. Its text, path, edition and ranges stay intact.
    pub(crate) fn configured_file(
        &mut self,
        file: FileId,
        cfg: Option<Arc<flux_syntax::CfgOptions>>,
    ) -> FileId {
        if self.file(file).parse.tree().cfg_options() == cfg.as_ref() {
            return file;
        }
        if let Some(&id) = self.configured.get(&(file, cfg.clone())) {
            return id;
        }
        let mut source = self.file(file).clone();
        source.parse = source.parse.with_cfg_options(cfg.clone());
        let id = FileId(self.files.len() as u32);
        self.files.push(Arc::new(source));
        self.configured.insert((file, cfg), id);
        id
    }

    /// Adds the items a macro call in item position in `file` expanded
    /// to, as a file of their own.
    pub(crate) fn add_expansion(&mut self, parse: flux_syntax::Parse, file: FileId) -> FileId {
        let id = FileId(self.files.len() as u32);
        let path = self.file(file).path.clone();
        self.files.push(Arc::new(SourceFile {
            diagnostic_text: None,
            path,
            parse,
            edition: self.file(file).edition,
            expansion_of: Some(file),
            range_base: 0,
        }));
        id
    }

    /// Retains an included file's own path and ranges. Each inclusion gets a
    /// syntax identity because its fragment and lexical scope may differ.
    pub(crate) fn add_inclusion(
        &mut self,
        parse: Parse,
        path: PathBuf,
        edition: flux_syntax::Edition,
    ) -> Result<FileId, String> {
        let id = FileId(self.files.len() as u32);
        let range_base = self.next_include_offset;
        let size = u32::try_from(parse.tree().text().len())
            .ok()
            .and_then(|size| size.checked_add(1));
        self.next_include_offset = size
            .and_then(|size| range_base.checked_add(size))
            .ok_or_else(|| "included source size limit exceeded".to_owned())?;
        let parse = parse.with_source_offset(range_base);
        self.files.push(Arc::new(SourceFile {
            diagnostic_text: None,
            path,
            parse,
            edition,
            expansion_of: None,
            range_base,
        }));
        Ok(id)
    }

    /// The file `id` is part of: itself, or for a macro expansion, the
    /// file of the call it expands.
    pub fn real_file(&self, id: FileId) -> FileId {
        self.file(id).expansion_of.unwrap_or(id)
    }

    /// Restores a diagnostic's physical file and byte offsets after lowering.
    pub fn resolve_diagnostic(&self, diagnostic: &mut crate::Diagnostic) {
        if let Some((file, source)) = self
            .files()
            .find(|(_, source)| source.contains_range(diagnostic.range))
        {
            diagnostic.file = file;
            diagnostic.range = source.local_range(diagnostic.range);
        }
        diagnostic.file = self.real_file(diagnostic.file);
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
            diagnostic_text: None,
            path,
            parse,
            edition,
            expansion_of: None,
            range_base: 0,
        }));
        id
    }

    /// Adds artifact source text for diagnostics without parsing dependency code.
    pub fn add_metadata_source(
        &mut self,
        path: PathBuf,
        text: String,
        edition: flux_syntax::Edition,
    ) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(Arc::new(SourceFile {
            diagnostic_text: Some(Arc::from(text.into_boxed_str())),
            path,
            edition,
            range_base: 0,
            parse: flux_syntax::parse_source_file(""),
            expansion_of: None,
        }));
        id
    }

    /// Assigns an included artifact source fresh coordinates in this compilation.
    pub(crate) fn allocate_metadata_include(
        &mut self,
        id: FileId,
    ) -> Result<u32, crate::flib::Error> {
        let size = u32::try_from(self.file(id).text().len())
            .map_err(|_| crate::flib::Error("included source is too large"))?;
        let base = self.next_include_offset;
        self.next_include_offset = base
            .checked_add(size)
            .and_then(|end| end.checked_add(1))
            .ok_or(crate::flib::Error("included source coordinate overflow"))?;
        Arc::make_mut(&mut self.files[id.0 as usize]).range_base = base;
        Ok(base)
    }

    /// Restores diagnostic coordinates after artifact-local file IDs are remapped.
    pub(crate) fn set_metadata_origin(
        &mut self,
        id: FileId,
        range_base: u32,
        expansion_of: Option<FileId>,
    ) {
        let file = Arc::make_mut(&mut self.files[id.0 as usize]);
        file.range_base = range_base;
        file.expansion_of = expansion_of;
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
        let mut text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        // Like rustc, ignore a leading byte order mark. Diagnostics take
        // their offsets from the stored text, which no longer has it.
        if text.starts_with('\u{feff}') {
            text.remove(0);
        }
        Ok(self.add_with_edition(path, text, edition))
    }

    /// Resolves data and source includes against their physical containing file.
    pub(crate) fn include_path(source: &SourceFile, filename: &str) -> Result<PathBuf, String> {
        if source.expansion_of.is_some()
            || source.path.starts_with(CORE_ROOT)
            || source.path.starts_with(ALLOC_ROOT)
            || source.path.starts_with(STD_ROOT)
        {
            return Err("includes from virtual sources are not supported yet".into());
        }
        let parent = source
            .path
            .parent()
            .ok_or_else(|| "include source directory is unknown".to_owned())?;
        Ok(parent.join(filename))
    }

    /// Reads bytes unchanged, including BOM, CRLF and non-UTF-8 data.
    pub(crate) fn read_include_bytes(
        source: &SourceFile,
        filename: &str,
    ) -> Result<Vec<u8>, String> {
        let path = Self::include_path(source, filename)?;
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
    }

    pub(crate) fn read_include_str(source: &SourceFile, filename: &str) -> Result<String, String> {
        let path = Self::include_path(source, filename)?;
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
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
                    range: if f.contains_range(e.range) {
                        f.local_range(e.range)
                    } else {
                        e.range
                    },
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
