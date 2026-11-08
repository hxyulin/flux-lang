//! Native libraries for the executable link: rustc's `-l` and `-L`
//! requests, resolved to files the way the platform linker resolves them.

use crate::{Error, Stage};
use flux_hir::{NativeKind, NativeLibrary};
use std::path::PathBuf;

/// The native libraries and search directories of one executable link.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeLink {
    /// Libraries in request order, each once.
    pub libraries: Vec<NativeLibrary>,
    /// Directories searched for `-l` libraries: `-L native=` and `-L all=`.
    pub search_paths: Vec<PathBuf>,
    /// Directories searched for frameworks: `-L framework=`.
    pub framework_paths: Vec<PathBuf>,
}

/// Files and resolved platform semantics consumed by a native executable link.
///
/// Callers can hash `files` and retain `context` alongside an artifact. Resolve
/// again before reuse: a newly created earlier search candidate can change the
/// selected files even when the old files still exist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeLinkInputs {
    /// Files in the link's input order, including platform runtime files.
    pub files: Vec<PathBuf>,
    /// Canonical platform and dynamic-library interface data used by the linker.
    pub context: Vec<u8>,
}

impl NativeLinkInputs {
    pub(crate) fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.context
            .extend_from_slice(&(value.len() as u64).to_le_bytes());
        self.context.extend_from_slice(value);
    }

    pub(crate) fn contents(&mut self, bytes: &[u8]) {
        self.context
            .extend_from_slice(&flux_hir::flib::digest(bytes));
    }

    pub(crate) fn dylib(&mut self, dylib: &keel_link::Dylib) {
        self.string(&dylib.install_name);
        self.context
            .extend_from_slice(&(dylib.exports.len() as u64).to_le_bytes());
        for symbol in &dylib.exports {
            self.string(symbol);
        }
        self.context
            .extend_from_slice(&(dylib.weak_exports.len() as u64).to_le_bytes());
        for symbol in &dylib.weak_exports {
            self.string(symbol);
        }
    }
}

impl NativeLink {
    /// Libraries alone, without search directories.
    pub fn libraries(libraries: &[NativeLibrary]) -> Self {
        let mut link = Self::default();
        for library in libraries {
            link.add_library(*library);
        }
        link
    }

    pub fn add_library(&mut self, library: NativeLibrary) {
        if !self.libraries.contains(&library) {
            self.libraries.push(library);
        }
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    pub fn add_framework_path(&mut self, path: PathBuf) {
        if !self.framework_paths.contains(&path) {
            self.framework_paths.push(path);
        }
    }

    /// Adds `other`'s libraries and directories after this link's own.
    pub fn extend(&mut self, other: &NativeLink) {
        for library in &other.libraries {
            self.add_library(*library);
        }
        for path in &other.search_paths {
            self.add_search_path(path.clone());
        }
        for path in &other.framework_paths {
            self.add_framework_path(path.clone());
        }
    }
}

pub(crate) struct DarwinLibraries {
    pub sdk: PathBuf,
    pub sdk_version: u32,
    pub dylibs: Vec<keel_link::Dylib>,
    pub archives: Vec<(PathBuf, Vec<u8>)>,
    pub files: Vec<PathBuf>,
}

impl DarwinLibraries {
    pub(crate) fn inputs(&self) -> NativeLinkInputs {
        let mut inputs = NativeLinkInputs::default();
        inputs.string("darwin-native-link-v2");
        inputs.bytes(self.sdk.as_os_str().as_encoded_bytes());
        inputs
            .context
            .extend_from_slice(&self.sdk_version.to_le_bytes());
        inputs.files = self.files.clone();
        for dylib in &self.dylibs {
            inputs.dylib(dylib);
        }
        inputs
            .context
            .extend_from_slice(&(self.archives.len() as u64).to_le_bytes());
        for (_, bytes) in &self.archives {
            inputs.contents(bytes);
        }
        inputs
    }
}

/// Resolve once with the same ordering for linking and freshness inspection.
pub(crate) fn darwin(native: &NativeLink) -> Result<DarwinLibraries, Error> {
    darwin_at(native, crate::macos_sdk()?)
}

fn darwin_at(native: &NativeLink, sdk: PathBuf) -> Result<DarwinLibraries, Error> {
    let system = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk)
        .map_err(|message| Error::message(Stage::Sdk, message))?;
    let mut files = vec![sdk.join("usr/lib/libSystem.tbd")];
    let mut dylibs = vec![system];
    let mut archives = Vec::new();
    let libraries = [sdk.join("usr/lib")];
    let frameworks = [sdk.join("System/Library/Frameworks")];
    for library in &native.libraries {
        // Darwin's libc and libm are umbrella aliases of libSystem,
        // already included for the executable's runtime entry point.
        if library.kind == flux_hir::NativeKind::Dylib
            && matches!(library.name.as_str(), "c" | "m" | "System")
        {
            continue;
        }
        // ld64's default `-search_paths_first` order.
        let found = find(
            *library,
            native,
            &libraries,
            &frameworks,
            &["tbd", "dylib", "so", "a"],
        )?;
        let path = match found {
            Found::Archive(path, bytes) => {
                if !archives.iter().any(|(p, _)| *p == path) {
                    files.push(path.clone());
                    archives.push((path, bytes));
                }
                continue;
            }
            Found::Interface(path) => path,
            Found::Shared(path) => {
                return Err(Error::message(
                    Stage::Link,
                    format!(
                        "linking the Mach-O shared library {} is not supported yet; \
                         the linker reads only `.tbd` interfaces",
                        path.display()
                    ),
                ));
            }
        };
        files.push(path.clone());
        let dylib = keel_link::Dylib::from_tbd(&path, &sdk)
            .map_err(|message| Error::message(Stage::Sdk, message))?;
        // Aliases such as libpthread name a library already present.
        if !dylibs.iter().any(|d| d.install_name == dylib.install_name) {
            dylibs.push(dylib);
        }
    }
    let defaults = keel_link::Options::default();
    let sdk_version = crate::sdk_version(&sdk).unwrap_or(defaults.minimum_macos);
    let settings = sdk.join("SDKSettings.json");
    if settings.is_file() {
        files.push(settings);
    }
    Ok(DarwinLibraries {
        sdk,
        sdk_version,
        dylibs,
        archives,
        files,
    })
}

/// A library file the link reads.
pub(crate) enum Found {
    /// A static archive: its path and bytes.
    Archive(PathBuf, Vec<u8>),
    /// A text-based dynamic library interface (`.tbd`).
    Interface(PathBuf),
    /// A shared library binary (`.so`, `.dylib`, or a framework binary).
    Shared(PathBuf),
}

/// Finds `library` the way rustc and the platform linker do: a static
/// library is `libNAME.a` in a search directory; a dylib request takes,
/// in each directory in turn, the first of `extensions` that exists; a
/// framework is `NAME.framework/NAME`, preferring its `.tbd` interface.
/// `system` and `frameworks` are searched after the request's directories.
pub(crate) fn find(
    library: NativeLibrary,
    link: &NativeLink,
    system: &[PathBuf],
    frameworks: &[PathBuf],
    extensions: &[&str],
) -> Result<Found, Error> {
    let name = library.name.as_str();
    let not_found = |message: String| Err(Error::message(Stage::Link, message));
    match library.kind {
        NativeKind::Static => {
            for directory in &link.search_paths {
                let path = directory.join(format!("lib{name}.a"));
                if path.is_file() {
                    return archive(path);
                }
            }
            not_found(format!(
                "could not find native static library `{name}`, perhaps an -L flag is missing?"
            ))
        }
        NativeKind::Dylib => {
            for directory in link.search_paths.iter().chain(system) {
                for extension in extensions {
                    let path = directory.join(format!("lib{name}.{extension}"));
                    if !path.is_file() {
                        continue;
                    }
                    return match *extension {
                        "a" => archive(path),
                        "tbd" => Ok(Found::Interface(path)),
                        _ => Ok(Found::Shared(path)),
                    };
                }
            }
            not_found(format!("cannot find native library `{name}`"))
        }
        NativeKind::Framework => {
            for directory in link.framework_paths.iter().chain(frameworks) {
                let bundle = directory.join(format!("{name}.framework"));
                let interface = bundle.join(format!("{name}.tbd"));
                if interface.is_file() {
                    return Ok(Found::Interface(interface));
                }
                let binary = bundle.join(name);
                if binary.is_file() {
                    return Ok(Found::Shared(binary));
                }
            }
            not_found(format!("cannot find framework `{name}`"))
        }
    }
}

fn archive(path: PathBuf) -> Result<Found, Error> {
    let bytes = std::fs::read(&path)
        .map_err(|e| Error::io(Stage::Link, &format!("cannot read {}", path.display()), e))?;
    Ok(Found::Archive(path, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_intern::Symbol;

    fn library(name: &str, kind: NativeKind) -> NativeLibrary {
        NativeLibrary {
            name: Symbol::intern(name),
            kind,
        }
    }

    #[test]
    fn requests_are_kept_once_in_order() {
        let mut link = NativeLink::default();
        link.add_library(library("b", NativeKind::Static));
        link.add_library(library("a", NativeKind::Dylib));
        link.add_library(library("b", NativeKind::Static));
        link.add_search_path("/x".into());
        link.add_search_path("/x".into());
        assert_eq!(
            link.libraries,
            [
                library("b", NativeKind::Static),
                library("a", NativeKind::Dylib)
            ]
        );
        assert_eq!(link.search_paths, [PathBuf::from("/x")]);
    }

    #[test]
    fn dylib_requests_take_the_first_extension_in_each_directory() {
        let root = std::env::temp_dir().join(format!("flux-native-{}", std::process::id()));
        let (first, second) = (root.join("first"), root.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("libz.a"), b"!<arch>\n").unwrap();
        std::fs::write(second.join("libz.tbd"), b"").unwrap();
        let link = NativeLink {
            search_paths: vec![first.clone(), second],
            ..NativeLink::default()
        };
        let found = find(
            library("z", NativeKind::Dylib),
            &link,
            &[],
            &[],
            &["tbd", "dylib", "a"],
        );
        assert!(matches!(found, Ok(Found::Archive(path, _)) if path == first.join("libz.a")));
        let missing = find(library("q", NativeKind::Static), &link, &[], &[], &[]);
        assert!(missing.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use flux_intern::Symbol;

    fn sdk(name: &str, reexport: bool) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("flux-link-inputs-{name}-{}", std::process::id()));
        std::fs::create_dir_all(root.join("usr/lib")).unwrap();
        let child = if reexport {
            "reexported-libraries:\n  - targets: [ arm64-macos ]\n    libraries: [ '/usr/lib/libChild.dylib' ]\n"
        } else {
            ""
        };
        std::fs::write(root.join("usr/lib/libSystem.tbd"), format!("--- !tapi-tbd\ntbd-version: 4\ntargets: [ arm64-macos ]\ninstall-name: '/usr/lib/libSystem.B.dylib'\n{child}")).unwrap();
        root
    }

    #[test]
    fn sdk_reexports_contribute_resolved_semantics() {
        let root = sdk("reexport", true);
        let path = root.join("usr/lib/libChild.tbd");
        let child = |symbol| {
            format!(
                "--- !tapi-tbd\ntbd-version: 4\ntargets: [ arm64-macos ]\ninstall-name: '/usr/lib/libChild.dylib'\nexports:\n  - targets: [ arm64-macos ]\n    weak-symbols: [ '{symbol}' ]\n"
            )
        };
        std::fs::write(&path, child("_first")).unwrap();
        let root_bytes = std::fs::read(root.join("usr/lib/libSystem.tbd")).unwrap();
        let before = darwin_at(&NativeLink::default(), root.clone()).unwrap();
        let mut before_context = NativeLinkInputs::default();
        for library in &before.dylibs {
            before_context.dylib(library);
        }
        std::fs::write(&path, child("_second")).unwrap();
        let after = darwin_at(&NativeLink::default(), root.clone()).unwrap();
        let mut after_context = NativeLinkInputs::default();
        for library in &after.dylibs {
            after_context.dylib(library);
        }
        assert_eq!(before.files, after.files);
        assert_eq!(
            std::fs::read(root.join("usr/lib/libSystem.tbd")).unwrap(),
            root_bytes
        );
        assert_ne!(before_context.context, after_context.context);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_resolution_reselects_earlier_candidates() {
        let root = sdk("shadow", false);
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join("libprobe.a"), b"!<arch>\n").unwrap();
        let link = NativeLink {
            libraries: vec![NativeLibrary {
                name: Symbol::intern("probe"),
                kind: NativeKind::Static,
            }],
            search_paths: vec![first.clone(), second.clone()],
            ..NativeLink::default()
        };
        let before = darwin_at(&link, root.clone()).unwrap();
        assert_eq!(before.archives[0].0, second.join("libprobe.a"));
        std::fs::write(first.join("libprobe.a"), b"!<arch>\n").unwrap();
        let after = darwin_at(&link, root.clone()).unwrap();
        assert_eq!(after.archives[0].0, first.join("libprobe.a"));
        assert_ne!(before.files, after.files);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retained_archive_context_survives_a_file_rollback() {
        let root = sdk("archive-rollback", false);
        let path = root.join("libprobe.a");
        let link = NativeLink {
            libraries: vec![NativeLibrary {
                name: Symbol::intern("probe"),
                kind: NativeKind::Static,
            }],
            search_paths: vec![root.clone()],
            ..NativeLink::default()
        };
        std::fs::write(&path, b"!<arch>\nA").unwrap();
        let before = darwin_at(&link, root.clone()).unwrap().inputs();
        std::fs::write(&path, b"!<arch>\nB").unwrap();
        let consumed = darwin_at(&link, root.clone()).unwrap();
        std::fs::write(&path, b"!<arch>\nA").unwrap();
        let after = darwin_at(&link, root.clone()).unwrap().inputs();
        assert_eq!(before, after);
        assert_eq!(consumed.inputs().files, after.files);
        assert_ne!(consumed.inputs().context, after.context);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retained_sdk_version_survives_a_file_rollback() {
        let root = sdk("sdk-rollback", false);
        let settings = root.join("SDKSettings.json");
        std::fs::write(&settings, r#"{"Version":"15.0"}"#).unwrap();
        let before = darwin_at(&NativeLink::default(), root.clone())
            .unwrap()
            .inputs();
        std::fs::write(&settings, r#"{"Version":"16.0"}"#).unwrap();
        let consumed = darwin_at(&NativeLink::default(), root.clone()).unwrap();
        std::fs::write(&settings, r#"{"Version":"15.0"}"#).unwrap();
        let after = darwin_at(&NativeLink::default(), root.clone())
            .unwrap()
            .inputs();
        assert_eq!(before, after);
        assert_eq!(consumed.sdk_version, 0x0010_0000);
        assert_ne!(consumed.inputs().context, after.context);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn darwin_system_aliases_keep_the_implicit_interface() {
        let root = sdk("aliases", false);
        let first = root.join("first");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::write(first.join("libSystem.dylib"), b"not an interface").unwrap();
        let link = NativeLink {
            libraries: ["c", "m", "System"]
                .map(|name| NativeLibrary {
                    name: Symbol::intern(name),
                    kind: NativeKind::Dylib,
                })
                .to_vec(),
            search_paths: vec![first],
            ..NativeLink::default()
        };
        let resolved = darwin_at(&link, root.clone()).unwrap();
        assert_eq!(resolved.files, [root.join("usr/lib/libSystem.tbd")]);
        assert_eq!(resolved.dylibs.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
