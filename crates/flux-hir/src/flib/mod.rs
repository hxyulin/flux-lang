//! Binary metadata primitives and crate-local identity maps for Fluxlib.

mod allocations;
mod archive;
mod builtin_suite;
pub use builtin_suite::{read_builtin_suite, write_builtin_suite};
mod digest;
mod identity;
mod items;
mod macros;
pub use archive::{read_items, write_items};
pub use digest::digest;
pub use identity::CrateIndices;

use std::collections::BTreeMap;
use std::fmt;

use flux_arena::Idx;
use flux_intern::Symbol;

/// Maximum byte length or element count of one metadata allocation.
pub const MAX_RECORD_LENGTH: usize = 1_000_000;

/// Maximum number of simultaneously nested metadata records.
pub const MAX_NESTING_DEPTH: u32 = 128;

/// A malformed or unsupported Fluxlib metadata record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub &'static str);

impl Error {
    pub const fn new(message: &'static str) -> Self {
        Self(message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}

/// An independently indexed semantic arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdKind {
    Crate,
    Module,
    Function,
    Struct,
    Trait,
    Impl,
    Const,
    TypeAlias,
    Macro,
    MacroScope,
    File,
}

/// Explicit mappings between process and artifact-local indices.
#[derive(Debug, Clone, Default)]
pub struct IdMap {
    entries: BTreeMap<(IdKind, u32), u32>,
    strict: bool,
    owned: std::collections::BTreeSet<(IdKind, u32)>,
    source_ranges: Vec<(u32, u32, u32)>,
}
impl IdMap {
    /// Creates a map that rejects references without an explicit mapping.
    pub fn strict() -> Self {
        Self {
            entries: BTreeMap::new(),
            strict: true,
            owned: std::collections::BTreeSet::new(),
            source_ranges: Vec::new(),
        }
    }
    /// Records one reference mapping.
    pub fn insert(&mut self, kind: IdKind, from: u32, to: u32) {
        self.entries.insert((kind, from), to);
    }
    /// Registers the process coordinates of an included diagnostic source.
    pub fn add_source_range(&mut self, file: u32, start: u32, end: u32) {
        self.source_ranges.push((file, start, end));
    }
    fn source_range(&self, range: flux_syntax::TextRange) -> Option<(u32, u32)> {
        self.source_ranges.iter().find_map(|&(file, start, end)| {
            (range.start() >= start && range.end() <= end).then_some((file, start))
        })
    }
    fn source_bounds(&self, file: u32) -> Option<(u32, u32)> {
        self.source_ranges
            .iter()
            .find_map(|&(found, start, end)| (found == file).then_some((start, end)))
    }
    pub fn is_strict(&self) -> bool {
        self.strict
    }
    pub fn mark_owned(&mut self, kind: IdKind, index: u32) {
        self.owned.insert((kind, index));
    }
    pub fn contains_owned(&self, kind: IdKind, index: u32) -> bool {
        self.owned.contains(&(kind, index))
    }
    pub fn extend(&mut self, other: &Self) {
        self.entries
            .extend(other.entries.iter().map(|(&k, &v)| (k, v)));
        self.owned.extend(other.owned.iter().copied());
        self.source_ranges
            .extend(other.source_ranges.iter().copied());
    }
    /// Resolves a reference, rejecting missing mappings in a strict map.
    pub fn get(&self, kind: IdKind, from: u32) -> Result<u32, Error> {
        match self.entries.get(&(kind, from)) {
            Some(&to) => Ok(to),
            None if self.strict => Err(Error("unmapped item reference")),
            None => Ok(from),
        }
    }
    /// Returns the inverse mapping, rejecting ambiguous destination indices.
    pub fn inverse(&self) -> Result<Self, Error> {
        let mut result = Self::strict();
        for (&(kind, from), &to) in &self.entries {
            if result.entries.insert((kind, to), from).is_some() {
                return Err(Error("duplicate item reference"));
            }
        }
        result.owned.extend(self.owned.iter().copied());
        result.source_ranges = self.source_ranges.clone();
        Ok(result)
    }
}

/// A binary metadata writer with explicit item identity mappings.
#[derive(Debug, Default)]
pub struct Encoder {
    pub(crate) trusted_builtin_suite: bool,
    bytes: Vec<u8>,
    depth: u32,
    pub ids: IdMap,
    contexts: BTreeMap<u32, u32>,
    groups: BTreeMap<u32, u32>,
    types: BTreeMap<u32, (u32, Option<u32>)>,
    type_depths: BTreeMap<u32, u32>,
    allocations: BTreeMap<allocations::AllocationKey, allocations::AllocationMemo>,
    next_allocation: u32,
}
impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_ids(ids: IdMap) -> Self {
        Self {
            trusted_builtin_suite: false,
            bytes: Vec::new(),
            depth: 0,
            ids,
            contexts: BTreeMap::new(),
            groups: BTreeMap::new(),
            types: BTreeMap::new(),
            type_depths: BTreeMap::new(),
            allocations: BTreeMap::new(),
            next_allocation: 0,
        }
    }
    /// Returns a memoized semantic depth without requiring a wire definition.
    pub fn type_depth(&self, raw: u32) -> Option<u32> {
        self.type_depths.get(&raw).copied()
    }
    /// Memoizes type-DAG depth independently of preorder wire-slot reservation.
    pub fn set_type_depth(&mut self, raw: u32, depth: u32) -> Result<(), Error> {
        if depth == 0 || depth > MAX_NESTING_DEPTH {
            return Err(Error("type semantic nesting limit exceeded"));
        }
        if let Some(&old) = self.type_depths.get(&raw) {
            if old != depth {
                return Err(Error("inconsistent type semantic depth"));
            }
            return Ok(());
        }
        if self.type_depths.len() >= MAX_RECORD_LENGTH {
            return Err(Error("type table limit exceeded"));
        }
        self.type_depths.insert(raw, depth);
        Ok(())
    }
    /// Finds a completed artifact slot for one opaque process-local type key.
    pub fn type_reference(&self, raw: u32) -> Result<Option<(u32, u32)>, Error> {
        match self.types.get(&raw) {
            Some(&(wire, Some(depth))) => Ok(Some((wire, depth))),
            Some(_) => Err(Error("recursive type metadata")),
            None => Ok(None),
        }
    }
    /// Reserves a new preorder type-table slot before encoding its children.
    pub fn reserve_type(&mut self, raw: u32) -> Result<u32, Error> {
        if self.types.contains_key(&raw) {
            return Err(Error("duplicate type definition"));
        }
        if self.types.len() >= MAX_RECORD_LENGTH {
            return Err(Error("type table limit exceeded"));
        }
        let wire = self.types.len() as u32;
        self.types.insert(raw, (wire, None));
        Ok(wire)
    }
    /// Completes a type definition with its validated semantic graph depth.
    pub fn complete_type(&mut self, raw: u32, semantic_depth: u32) -> Result<(), Error> {
        self.set_type_depth(raw, semantic_depth)?;
        let entry = self
            .types
            .get_mut(&raw)
            .ok_or(Error("unreserved type definition"))?;
        if entry.1.is_some() {
            return Err(Error("duplicate type completion"));
        }
        entry.1 = Some(semantic_depth);
        Ok(())
    }
    /// Encodes one nested record using the decoder's matching depth budget.
    pub fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        if self.depth >= MAX_NESTING_DEPTH {
            return Err(Error("metadata nesting limit exceeded"));
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn write(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
}

/// A bounded metadata reader. Lengths and nesting are checked before allocation.
#[derive(Debug)]
pub struct Decoder<'a> {
    pub(crate) trusted_builtin_suite: bool,
    proc_macro_resolver: Option<std::sync::Arc<dyn crate::ProcMacroResolver>>,
    proc_macro_error: Option<String>,
    bytes: &'a [u8],
    position: usize,
    depth: u32,
    pub ids: IdMap,
    contexts: BTreeMap<u32, u32>,
    groups: BTreeMap<u32, u32>,
    types: Vec<Option<(u32, u32)>>,
    type_depths: BTreeMap<u32, u32>,
    allocations: Vec<allocations::DecodedAllocation>,
    allocation_memo: BTreeMap<allocations::AllocationKey, allocations::AllocationMemo>,
}
impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self::with_ids(bytes, IdMap::default())
    }
    pub fn with_ids(bytes: &'a [u8], ids: IdMap) -> Self {
        Self {
            trusted_builtin_suite: false,
            bytes,
            position: 0,
            depth: 0,
            proc_macro_resolver: None,
            proc_macro_error: None,
            ids,
            contexts: BTreeMap::new(),
            groups: BTreeMap::new(),
            types: Vec::new(),
            type_depths: BTreeMap::new(),
            allocations: Vec::new(),
            allocation_memo: BTreeMap::new(),
        }
    }
    /// Installs the driver-owned host resolver shared by one compilation graph.
    pub fn set_proc_macro_resolver(
        &mut self,
        resolver: std::sync::Arc<dyn crate::ProcMacroResolver>,
    ) {
        self.proc_macro_resolver = Some(resolver);
    }
    /// The host's detailed failure, when procedural-macro restoration failed.
    pub fn proc_macro_error(&self) -> Option<&str> {
        self.proc_macro_error.as_deref()
    }
    pub(crate) fn resolve_proc_macro(
        &mut self,
        descriptor: &[u8],
    ) -> Result<std::sync::Arc<dyn crate::ProcMacroExpander>, Error> {
        let resolver = self
            .proc_macro_resolver
            .as_ref()
            .ok_or(Error("procedural macro metadata requires a host resolver"))?;
        match resolver.resolve(descriptor) {
            Ok(expander) => Ok(expander),
            Err(message) => {
                self.proc_macro_error = Some(message);
                Err(Error(
                    "cannot restore procedural macro host; rebuild artifact",
                ))
            }
        }
    }
    /// Returns the semantic depth of a validated interned type in this stream.
    pub fn type_depth(&self, raw: u32) -> Option<u32> {
        self.type_depths.get(&raw).copied()
    }
    /// Reserves the next sequential type slot, rejecting duplicate/forward definitions.
    pub fn reserve_type(&mut self, wire: u32) -> Result<(), Error> {
        if self.types.len() >= MAX_RECORD_LENGTH {
            return Err(Error("type table limit exceeded"));
        }
        if wire as usize != self.types.len() {
            return Err(Error("invalid type definition slot"));
        }
        self.types.push(None);
        Ok(())
    }
    /// Publishes the interned process key only after all children validate.
    pub fn complete_type(&mut self, wire: u32, raw: u32, semantic_depth: u32) -> Result<(), Error> {
        if semantic_depth == 0 || semantic_depth > MAX_NESTING_DEPTH {
            return Err(Error("type semantic nesting limit exceeded"));
        }
        let entry = self
            .types
            .get_mut(wire as usize)
            .ok_or(Error("unreserved type definition"))?;
        if entry.is_some() {
            return Err(Error("duplicate type completion"));
        }
        if self.type_depths.contains_key(&raw) {
            return Err(Error("duplicate semantic type definition"));
        }
        self.type_depths.insert(raw, semantic_depth);
        *entry = Some((raw, semantic_depth));
        Ok(())
    }
    /// Resolves a completed type slot; unresolved parents and forward references fail.
    pub fn type_reference(&self, wire: u32) -> Result<(u32, u32), Error> {
        self.types
            .get(wire as usize)
            .copied()
            .flatten()
            .ok_or(Error("unresolved type reference"))
    }
    pub fn read(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(Error("metadata length overflow"))?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or(Error("truncated metadata"))?;
        self.position = end;
        Ok(result)
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub fn finish(&self) -> Result<(), Error> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(Error("trailing metadata bytes"))
        }
    }
    pub fn enter(&mut self) -> Result<(), Error> {
        if self.depth >= MAX_NESTING_DEPTH {
            return Err(Error("metadata nesting limit exceeded"));
        }
        self.depth += 1;
        Ok(())
    }
    pub fn leave(&mut self) {
        self.depth -= 1;
    }
    pub fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        self.enter()?;
        let result = f(self);
        self.leave();
        result
    }
    pub fn length(&mut self) -> Result<usize, Error> {
        let n = u32::decode(self)? as usize;
        if n > self.remaining() || n > MAX_RECORD_LENGTH {
            return Err(Error("metadata allocation limit exceeded"));
        }
        Ok(n)
    }
}

/// An explicitly tagged, deterministic binary record.
pub trait Encode {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error>;
}
/// A checked binary record reader.
pub trait Decode: Sized {
    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, Error>;
}

macro_rules! integer { ($($ty:ty),*) => { $(
    impl Encode for $ty { fn encode(&self,e:&mut Encoder)->Result<(),Error>{e.write(&self.to_le_bytes());Ok(())} }
    impl Decode for $ty { fn decode(d:&mut Decoder<'_>)->Result<Self,Error>{let mut bytes=[0;std::mem::size_of::<Self>()];bytes.copy_from_slice(d.read(std::mem::size_of::<Self>())?);Ok(Self::from_le_bytes(bytes))} }
)* }; }
integer!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);
impl Encode for usize {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        {
            if *self > MAX_RECORD_LENGTH {
                return Err(Error("metadata allocation limit exceeded"));
            }
            u32::try_from(*self)
                .map_err(|_| Error("metadata index overflow"))?
                .encode(e)
        }
    }
}
impl Decode for usize {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let value = u32::decode(d)? as usize;
        if value > MAX_RECORD_LENGTH {
            return Err(Error("metadata index limit exceeded"));
        }
        Ok(value)
    }
}
impl Encode for bool {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        u8::from(*self).encode(e)
    }
}
impl Decode for bool {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error("invalid boolean tag")),
        }
    }
}
impl Encode for char {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        (*self as u32).encode(e)
    }
}
impl Decode for char {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        char::from_u32(u32::decode(d)?).ok_or(Error("invalid character"))
    }
}
impl Encode for str {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.len().encode(e)?;
        e.write(self.as_bytes());
        Ok(())
    }
}
impl Encode for String {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.as_str().encode(e)
    }
}
impl Decode for String {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let n = d.length()?;
        let text = std::str::from_utf8(d.read(n)?).map_err(|_| Error("invalid metadata utf-8"))?;
        Ok(text.to_owned())
    }
}
impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        (**self).encode(e)
    }
}
impl<T: Encode> Encode for Box<T> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| (**self).encode(e))
    }
}
impl<T: Encode> Encode for Box<[T]> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.as_ref().encode(e)
    }
}
impl Encode for Box<str> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.as_ref().encode(e)
    }
}
impl<T: Decode> Decode for Box<T> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.enter()?;
        let result = T::decode(d).map(Box::new);
        d.leave();
        result
    }
}
impl<T: Encode> Encode for [T] {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| {
            self.len().encode(e)?;
            for value in self {
                value.encode(e)?;
            }
            Ok(())
        })
    }
}
impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.as_slice().encode(e)
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let n = d.length()?;
        d.enter()?;
        let result = (0..n).map(|_| T::decode(d)).collect();
        d.leave();
        result
    }
}
impl<T: Decode> Decode for Box<[T]> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Vec::<T>::decode(d)?.into_boxed_slice())
    }
}
impl Decode for Box<str> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(String::decode(d)?.into_boxed_str())
    }
}
impl<T: Encode> Encode for Option<T> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        match self {
            None => 0u8.encode(e),
            Some(v) => {
                1u8.encode(e)?;
                v.encode(e)
            }
        }
    }
}
impl<T: Decode> Decode for Option<T> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(d)?)),
            _ => Err(Error("invalid option tag")),
        }
    }
}
macro_rules! tuple { ($($t:ident:$n:tt),*) => {
impl<$($t:Encode),*> Encode for ($($t,)*) {fn encode(&self,e:&mut Encoder)->Result<(),Error>{$((self.$n).encode(e)?;)*Ok(())}}
impl<$($t:Decode),*> Decode for ($($t,)*) {fn decode(d:&mut Decoder<'_>)->Result<Self,Error>{Ok(($($t::decode(d)?,)*))}}
}; }
tuple!(A:0,B:1);
tuple!(A:0,B:1,C:2);
tuple!(A:0,B:1,C:2,D:3);

impl Encode for Symbol {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.as_str().encode(e)
    }
}
impl Decode for Symbol {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Symbol::intern(&String::decode(d)?))
    }
}
impl Encode for flux_syntax::TextRange {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        if let Some((file, base)) = e.ids.source_range(*self) {
            1u8.encode(e)?;
            crate::FileId(file).encode(e)?;
            (self.start() - base).encode(e)?;
            (self.end() - base).encode(e)
        } else {
            0u8.encode(e)?;
            self.start().encode(e)?;
            self.end().encode(e)
        }
    }
}
impl Decode for flux_syntax::TextRange {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let (base, limit) = match u8::decode(d)? {
            0 => (0, u32::MAX),
            1 => {
                let file = crate::FileId::decode(d)?;
                d.ids
                    .source_bounds(file.0)
                    .ok_or(Error("missing included source coordinates"))?
            }
            _ => return Err(Error("invalid source range tag")),
        };
        let start = u32::decode(d)?;
        let end = u32::decode(d)?;
        if start > end {
            return Err(Error("invalid source range"));
        }
        let start = base
            .checked_add(start)
            .ok_or(Error("source range overflow"))?;
        let end = base
            .checked_add(end)
            .ok_or(Error("source range overflow"))?;
        if end > limit {
            return Err(Error("source range exceeds included source"));
        }
        Ok(Self::new(start, end))
    }
}
impl Encode for flux_syntax::NodeId {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.index().encode(e)
    }
}
impl Decode for flux_syntax::NodeId {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self::from_raw(usize::decode(d)? as u32))
    }
}
impl Encode for std::path::PathBuf {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.to_str()
            .ok_or(Error("non-utf-8 metadata path"))?
            .encode(e)
    }
}
impl Decode for std::path::PathBuf {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(String::decode(d)?.into())
    }
}

/// The arena kind associated with a typed semantic index.
pub trait FlibItem {
    const KIND: IdKind;
    const REMAP: bool = true;
}
impl FlibItem for crate::Expr {
    const KIND: IdKind = IdKind::Function;
    const REMAP: bool = false;
}
macro_rules! item { ($($t:ty=>$kind:ident),*) => {$(impl FlibItem for $t {const KIND:IdKind=IdKind::$kind;})*}; }
item!(crate::Module=>Module,crate::Function=>Function,crate::Struct=>Struct,crate::Trait=>Trait,crate::Impl=>Impl,crate::Const=>Const,crate::TypeAlias=>TypeAlias,crate::MacroDef=>Macro,crate::MacroScope=>MacroScope);
impl<T: FlibItem> Encode for Idx<T> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        (if T::REMAP {
            e.ids.get(T::KIND, self.into_raw())?
        } else {
            self.into_raw()
        })
        .encode(e)
    }
}
impl<T: FlibItem> Decode for Idx<T> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let raw = u32::decode(d)?;
        Ok(Self::from_raw(if T::REMAP {
            d.ids.get(T::KIND, raw)?
        } else {
            raw
        }))
    }
}
macro_rules! index {
    ($t:ty,$kind:ident) => {
        impl Encode for $t {
            fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
                e.ids.get(IdKind::$kind, self.0)?.encode(e)
            }
        }
        impl Decode for $t {
            fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
                let raw = u32::decode(d)?;
                Ok(Self(d.ids.get(IdKind::$kind, raw)?))
            }
        }
    };
}
impl Encode for crate::FileId {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        if *self == Self::UNKNOWN {
            u32::MAX.encode(e)
        } else {
            e.ids.get(IdKind::File, self.0)?.encode(e)
        }
    }
}
impl Decode for crate::FileId {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let raw = u32::decode(d)?;
        if raw == u32::MAX {
            Ok(Self::UNKNOWN)
        } else {
            Ok(Self(d.ids.get(IdKind::File, raw)?))
        }
    }
}
index!(crate::CrateId, Crate);

impl<K: Encode + Eq + std::hash::Hash, V: Encode, S: std::hash::BuildHasher> Encode
    for std::collections::HashMap<K, V, S>
{
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        let mut entries = Vec::new();
        for (key, value) in self {
            let mut wire = Encoder::with_ids(e.ids.clone());
            wire.depth = e.depth;
            key.encode(&mut wire)?;
            entries.push((wire.into_bytes(), key, value));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.len().encode(e)?;
        for (_, key, value) in entries {
            key.encode(e)?;
            value.encode(e)?;
        }
        Ok(())
    }
}
impl<K: Decode + Eq + std::hash::Hash, V: Decode, S: std::hash::BuildHasher + Default> Decode
    for std::collections::HashMap<K, V, S>
{
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let n = d.length()?;
        let mut result = Self::default();
        for _ in 0..n {
            if result.insert(K::decode(d)?, V::decode(d)?).is_some() {
                return Err(Error("duplicate metadata map key"));
            }
        }
        Ok(result)
    }
}
impl Encode for flux_syntax::Edition {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        let value = match self {
            Self::E2015 => 0u8,
            Self::E2018 => 1,
            Self::E2021 => 2,
            Self::E2024 => 3,
        };
        value.encode(e)
    }
}
impl Decode for flux_syntax::Edition {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(Self::E2015),
            1 => Ok(Self::E2018),
            2 => Ok(Self::E2021),
            3 => Ok(Self::E2024),
            _ => Err(Error("unsupported metadata edition")),
        }
    }
}
impl Encode for crate::Intrinsic {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.name().encode(e)
    }
}
impl Decode for crate::Intrinsic {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Self::from_name(&String::decode(d)?).ok_or(Error("unsupported intrinsic identity"))
    }
}

impl<T: Encode> Encode for std::sync::Arc<T> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        (**self).encode(e)
    }
}
impl<T: Decode> Decode for std::sync::Arc<T> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self::new(T::decode(d)?))
    }
}
impl<K: Encode + Ord, V: Encode> Encode for BTreeMap<K, V> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.len().encode(e)?;
        for (k, v) in self {
            k.encode(e)?;
            v.encode(e)?;
        }
        Ok(())
    }
}
impl<K: Decode + Ord, V: Decode> Decode for BTreeMap<K, V> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let n = d.length()?;
        let mut map = Self::new();
        for _ in 0..n {
            if map.insert(K::decode(d)?, V::decode(d)?).is_some() {
                return Err(Error("duplicate metadata map key"));
            }
        }
        Ok(map)
    }
}
impl Encode for flux_syntax::CfgOptions {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.names.encode(e)?;
        self.values.encode(e)
    }
}
impl Decode for flux_syntax::CfgOptions {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            names: Decode::decode(d)?,
            values: Decode::decode(d)?,
        })
    }
}

impl Encode for crate::TrackedInputs {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.files.len().encode(e)?;
        for (path, digest) in &self.files {
            path.encode(e)?;
            digest.as_slice().encode(e)?;
        }
        self.environment.encode(e)?;
        self.macro_outputs.encode(e)
    }
}
impl Decode for crate::TrackedInputs {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let count = d.length()?;
        let mut files = BTreeMap::new();
        for _ in 0..count {
            let path = Decode::decode(d)?;
            let bytes = Vec::<u8>::decode(d)?;
            let digest = bytes
                .try_into()
                .map_err(|_| Error("invalid tracked file digest"))?;
            if files.insert(path, digest).is_some() {
                return Err(Error("duplicate tracked input path"));
            }
        }
        Ok(Self {
            files,
            environment: Decode::decode(d)?,
            macro_outputs: Decode::decode(d)?,
        })
    }
}
