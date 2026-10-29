//! Crate records carry semantic dependency edges, never process handles.
use super::{CrateData, LoadedCrates, Mutex};
use crate::flib::{Decode, Decoder, Encode, Encoder, Error};
impl Encode for CrateData {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        if self.is_core && !e.trusted_builtin_suite {
            return Err(Error("embedded crates cannot be written as user flibs"));
        }
        if e.trusted_builtin_suite {
            self.is_core.encode(e)?;
        }
        self.name.encode(e)?;
        self.root.encode(e)?;
        self.edition.encode(e)?;
        self.recursion_limit.encode(e)?;
        self.externs.encode(e)?;
        self.extern_aliases.encode(e)?;
        self.dependencies.encode(e)?;
        self.no_std.encode(e)?;
        self.macro_use.encode(e)?;
        self.compilation_dir.encode(e)?;
        self.environment.encode(e)?;
        self.cfg.encode(e)?;
        self.loaded_externs
            .0
            .lock()
            .expect("loaded crates lock poisoned")
            .encode(e)?;
        self.tracked_inputs().encode(e)
    }
}
impl Decode for CrateData {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let is_core = if d.trusted_builtin_suite {
            bool::decode(d)?
        } else {
            false
        };
        let name = Decode::decode(d)?;
        let root = Decode::decode(d)?;
        let edition = Decode::decode(d)?;
        let recursion_limit = Decode::decode(d)?;
        let externs = Decode::decode(d)?;
        let extern_aliases = Decode::decode(d)?;
        let dependencies = Decode::decode(d)?;
        let no_std = Decode::decode(d)?;
        let macro_use = Decode::decode(d)?;
        let compilation_dir = Decode::decode(d)?;
        let environment = Decode::decode(d)?;
        let cfg = Decode::decode(d)?;
        let loaded = Decode::decode(d)?;
        let tracked = Decode::decode(d)?;
        Ok(Self {
            name,
            root,
            edition,
            recursion_limit,
            externs,
            extern_aliases,
            dependencies,
            no_std,
            macro_use,
            is_core,
            compilation_dir,
            environment,
            cfg,
            loaded_externs: LoadedCrates(Mutex::new(loaded)),
            input_trace: crate::inputs::InputTrace(Mutex::new(tracked)),
        })
    }
}
