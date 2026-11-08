//! Native link requests and the resolved inputs of a reusable executable.
use std::path::PathBuf;

use flux_hir::{NativeKind, NativeLibrary};
use flux_intern::Symbol;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum Kind {
    Static,
    Dylib,
    Framework,
}

/// A worker-safe link request; interned symbols are reconstructed on its thread.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Request {
    libraries: Vec<(String, Kind)>,
    search_paths: Vec<PathBuf>,
    framework_paths: Vec<PathBuf>,
}

impl From<&flux_codegen::NativeLink> for Request {
    fn from(link: &flux_codegen::NativeLink) -> Self {
        Self {
            libraries: link
                .libraries
                .iter()
                .map(|library| {
                    (
                        library.name.to_string(),
                        match library.kind {
                            NativeKind::Static => Kind::Static,
                            NativeKind::Dylib => Kind::Dylib,
                            NativeKind::Framework => Kind::Framework,
                        },
                    )
                })
                .collect(),
            search_paths: link.search_paths.clone(),
            framework_paths: link.framework_paths.clone(),
        }
    }
}

impl Request {
    pub(crate) fn link(&self) -> flux_codegen::NativeLink {
        flux_codegen::NativeLink {
            libraries: self
                .libraries
                .iter()
                .map(|(name, kind)| NativeLibrary {
                    name: Symbol::intern(name),
                    kind: match kind {
                        Kind::Static => NativeKind::Static,
                        Kind::Dylib => NativeKind::Dylib,
                        Kind::Framework => NativeKind::Framework,
                    },
                })
                .collect(),
            search_paths: self.search_paths.clone(),
            framework_paths: self.framework_paths.clone(),
        }
    }

    pub(crate) fn configuration(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Input {
    path: PathBuf,
    resolved: PathBuf,
    hash: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Snapshot {
    request: Request,
    inputs: Vec<Input>,
    context: [u8; 32],
}

impl Snapshot {
    pub(crate) fn capture(link: &flux_codegen::NativeLink) -> Result<Self, String> {
        let target = flux_codegen::Target::host().ok_or("unsupported native link target")?;
        let resolved =
            flux_codegen::native_link_inputs(target, link).map_err(|error| error.to_string())?;
        let inputs = resolved
            .files
            .into_iter()
            .map(|path| {
                let resolved = path.canonicalize().map_err(|error| error.to_string())?;
                if path.to_str().is_none() || resolved.to_str().is_none() {
                    return Err("native link input path is not utf-8".into());
                }
                let hash = crate::artifact_cache::hash_file(&path)?;
                Ok(Input {
                    path,
                    resolved,
                    hash,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            request: Request::from(link),
            inputs,
            context: blake3::hash(&resolved.context).into(),
        })
    }

    pub(crate) fn matches_request(&self, link: &flux_codegen::NativeLink) -> bool {
        self.request == Request::from(link)
    }

    pub(crate) fn matches_consumed(&self, inputs: &flux_codegen::NativeLinkInputs) -> bool {
        self.context == <[u8; 32]>::from(blake3::hash(&inputs.context))
            && self
                .inputs
                .iter()
                .map(|input| &input.path)
                .eq(inputs.files.iter())
    }

    pub(crate) fn is_fresh(&self) -> bool {
        Self::capture(&self.request.link()).is_ok_and(|current| current == *self)
    }
}
