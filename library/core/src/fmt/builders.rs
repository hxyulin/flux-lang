//! The helpers `#[derive(Debug)]` and `Debug` impls write through:
//! `debug_struct`, `debug_tuple`, `debug_list`, `debug_set` and
//! `debug_map`, as rustc's, `{:#?}`'s indentation included.

use super::*;
use crate::iter::{IntoIterator, Iterator};

/// Whether the text written so far ends a line.
struct PadAdapterState {
    on_newline: bool,
}

impl PadAdapterState {
    fn new() -> PadAdapterState {
        PadAdapterState { on_newline: true }
    }
}

/// Indents every line written through it by four spaces: what `{:#?}`
/// writes a nested value through.
struct PadAdapter<'buf, 'state> {
    buf: &'buf mut (dyn Write + 'buf),
    state: &'state mut PadAdapterState,
}

impl Write for PadAdapter<'_, '_> {
    fn write_str(&mut self, s: &str) -> Result {
        let bytes = s.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'\n' {
                end += 1;
            }
            if end < bytes.len() {
                end += 1;
            }
            if self.state.on_newline {
                self.buf.write_str("    ")?;
            }
            self.state.on_newline = bytes[end - 1] == b'\n';
            self.buf.write_str(sub(s, start, end))?;
            start = end;
        }
        Ok(())
    }

    fn write_char(&mut self, c: char) -> Result {
        if self.state.on_newline {
            self.buf.write_str("    ")?;
        }
        self.state.on_newline = c == '\n';
        self.buf.write_char(c)
    }
}

/// Writes `value` with `fmt`'s options, indented, then `suffix`.
fn pretty(fmt: &mut Formatter<'_>, state: &mut PadAdapterState, value: &dyn Debug, suffix: &str) -> Result {
    let options = fmt.options;
    let mut pad = PadAdapter {
        buf: &mut *fmt.buf,
        state,
    };
    let mut writer = Formatter {
        options,
        buf: &mut pad,
    };
    Debug::fmt(value, &mut writer)?;
    writer.write_str(suffix)
}

/// Writes `s`, indented.
fn pretty_str(fmt: &mut Formatter<'_>, s: &str) -> Result {
    let mut state = PadAdapterState::new();
    let mut pad = PadAdapter {
        buf: &mut *fmt.buf,
        state: &mut state,
    };
    pad.write_str(s)
}

/// What `Formatter::debug_struct` gives.
pub struct DebugStruct<'a, 'b: 'a> {
    fmt: &'a mut Formatter<'b>,
    result: Result,
    has_fields: bool,
}

pub(super) fn debug_struct_new<'a, 'b>(fmt: &'a mut Formatter<'b>, name: &str) -> DebugStruct<'a, 'b> {
    let result = fmt.write_str(name);
    DebugStruct {
        fmt,
        result,
        has_fields: false,
    }
}

impl<'a, 'b: 'a> DebugStruct<'a, 'b> {
    /// Adds a field, `name: value`.
    pub fn field(&mut self, name: &str, value: &dyn Debug) -> &mut DebugStruct<'a, 'b> {
        if self.result.is_ok() {
            self.result = self.write_field(name, value);
        }
        self.has_fields = true;
        self
    }

    fn write_field(&mut self, name: &str, value: &dyn Debug) -> Result {
        if self.fmt.alternate() {
            if !self.has_fields {
                self.fmt.write_str(" {\n")?;
            }
            let mut state = PadAdapterState::new();
            let options = self.fmt.options;
            let mut pad = PadAdapter {
                buf: &mut *self.fmt.buf,
                state: &mut state,
            };
            let mut writer = Formatter {
                options,
                buf: &mut pad,
            };
            writer.write_str(name)?;
            writer.write_str(": ")?;
            Debug::fmt(value, &mut writer)?;
            writer.write_str(",\n")
        } else {
            let prefix = if self.has_fields { ", " } else { " { " };
            self.fmt.write_str(prefix)?;
            self.fmt.write_str(name)?;
            self.fmt.write_str(": ")?;
            Debug::fmt(value, self.fmt)
        }
    }

    /// Ends with `..`, for fields not shown.
    pub fn finish_non_exhaustive(&mut self) -> Result {
        if self.result.is_ok() {
            self.result = if self.has_fields {
                if self.fmt.alternate() {
                    match pretty_str(self.fmt, "..\n") {
                        Ok(()) => self.fmt.write_str("}"),
                        Err(e) => Err(e),
                    }
                } else {
                    self.fmt.write_str(", .. }")
                }
            } else {
                self.fmt.write_str(" { .. }")
            };
        }
        self.result
    }

    pub fn finish(&mut self) -> Result {
        if self.has_fields && self.result.is_ok() {
            self.result = if self.fmt.alternate() {
                self.fmt.write_str("}")
            } else {
                self.fmt.write_str(" }")
            };
        }
        self.result
    }
}

/// What `Formatter::debug_tuple` gives.
pub struct DebugTuple<'a, 'b: 'a> {
    fmt: &'a mut Formatter<'b>,
    result: Result,
    fields: usize,
    empty_name: bool,
}

pub(super) fn debug_tuple_new<'a, 'b>(fmt: &'a mut Formatter<'b>, name: &str) -> DebugTuple<'a, 'b> {
    let result = fmt.write_str(name);
    DebugTuple {
        fmt,
        result,
        fields: 0,
        empty_name: name.is_empty(),
    }
}

impl<'a, 'b: 'a> DebugTuple<'a, 'b> {
    /// Adds a field.
    pub fn field(&mut self, value: &dyn Debug) -> &mut DebugTuple<'a, 'b> {
        if self.result.is_ok() {
            self.result = self.write_field(value);
        }
        self.fields += 1;
        self
    }

    fn write_field(&mut self, value: &dyn Debug) -> Result {
        if self.fmt.alternate() {
            if self.fields == 0 {
                self.fmt.write_str("(\n")?;
            }
            let mut state = PadAdapterState::new();
            pretty(self.fmt, &mut state, value, ",\n")
        } else {
            let prefix = if self.fields == 0 { "(" } else { ", " };
            self.fmt.write_str(prefix)?;
            Debug::fmt(value, self.fmt)
        }
    }

    pub fn finish_non_exhaustive(&mut self) -> Result {
        if self.result.is_ok() {
            self.result = if self.fields > 0 {
                if self.fmt.alternate() {
                    match pretty_str(self.fmt, "..\n") {
                        Ok(()) => self.fmt.write_str(")"),
                        Err(e) => Err(e),
                    }
                } else {
                    self.fmt.write_str(", ..)")
                }
            } else {
                self.fmt.write_str("(..)")
            };
        }
        self.result
    }

    pub fn finish(&mut self) -> Result {
        if self.fields > 0 && self.result.is_ok() {
            self.result = self.write_end();
        }
        self.result
    }

    fn write_end(&mut self) -> Result {
        if self.fields == 1 && self.empty_name && !self.fmt.alternate() {
            self.fmt.write_str(",")?;
        }
        self.fmt.write_str(")")
    }
}

/// The entries of a list or set.
struct DebugInner<'a, 'b: 'a> {
    fmt: &'a mut Formatter<'b>,
    result: Result,
    has_fields: bool,
}

impl<'a, 'b: 'a> DebugInner<'a, 'b> {
    fn entry(&mut self, entry: &dyn Debug) {
        if self.result.is_ok() {
            self.result = self.write_entry(entry);
        }
        self.has_fields = true;
    }

    fn write_entry(&mut self, entry: &dyn Debug) -> Result {
        if self.fmt.alternate() {
            if !self.has_fields {
                self.fmt.write_str("\n")?;
            }
            let mut state = PadAdapterState::new();
            pretty(self.fmt, &mut state, entry, ",\n")
        } else {
            if self.has_fields {
                self.fmt.write_str(", ")?;
            }
            Debug::fmt(entry, self.fmt)
        }
    }

    fn finish_non_exhaustive(&mut self, close: &str, after_fields: &str, empty: &str) -> Result {
        if self.result.is_ok() {
            self.result = if self.has_fields {
                if self.fmt.alternate() {
                    match pretty_str(self.fmt, "..\n") {
                        Ok(()) => self.fmt.write_str(close),
                        Err(e) => Err(e),
                    }
                } else {
                    self.fmt.write_str(after_fields)
                }
            } else {
                self.fmt.write_str(empty)
            };
        }
        self.result
    }

    fn finish(&mut self, close: &str) -> Result {
        if self.result.is_ok() {
            self.result = self.fmt.write_str(close);
        }
        self.result
    }
}

/// What `Formatter::debug_set` gives.
pub struct DebugSet<'a, 'b: 'a> {
    inner: DebugInner<'a, 'b>,
}

pub(super) fn debug_set_new<'a, 'b>(fmt: &'a mut Formatter<'b>) -> DebugSet<'a, 'b> {
    let result = fmt.write_str("{");
    DebugSet {
        inner: DebugInner {
            fmt,
            result,
            has_fields: false,
        },
    }
}

impl<'a, 'b: 'a> DebugSet<'a, 'b> {
    pub fn entry(&mut self, entry: &dyn Debug) -> &mut DebugSet<'a, 'b> {
        self.inner.entry(entry);
        self
    }

    pub fn entries<D: Debug, I: IntoIterator<Item = D>>(&mut self, entries: I) -> &mut DebugSet<'a, 'b> {
        for entry in entries {
            self.inner.entry(&entry);
        }
        self
    }

    pub fn finish_non_exhaustive(&mut self) -> Result {
        self.inner.finish_non_exhaustive("}", ", ..}", "..}")
    }

    pub fn finish(&mut self) -> Result {
        self.inner.finish("}")
    }
}

/// What `Formatter::debug_list` gives.
pub struct DebugList<'a, 'b: 'a> {
    inner: DebugInner<'a, 'b>,
}

pub(super) fn debug_list_new<'a, 'b>(fmt: &'a mut Formatter<'b>) -> DebugList<'a, 'b> {
    let result = fmt.write_str("[");
    DebugList {
        inner: DebugInner {
            fmt,
            result,
            has_fields: false,
        },
    }
}

impl<'a, 'b: 'a> DebugList<'a, 'b> {
    pub fn entry(&mut self, entry: &dyn Debug) -> &mut DebugList<'a, 'b> {
        self.inner.entry(entry);
        self
    }

    pub fn entries<D: Debug, I: IntoIterator<Item = D>>(&mut self, entries: I) -> &mut DebugList<'a, 'b> {
        for entry in entries {
            self.inner.entry(&entry);
        }
        self
    }

    pub fn finish_non_exhaustive(&mut self) -> Result {
        self.inner.finish_non_exhaustive("]", ", ..]", "..]")
    }

    pub fn finish(&mut self) -> Result {
        self.inner.finish("]")
    }
}

/// What `Formatter::debug_map` gives.
pub struct DebugMap<'a, 'b: 'a> {
    fmt: &'a mut Formatter<'b>,
    result: Result,
    has_fields: bool,
    has_key: bool,
    state: PadAdapterState,
}

pub(super) fn debug_map_new<'a, 'b>(fmt: &'a mut Formatter<'b>) -> DebugMap<'a, 'b> {
    let result = fmt.write_str("{");
    DebugMap {
        fmt,
        result,
        has_fields: false,
        has_key: false,
        state: PadAdapterState::new(),
    }
}

impl<'a, 'b: 'a> DebugMap<'a, 'b> {
    /// Adds an entry, `key: value`.
    pub fn entry(&mut self, key: &dyn Debug, value: &dyn Debug) -> &mut DebugMap<'a, 'b> {
        self.key(key);
        self.value(value)
    }

    pub fn key(&mut self, key: &dyn Debug) -> &mut DebugMap<'a, 'b> {
        if self.result.is_ok() {
            if self.has_key {
                crate::panicking::panic(
                    "attempted to begin a new map entry without completing the previous one",
                );
            }
            self.result = self.write_key(key);
            self.has_key = true;
        }
        self
    }

    fn write_key(&mut self, key: &dyn Debug) -> Result {
        if self.fmt.alternate() {
            if !self.has_fields {
                self.fmt.write_str("\n")?;
            }
            self.state = PadAdapterState::new();
            pretty(self.fmt, &mut self.state, key, ": ")
        } else {
            if self.has_fields {
                self.fmt.write_str(", ")?;
            }
            Debug::fmt(key, self.fmt)?;
            self.fmt.write_str(": ")
        }
    }

    pub fn value(&mut self, value: &dyn Debug) -> &mut DebugMap<'a, 'b> {
        if self.result.is_ok() {
            if !self.has_key {
                crate::panicking::panic("attempted to format a map value before its key");
            }
            self.result = if self.fmt.alternate() {
                pretty(self.fmt, &mut self.state, value, ",\n")
            } else {
                Debug::fmt(value, self.fmt)
            };
            self.has_key = false;
        }
        self.has_fields = true;
        self
    }

    pub fn entries<K: Debug, V: Debug, I: IntoIterator<Item = (K, V)>>(
        &mut self,
        entries: I,
    ) -> &mut DebugMap<'a, 'b> {
        for (k, v) in entries {
            self.entry(&k, &v);
        }
        self
    }

    pub fn finish_non_exhaustive(&mut self) -> Result {
        if self.result.is_ok() {
            if self.has_key {
                crate::panicking::panic("attempted to finish a map with a partial entry");
            }
            self.result = if self.has_fields {
                if self.fmt.alternate() {
                    match pretty_str(self.fmt, "..\n") {
                        Ok(()) => self.fmt.write_str("}"),
                        Err(e) => Err(e),
                    }
                } else {
                    self.fmt.write_str(", ..}")
                }
            } else {
                self.fmt.write_str("..}")
            };
        }
        self.result
    }

    pub fn finish(&mut self) -> Result {
        if self.result.is_ok() {
            if self.has_key {
                crate::panicking::panic("attempted to finish a map with a partial entry");
            }
            self.result = self.fmt.write_str("}");
        }
        self.result
    }
}
