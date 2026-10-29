//! Safe, typed allocation caches. Process pointer keys never enter the wire format.

use std::any::Any;
use std::collections::{BTreeMap, btree_map::Entry};
use std::fmt;

use super::{Decoder, Encoder, Error, MAX_NESTING_DEPTH, MAX_RECORD_LENGTH};

pub(super) type AllocationKey = (u8, usize);
type ErasedOwner = Box<dyn Any + Send + Sync>;

#[derive(Debug)]
enum WireState {
    Unassigned,
    Pending(u32),
    Complete(u32),
}

pub(super) struct AllocationMemo {
    owner: ErasedOwner,
    depth: Option<u32>,
    wire: WireState,
}
impl fmt::Debug for AllocationMemo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AllocationMemo")
            .field("depth", &self.depth)
            .field("wire", &self.wire)
            .finish()
    }
}

pub(super) enum DecodedAllocation {
    Pending {
        domain: u8,
    },
    Complete {
        domain: u8,
        owner: ErasedOwner,
        depth: u32,
    },
}
impl fmt::Debug for DecodedAllocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending { domain } => f
                .debug_struct("PendingAllocation")
                .field("domain", domain)
                .finish(),
            Self::Complete { domain, depth, .. } => f
                .debug_struct("CompleteAllocation")
                .field("domain", domain)
                .field("depth", depth)
                .finish(),
        }
    }
}

fn validate_depth(depth: u32) -> Result<(), Error> {
    if depth == 0 || depth > MAX_NESTING_DEPTH {
        Err(Error("allocation semantic nesting limit exceeded"))
    } else {
        Ok(())
    }
}
fn retain<T: Clone + Send + Sync + 'static>(
    memo: &mut BTreeMap<AllocationKey, AllocationMemo>,
    key: AllocationKey,
    owner: T,
) -> Result<&mut AllocationMemo, Error> {
    let full = memo.len() >= MAX_RECORD_LENGTH;
    let entry = match memo.entry(key) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            if full {
                return Err(Error("allocation table limit exceeded"));
            }
            entry.insert(AllocationMemo {
                owner: Box::new(owner),
                depth: None,
                wire: WireState::Unassigned,
            })
        }
    };
    if entry.owner.as_ref().downcast_ref::<T>().is_none() {
        return Err(Error("allocation cache type mismatch"));
    }
    Ok(entry)
}
fn set_depth(entry: &mut AllocationMemo, depth: u32) -> Result<(), Error> {
    validate_depth(depth)?;
    if entry.depth.is_some_and(|old| old != depth) {
        return Err(Error("inconsistent allocation semantic depth"));
    }
    entry.depth = Some(depth);
    Ok(())
}

impl Encoder {
    /// Holds an allocation owner so its pointer key cannot be recycled during encoding.
    /// `raw` identifies the allocation retained by `owner`; it is never serialized.
    pub fn retain_allocation<T: Clone + Send + Sync + 'static>(
        &mut self,
        domain: u8,
        raw: usize,
        owner: T,
    ) -> Result<(), Error> {
        retain(&mut self.allocations, (domain, raw), owner)?;
        Ok(())
    }
    /// Finds a completed wire slot; an in-progress allocation is an invalid cycle.
    pub fn allocation_reference(
        &self,
        domain: u8,
        raw: usize,
    ) -> Result<Option<(u32, u32)>, Error> {
        let Some(entry) = self.allocations.get(&(domain, raw)) else {
            return Ok(None);
        };
        match entry.wire {
            WireState::Unassigned => Ok(None),
            WireState::Pending(_) => Err(Error("recursive allocation metadata")),
            WireState::Complete(wire) => Ok(Some((
                wire,
                entry
                    .depth
                    .ok_or(Error("missing allocation semantic depth"))?,
            ))),
        }
    }
    /// Reserves a sequential wire slot while retaining the owning Arc clone.
    pub fn reserve_allocation<T: Clone + Send + Sync + 'static>(
        &mut self,
        domain: u8,
        raw: usize,
        owner: T,
    ) -> Result<u32, Error> {
        let entry = retain(&mut self.allocations, (domain, raw), owner)?;
        if !matches!(entry.wire, WireState::Unassigned) {
            return Err(Error("duplicate allocation definition"));
        }
        if self.next_allocation as usize >= MAX_RECORD_LENGTH {
            return Err(Error("allocation table limit exceeded"));
        }
        let wire = self.next_allocation;
        self.next_allocation += 1;
        entry.wire = WireState::Pending(wire);
        Ok(wire)
    }
    /// Publishes a fully encoded allocation after its descendants validate.
    pub fn complete_allocation(&mut self, domain: u8, raw: usize, depth: u32) -> Result<(), Error> {
        let entry = self
            .allocations
            .get_mut(&(domain, raw))
            .ok_or(Error("unreserved allocation definition"))?;
        let wire = match entry.wire {
            WireState::Pending(wire) => wire,
            _ => return Err(Error("unreserved allocation completion")),
        };
        set_depth(entry, depth)?;
        entry.wire = WireState::Complete(wire);
        Ok(())
    }
    /// Returns memoized depth independently of wire-slot assignment.
    pub fn allocation_depth(&self, domain: u8, raw: usize) -> Option<u32> {
        self.allocations
            .get(&(domain, raw))
            .and_then(|entry| entry.depth)
    }
    /// Memoizes a source allocation's depth and retains its owner before encoding.
    pub fn set_allocation_depth<T: Clone + Send + Sync + 'static>(
        &mut self,
        domain: u8,
        raw: usize,
        owner: T,
        depth: u32,
    ) -> Result<(), Error> {
        validate_depth(depth)?;
        set_depth(retain(&mut self.allocations, (domain, raw), owner)?, depth)
    }
}

impl Decoder<'_> {
    /// Reserves the next allocation slot before reading its payload.
    pub fn reserve_allocation(&mut self, domain: u8, wire: u32) -> Result<(), Error> {
        if self.allocations.len() >= MAX_RECORD_LENGTH {
            return Err(Error("allocation table limit exceeded"));
        }
        if wire as usize != self.allocations.len() {
            return Err(Error("invalid allocation definition slot"));
        }
        self.allocations.push(DecodedAllocation::Pending { domain });
        Ok(())
    }
    /// Stores a safe typed owner only after the allocation and its depth validate.
    pub fn complete_allocation<T: Clone + Send + Sync + 'static>(
        &mut self,
        domain: u8,
        wire: u32,
        owner: T,
        depth: u32,
    ) -> Result<(), Error> {
        validate_depth(depth)?;
        let entry = self
            .allocations
            .get_mut(wire as usize)
            .ok_or(Error("unreserved allocation definition"))?;
        match entry {
            DecodedAllocation::Pending { domain: expected } if *expected == domain => {}
            DecodedAllocation::Pending { .. } => return Err(Error("allocation domain mismatch")),
            DecodedAllocation::Complete { .. } => {
                return Err(Error("duplicate allocation completion"));
            }
        }
        *entry = DecodedAllocation::Complete {
            domain,
            owner: Box::new(owner),
            depth,
        };
        Ok(())
    }
    /// Clones an already validated typed owner, preserving Arc allocation identity.
    pub fn allocation_reference<T: Clone + Send + Sync + 'static>(
        &self,
        domain: u8,
        wire: u32,
    ) -> Result<(T, u32), Error> {
        let entry = self
            .allocations
            .get(wire as usize)
            .ok_or(Error("unresolved allocation reference"))?;
        match entry {
            DecodedAllocation::Complete {
                domain: expected,
                owner,
                depth,
            } if *expected == domain => {
                let value = owner
                    .as_ref()
                    .downcast_ref::<T>()
                    .ok_or(Error("allocation cache type mismatch"))?;
                Ok((value.clone(), *depth))
            }
            DecodedAllocation::Complete { .. } => Err(Error("allocation domain mismatch")),
            DecodedAllocation::Pending { .. } => Err(Error("unresolved allocation reference")),
        }
    }
    /// Returns memoized depth of an Arc-backed decoded value.
    pub fn allocation_depth(&self, domain: u8, raw: usize) -> Option<u32> {
        self.allocation_memo
            .get(&(domain, raw))
            .and_then(|entry| entry.depth)
    }
    /// Retains a decoded owner and its depth for parent-depth computation.
    pub fn set_allocation_depth<T: Clone + Send + Sync + 'static>(
        &mut self,
        domain: u8,
        raw: usize,
        owner: T,
        depth: u32,
    ) -> Result<(), Error> {
        validate_depth(depth)?;
        set_depth(
            retain(&mut self.allocation_memo, (domain, raw), owner)?,
            depth,
        )
    }
}
