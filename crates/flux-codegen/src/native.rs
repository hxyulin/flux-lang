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
