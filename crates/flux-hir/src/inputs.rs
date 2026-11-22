//! Inputs read while expanding a source crate, retained for artifact identity.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Actual file contents and environment values consumed by macro expansion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackedInputs {
    /// Physical include paths and SHA-256 digests of the exact bytes read.
    pub files: BTreeMap<PathBuf, [u8; 32]>,
    /// Variable names and their values, including absent `option_env!` inputs.
    pub environment: BTreeMap<String, Option<String>>,
    /// Successful host output, including body expansions without stored parses.
    pub macro_outputs: Vec<Vec<crate::ProcMacroToken>>,
}

#[derive(Debug, Default)]
pub(crate) struct InputTrace(pub(crate) Mutex<TrackedInputs>, AtomicU64);

impl InputTrace {
    pub(crate) fn from_inputs(inputs: TrackedInputs) -> Self {
        Self(Mutex::new(inputs), AtomicU64::default())
    }
}
impl Clone for InputTrace {
    fn clone(&self) -> Self {
        Self(
            Mutex::new(self.0.lock().expect("input trace lock poisoned").clone()),
            AtomicU64::new(self.1.load(Ordering::Relaxed)),
        )
    }
}

impl crate::CrateData {
    /// Counts external read and host expansion attempts, including repeats.
    /// Compare within one compilation before and after a lowering phase.
    pub fn observation_count(&self) -> u64 {
        self.input_trace.1.load(Ordering::Relaxed)
    }

    pub(crate) fn observe_input(&self) {
        self.input_trace.1.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether lowering executed a host macro, including empty expansions.
    pub fn expanded_host_macros(&self) -> bool {
        !self
            .input_trace
            .0
            .lock()
            .expect("input trace lock poisoned")
            .macro_outputs
            .is_empty()
    }

    /// Returns a snapshot of the inputs actually consumed by this crate.
    pub fn tracked_inputs(&self) -> TrackedInputs {
        self.with_tracked_inputs(TrackedInputs::clone)
    }

    /// Reads a consistent trace without copying macro tokens.
    pub fn with_tracked_inputs<R>(&self, read: impl FnOnce(&TrackedInputs) -> R) -> R {
        let trace = self
            .input_trace
            .0
            .lock()
            .expect("input trace lock poisoned");
        read(&trace)
    }

    pub(crate) fn record_macro_output(&self, tokens: &[crate::ProcMacroToken]) {
        self.input_trace
            .0
            .lock()
            .expect("input trace lock poisoned")
            .macro_outputs
            .push(tokens.to_vec());
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
