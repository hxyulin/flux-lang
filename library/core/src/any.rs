//! Runtime type identity and checked reflection of static types.

/// Opaque identity of a type. Its bits and ordering are unspecified.
#[derive(Clone, Copy)]
pub struct TypeId {
    value: u128,
}

impl TypeId {
    pub const fn of<T: ?Sized + 'static>() -> Self {
        Self {
            value: crate::intrinsics::type_id::<T>(),
        }
    }
}

impl PartialEq for TypeId {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for TypeId {}
impl PartialOrd for TypeId {
    fn partial_cmp(&self, other: &Self) -> Option<crate::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TypeId {
    fn cmp(&self, other: &Self) -> crate::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}
impl crate::hash::Hash for TypeId {
    fn hash<H: crate::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}
impl crate::fmt::Debug for TypeId {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        write!(f, "TypeId(0x{:032x})", self.value)
    }
}

/// Types without non-static borrowed data, supporting checked downcasts.
pub trait Any: 'static {
    fn type_id(&self) -> TypeId;
}

impl<T: ?Sized + 'static> Any for T {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

impl dyn Any {
    pub fn is<T: Any>(&self) -> bool {
        <Self as Any>::type_id(self) == TypeId::of::<T>()
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: the dynamic identity proves the data pointer contains T.
            Some(unsafe { &*(self as *const dyn Any as *const T) })
        } else {
            None
        }
    }

    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            // SAFETY: identity proves T and the receiver provides exclusivity.
            Some(unsafe { &mut *(self as *mut dyn Any as *mut T) })
        } else {
            None
        }
    }
}

impl dyn Any + Send {
    pub fn is<T: Any>(&self) -> bool {
        <Self as Any>::type_id(self) == TypeId::of::<T>()
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: the dynamic identity proves the data pointer contains T.
            Some(unsafe { &*(self as *const (dyn Any + Send) as *const T) })
        } else {
            None
        }
    }

    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            // SAFETY: identity proves T and the receiver provides exclusivity.
            Some(unsafe { &mut *(self as *mut (dyn Any + Send) as *mut T) })
        } else {
            None
        }
    }
}

impl dyn Any + Send + Sync {
    pub fn is<T: Any>(&self) -> bool {
        <Self as Any>::type_id(self) == TypeId::of::<T>()
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: the dynamic identity proves the data pointer contains T.
            Some(unsafe { &*(self as *const (dyn Any + Send + Sync) as *const T) })
        } else {
            None
        }
    }

    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            // SAFETY: identity proves T and the receiver provides exclusivity.
            Some(unsafe { &mut *(self as *mut (dyn Any + Send + Sync) as *mut T) })
        } else {
            None
        }
    }
}

/// Returns a best-effort description of `T` with static storage.
///
/// Spelling is implementation-specific and lifetimes are omitted. The result
/// is not a unique identifier and must not be used to decide type identity.
/// Flux currently uses its diagnostic type printer, which may omit paths.
/// This function is not const, matching the stable Rust API.
pub fn type_name<T: ?Sized>() -> &'static str {
    crate::intrinsics::type_name::<T>()
}

impl crate::fmt::Debug for dyn Any {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("Any { .. }")
    }
}
impl crate::fmt::Debug for dyn Any + Send {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("Any { .. }")
    }
}
impl crate::fmt::Debug for dyn Any + Send + Sync {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("Any { .. }")
    }
}
