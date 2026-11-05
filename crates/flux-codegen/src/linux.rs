//! Native glibc startup and shared-library discovery for Keel's ELF linker.
use crate::{Error, NativeLink, ObjectInput, Stage};
use std::path::Path;

pub(crate) fn link(
    objects: &[ObjectInput<'_>],
    output: &Path,
    native: &NativeLink,
) -> Result<Vec<String>, Error> {
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

    pub(super) fn link(
        objects: &[ObjectInput<'_>],
        output: &Path,
        native: &NativeLink,
    ) -> Result<Vec<String>, Error> {
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
        let files: Vec<_> = names
            .iter()
            .map(|name| {
                let path = if Path::new(name).is_absolute() {
                    PathBuf::from(name)
                } else {
                    runtime_file(name)?
                };
                std::fs::read(&path).map_err(|e| {
                    Error::io(Stage::Sdk, &format!("cannot read {}", path.display()), e)
                })
            })
            .collect::<Result<_, _>>()?;
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
        let interpreter = runtime_file("ld-linux-x86-64.so.2")?;
        let executable = keel_link::elf::link(
            &inputs,
            keel_link::elf::Options {
                interpreter: interpreter.to_string_lossy().into_owned(),
                ..Default::default()
            },
        )
        .map_err(|e| Error::message(Stage::Link, e))?;
        install(&executable.bytes, output)?;
        Ok(executable.warnings)
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
