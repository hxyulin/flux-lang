//! Inputs read while expanding a source crate, retained for artifact identity.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Actual file contents and environment values consumed by macro expansion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackedInputs {
    /// Physical include paths and SHA-256 digests of the exact bytes read.
    pub files: BTreeMap<PathBuf, [u8; 32]>,
    /// Variable names and their values, including absent `option_env!` inputs.
    pub environment: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Default)]
pub(crate) struct InputTrace(pub(crate) Mutex<TrackedInputs>);
impl Clone for InputTrace {
    fn clone(&self) -> Self {
        Self(Mutex::new(
            self.0.lock().expect("input trace lock poisoned").clone(),
        ))
    }
}

impl crate::CrateData {
    /// Returns a snapshot of the inputs actually consumed by this crate.
    pub fn tracked_inputs(&self) -> TrackedInputs {
        self.input_trace
            .0
            .lock()
            .expect("input trace lock poisoned")
            .clone()
    }

    pub(crate) fn record_file_input(&self, path: PathBuf, bytes: &[u8]) -> Result<(), String> {
        let digest = crate::flib::digest(bytes);
        let mut trace = self
            .input_trace
            .0
            .lock()
            .expect("input trace lock poisoned");
        if trace
            .files
            .get(&path)
            .is_some_and(|previous| *previous != digest)
        {
            return Err(format!(
                "input changed during compilation: {}",
                path.display()
            ));
        }
        trace.files.insert(path, digest);
        Ok(())
    }

    pub(crate) fn record_environment_input(
        &self,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        let value = value.map(str::to_owned);
        let mut trace = self
            .input_trace
            .0
            .lock()
            .expect("input trace lock poisoned");
        if trace
            .environment
            .get(name)
            .is_some_and(|previous| *previous != value)
        {
            return Err(format!(
                "environment variable changed during compilation: {name}"
            ));
        }
        trace.environment.insert(name.to_owned(), value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_clone_is_a_snapshot_and_conflicting_reads_fail() {
        let mut db = crate::SourceDb::new();
        let root = db.add("trace.rs", "pub fn value() {}");
        let hir = crate::lower(&mut db, root, flux_intern::Symbol::intern("trace"));
        let id = hir.items.crates.len() - 1;
        let crate_data = &hir.items.crates[id];
        crate_data
            .record_file_input(PathBuf::from("input"), b"first")
            .unwrap();
        crate_data
            .record_environment_input("NAME", Some("first"))
            .unwrap();
        let snapshot = crate_data.clone();
        crate_data
            .record_file_input(PathBuf::from("other"), b"second")
            .unwrap();
        assert!(
            !snapshot
                .tracked_inputs()
                .files
                .contains_key(&PathBuf::from("other"))
        );
        assert!(
            crate_data
                .record_file_input(PathBuf::from("input"), b"changed")
                .is_err()
        );
        assert!(crate_data.record_environment_input("NAME", None).is_err());
    }
}
