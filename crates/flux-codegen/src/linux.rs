//! Native glibc startup and shared-library discovery for Keel's ELF linker.
use crate::{Error, NativeLink, ObjectInput, Stage};
use std::path::Path;

pub(crate) fn link(
    objects: &[ObjectInput<'_>],
    output: &Path,
    native: &NativeLink,
) -> Result<(Vec<String>, crate::NativeLinkInputs), Error> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        native::link(objects, output, native)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = (objects, output, native);
        Err(Error::message(
            Stage::Link,
            "x86-64 Linux linking requires a native Linux host",
        ))
    }
}

pub(crate) fn inputs(native: &NativeLink) -> Result<crate::NativeLinkInputs, Error> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        native::inputs(native)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = native;
        Err(Error::message(
            Stage::Link,
            "x86-64 Linux linking requires a native Linux host",
        ))
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod native {
    use super::*;
    use crate::native::{Found, find};
    use flux_hir::NativeKind;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    // Ask the installed C driver for its runtime files, rather than guessing a
    // distribution's GCC version or multiarch directories. Never invoke its linker.
    fn runtime_file(name: &str) -> Result<PathBuf, Error> {
        let result = std::process::Command::new("cc")
            .arg(format!("-print-file-name={name}"))
            .output()
            .map_err(|e| Error::io(Stage::Sdk, "cannot locate Linux C runtime (cc required)", e))?;
        let path = PathBuf::from(String::from_utf8_lossy(&result.stdout).trim());
        if !result.status.success() || !path.is_file() {
            return Err(Error::message(
                Stage::Sdk,
                format!("cannot locate Linux runtime file `{name}`"),
            ));
        }
        Ok(path)
    }

    struct Resolved {
        names: Vec<String>,
        paths: Vec<PathBuf>,
        startup: usize,
        interpreter: PathBuf,
    }

    fn resolve(native: &NativeLink) -> Result<Resolved, Error> {
        let mut names = vec![
            "crt1.o".to_owned(),
            "crti.o".to_owned(),
            "crtbegin.o".to_owned(),
        ];
        let startup = names.len();
        names.extend([
            "libc.so.6".into(),
            "libm.so.6".into(),
            "libpthread.so.0".into(),
        ]);
        for library in &native.libraries {
            if library.kind == NativeKind::Framework {
                return Err(Error::message(
                    Stage::Link,
                    "frameworks can only be linked on Apple targets",
                ));
            }
            // Requested directories first, then the C driver's own.
            let found = if native.search_paths.is_empty() && library.kind == NativeKind::Dylib {
                None
            } else {
                match find(*library, native, &[], &[], &["so", "a"]) {
                    Ok(Found::Archive(path, _) | Found::Shared(path) | Found::Interface(path)) => {
                        Some(path)
                    }
                    Err(error) if library.kind == NativeKind::Static => return Err(error),
                    Err(_) => None,
                }
            };
            if let Some(path) = found {
                let path = path
                    .canonicalize()
                    .map_err(|e| Error::io(Stage::Link, "cannot resolve library path", e))?
                    .to_string_lossy()
                    .into_owned();
                if !names.contains(&path) {
                    names.push(path);
                }
                continue;
            }
            let name = library.name.as_str();
            let file = match name {
                "c" | "m" => continue,
                "pthread" => "libpthread.so.0".into(),
                "dl" => "libdl.so.2".into(),
                "rt" => "librt.so.1".into(),
                _ => format!("lib{name}.so"),
            };
            if !names.contains(&file) {
                names.push(file);
            }
        }
        names.extend(["libgcc.a".into(), "crtend.o".into(), "crtn.o".into()]);
        let paths = names
            .iter()
            .map(|name| {
                if Path::new(name).is_absolute() {
                    Ok(PathBuf::from(name))
                } else {
                    runtime_file(name)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let interpreter = runtime_file("ld-linux-x86-64.so.2")?;
        Ok(Resolved {
            names,
            paths,
            startup,
            interpreter,
        })
    }

    fn read_inputs(resolved: &Resolved) -> Result<Vec<Vec<u8>>, Error> {
        resolved
            .paths
            .iter()
            .map(|path| {
                std::fs::read(path).map_err(|error| {
                    Error::io(
                        Stage::Sdk,
                        &format!("cannot read {}", path.display()),
                        error,
                    )
                })
            })
            .collect()
    }

    fn consumed_inputs(resolved: &Resolved, files: &[Vec<u8>]) -> crate::NativeLinkInputs {
        let mut result = crate::NativeLinkInputs::default();
        result.string("linux-native-link-v2");
        result
            .context
            .extend_from_slice(&(resolved.names.len() as u64).to_le_bytes());
        for (name, bytes) in resolved.names.iter().zip(files) {
            result.string(name);
            result.contents(bytes);
        }
        // The interpreter's path is embedded; its bytes are not read by the
        // linker. CLI freshness can still track the installed interpreter file.
        result.bytes(resolved.interpreter.as_os_str().as_encoded_bytes());
        result.files = resolved.paths.clone();
        result.files.push(resolved.interpreter.clone());
        result
    }

    pub(super) fn inputs(native: &NativeLink) -> Result<crate::NativeLinkInputs, Error> {
        let resolved = resolve(native)?;
        let files = read_inputs(&resolved)?;
        Ok(consumed_inputs(&resolved, &files))
    }

    pub(super) fn link(
        objects: &[ObjectInput<'_>],
        output: &Path,
        native: &NativeLink,
    ) -> Result<(Vec<String>, crate::NativeLinkInputs), Error> {
        let resolved = resolve(native)?;
        let files = read_inputs(&resolved)?;
        let consumed = consumed_inputs(&resolved, &files);
        let Resolved {
            names,
            startup,
            interpreter,
            ..
        } = resolved;
        let mut inputs: Vec<_> = names
            .iter()
            .zip(&files)
            .map(|(name, bytes)| keel_link::Input { name, bytes })
            .collect();
        inputs.splice(
            startup..startup,
            objects.iter().map(|o| keel_link::Input {
                name: o.name,
                bytes: o.bytes,
            }),
        );
        let executable = keel_link::elf::link(
            &inputs,
            keel_link::elf::Options {
                interpreter: interpreter.to_string_lossy().into_owned(),
                ..Default::default()
            },
        )
        .map_err(|e| Error::message(Stage::Link, e))?;
        install(&executable.bytes, output)?;
        Ok((executable.warnings, consumed))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn consumed_runtime_bytes_survive_a_file_rollback() {
            let root =
                std::env::temp_dir().join(format!("flux-runtime-rollback-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("runtime.o");
            let resolved = Resolved {
                names: vec!["runtime.o".into()],
                paths: vec![path.clone()],
                startup: 1,
                interpreter: root.join("interpreter"),
            };
            std::fs::write(&path, b"A").unwrap();
            let before = consumed_inputs(&resolved, &read_inputs(&resolved).unwrap());
            std::fs::write(&path, b"B").unwrap();
            let bytes = read_inputs(&resolved).unwrap();
            std::fs::write(&path, b"A").unwrap();
            let after = consumed_inputs(&resolved, &read_inputs(&resolved).unwrap());
            assert_eq!(before, after);
            assert_ne!(consumed_inputs(&resolved, &bytes).context, after.context);
            std::fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn freshness_resolution_includes_runtime_and_interpreter() {
            let resolved = resolve(&NativeLink::default()).unwrap();
            assert_eq!(resolved.startup, 3);
            assert_eq!(&resolved.names[..3], ["crt1.o", "crti.o", "crtbegin.o"]);
            assert!(resolved.paths.iter().all(|path| path.is_file()));
            let tracked = inputs(&NativeLink::default()).unwrap();
            assert_eq!(&tracked.files[..resolved.paths.len()], resolved.paths);
            assert_eq!(tracked.files.last(), Some(&resolved.interpreter));
            assert!(!tracked.context.is_empty());
        }
    }

    // Replace only a complete executable, preserving the old destination on
    // failure and allowing a running compiler to rebuild its own path.
    fn install(bytes: &[u8], output: &Path) -> Result<(), Error> {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        for n in 0..100 {
            let temp = parent.join(format!(".flux-link-{}-{n}", std::process::id()));
            let mut file = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
            {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(Error::io(Stage::Write, "cannot create executable", e)),
            };
            let result = (|| {
                file.write_all(bytes)?;
                file.set_permissions(std::fs::Permissions::from_mode(0o755))?;
                drop(file);
                std::fs::rename(&temp, output)
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&temp);
            }
            return result.map_err(|e| Error::io(Stage::Write, "cannot install executable", e));
        }
        Err(Error::message(
            Stage::Write,
            "cannot allocate executable temporary file",
        ))
    }
}
