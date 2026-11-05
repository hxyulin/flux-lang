//! Bounded atomic storage, backed by native atomic instructions.
use crate::cell::UnsafeCell;
use crate::intrinsics;
use crate::marker::{Send, Sync};
use crate::panicking::panic;
use crate::result::Result::{self, Err, Ok};

/// Memory ordering for atomic operations and fences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ordering {
    /// No ordering of other memory accesses.
    Relaxed,
    /// Publishes prior accesses to an acquiring observer.
    Release,
    /// Observes accesses published by a releasing operation.
    Acquire,
    /// Both acquire and release ordering.
    AcqRel,
    /// Acquire/release ordering plus the sequentially consistent total order.
    SeqCst,
}
use self::Ordering::{AcqRel, Acquire, Relaxed, Release, SeqCst};

// Join the two Rust CAS orders without weakening either on Keel's
// single-order instruction. All fifteen valid pairs are accepted.
#[track_caller]
fn cas_order(success:Ordering,failure:Ordering)->Ordering {
    match failure {
        Release=>panic("there is no such thing as a release failure ordering"),
        AcqRel=>panic("there is no such thing as an acquire-release failure ordering"),
        SeqCst=>SeqCst,
        Relaxed=>success,
        Acquire=>match success {Relaxed|Acquire=>Acquire,Release|AcqRel=>AcqRel,SeqCst=>SeqCst},
    }
}
macro_rules! native_cas {
    ($scalar:ty,$relaxed:ident,$acquire:ident,$release:ident,$acqrel:ident,$seqcst:ident)=>{
        #[track_caller]
        pub fn compare_exchange(&self,current:$scalar,new:$scalar,success:Ordering,failure:Ordering)->Result<$scalar,$scalar> {
            // SAFETY: this type owns initialized and naturally aligned storage;
            // the intrinsic accesses exactly that width and returns the old value.
            let old=unsafe {match cas_order(success,failure) {
                Relaxed=>intrinsics::$relaxed(self.value.get(),current,new),
                Acquire=>intrinsics::$acquire(self.value.get(),current,new),
                Release=>intrinsics::$release(self.value.get(),current,new),
                AcqRel=>intrinsics::$acqrel(self.value.get(),current,new),
                SeqCst=>intrinsics::$seqcst(self.value.get(),current,new),
            }};
            if old==current {Ok(old)} else {Err(old)}
        }
        #[track_caller]
        pub fn compare_exchange_weak(&self,current:$scalar,new:$scalar,success:Ordering,failure:Ordering)->Result<$scalar,$scalar> {
            self.compare_exchange(current,new,success,failure)
        }
    };
}
macro_rules! atomic_updates {
    ($scalar:ty)=>{
        #[track_caller]
        pub fn fetch_update<F>(&self,set_order:Ordering,fetch_order:Ordering,mut update:F)->Result<$scalar,$scalar>
        where F:FnMut($scalar)->Option<$scalar> {
            let mut old=self.load(fetch_order);
            loop {
                let Some(new)=update(old) else {return Err(old);};
                match self.compare_exchange_weak(old,new,set_order,fetch_order) {
                    Ok(previous)=>return Ok(previous),Err(observed)=>old=observed,
                }
            }
        }
        #[track_caller]
        pub fn swap(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|_|Some(value)) {Ok(old)|Err(old)=>old}
        }
    };
}
macro_rules! atomic_integer_ops {
    ($scalar:ty)=>{
        #[track_caller]
        pub fn fetch_and(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|old|Some(old & value)) {Ok(old)|Err(old)=>old}
        }
        #[track_caller]
        pub fn fetch_or(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|old|Some(old | value)) {Ok(old)|Err(old)=>old}
        }
        #[track_caller]
        pub fn fetch_xor(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|old|Some(old ^ value)) {Ok(old)|Err(old)=>old}
        }
        #[track_caller]
        pub fn fetch_nand(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|old|Some(!(old & value))) {Ok(old)|Err(old)=>old}
        }
        #[track_caller]
        pub fn fetch_max(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|old|Some(if old>value {old} else {value})) {Ok(old)|Err(old)=>old}
        }
        #[track_caller]
        pub fn fetch_min(&self,value:$scalar,order:Ordering)->$scalar {
            match self.fetch_update(order,Relaxed,|old|Some(if old<value {old} else {value})) {Ok(old)|Err(old)=>old}
        }
    };
}

/// A naturally aligned, sized atomic machine word (64 bits on Flux targets).
/// Shared accesses use only native atomic operations; exclusive access may
/// use `get_mut` or `into_inner`. No wider or overlapping access is permitted.
#[repr(C, align(8))]
pub struct AtomicUsize {
    value: UnsafeCell<usize>,
}

// SAFETY: every shared access to the UnsafeCell is a native atomic operation.
// Send propagates structurally from UnsafeCell<usize>; no generic payload is hidden.
unsafe impl Sync for AtomicUsize {}

impl AtomicUsize {
    /// # Safety
    /// The pointer must be valid and aligned for Self for the returned lifetime.
    /// Shared accesses must use compatible atomic operations for that lifetime.
    pub const unsafe fn from_ptr<'a>(pointer:*mut usize)->&'a Self {
        // SAFETY: the caller supplies layout, lifetime and access guarantees.
        unsafe {&*(pointer as *const Self)}
    }
    atomic_updates!(usize);
    atomic_integer_ops!(usize);
    /// Initializes private storage; subsequent shared accesses are atomic.
    pub const fn new(value: usize) -> AtomicUsize {
        AtomicUsize {
            value: UnsafeCell::new(value),
        }
    }

    /// Exclusive access, with no concurrent atomic accesses possible.
    pub fn get_mut(&mut self) -> &mut usize {
        self.value.get_mut()
    }

    /// Consumes the atomic and returns its value.
    pub fn into_inner(self) -> usize {
        self.value.into_inner()
    }

    /// Loads with Relaxed, Acquire or SeqCst; other orders panic.
    #[track_caller]
    pub fn load(&self, order: Ordering) -> usize {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_load_relaxed(self.value.get()),
                Acquire => intrinsics::__atomic_usize_load_acquire(self.value.get()),
                SeqCst => intrinsics::__atomic_usize_load_seqcst(self.value.get()),
                Release => panic("there is no such thing as a release load"),
                AcqRel => panic("there is no such thing as an acquire-release load"),
            }
        }
    }

    /// Stores with Relaxed, Release or SeqCst; other orders panic.
    #[track_caller]
    pub fn store(&self, value: usize, order: Ordering) {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_store_relaxed(self.value.get(), value),
                Release => intrinsics::__atomic_usize_store_release(self.value.get(), value),
                SeqCst => intrinsics::__atomic_usize_store_seqcst(self.value.get(), value),
                Acquire => panic("there is no such thing as an acquire store"),
                AcqRel => panic("there is no such thing as an acquire-release store"),
            }
        }
    }

    /// Adds with wrapping arithmetic and returns the previous value.
    #[track_caller]
    pub fn fetch_add(&self, value: usize, order: Ordering) -> usize {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_add_relaxed(self.value.get(), value),
                Acquire => intrinsics::__atomic_usize_add_acquire(self.value.get(), value),
                Release => intrinsics::__atomic_usize_add_release(self.value.get(), value),
                AcqRel => intrinsics::__atomic_usize_add_acqrel(self.value.get(), value),
                SeqCst => intrinsics::__atomic_usize_add_seqcst(self.value.get(), value),
            }
        }
    }

    /// Subtracts with wrapping arithmetic and returns the previous value.
    #[track_caller]
    pub fn fetch_sub(&self, value: usize, order: Ordering) -> usize {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_sub_relaxed(self.value.get(), value),
                Acquire => intrinsics::__atomic_usize_sub_acquire(self.value.get(), value),
                Release => intrinsics::__atomic_usize_sub_release(self.value.get(), value),
                AcqRel => intrinsics::__atomic_usize_sub_acqrel(self.value.get(), value),
                SeqCst => intrinsics::__atomic_usize_sub_seqcst(self.value.get(), value),
            }
        }
    }

    /// Replaces `current`, returning the old value in Ok on success or Err
    /// on failure. Failure Release and AcqRel panic; all other pairs are valid.
    #[track_caller]
    pub fn compare_exchange(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        // Keel has one order: join both Rust orders without weakening either.
        // Validate failure before mapping; all fifteen remaining pairs are valid.
        let order = cas_order(success,failure);
        // SAFETY: aligned, live atomic storage. Pinned Keel CAS is strong and
        // returns the old scalar on BOTH paths, not a boolean or tuple.
        let old = unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_cas_relaxed(self.value.get(), current, new),
                Acquire => intrinsics::__atomic_usize_cas_acquire(self.value.get(), current, new),
                Release => intrinsics::__atomic_usize_cas_release(self.value.get(), current, new),
                AcqRel => intrinsics::__atomic_usize_cas_acqrel(self.value.get(), current, new),
                SeqCst => intrinsics::__atomic_usize_cas_seqcst(self.value.get(), current, new),
            }
        };
        if old == current { Ok(old) } else { Err(old) }
    }

    /// This bounded implementation uses strong CAS, which is allowed for weak CAS.
    #[track_caller]
    pub fn compare_exchange_weak(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.compare_exchange(current, new, success, failure)
    }
}

/// An atomic 64-bit unsigned word on Flux's 64-bit targets.
#[repr(C, align(8))]
pub struct AtomicU64 {
    value: AtomicUsize,
}

impl AtomicU64 {
    /// # Safety
    /// The pointer must be valid and aligned for Self for the returned lifetime.
    /// Shared accesses must use compatible atomic operations for that lifetime.
    pub const unsafe fn from_ptr<'a>(pointer:*mut u64)->&'a Self {
        // SAFETY: the caller supplies layout, lifetime and access guarantees.
        unsafe {&*(pointer as *const Self)}
    }
    atomic_updates!(u64);
    atomic_integer_ops!(u64);
    pub const fn new(value: u64) -> Self {
        Self { value: AtomicUsize::new(value as usize) }
    }
    pub fn get_mut(&mut self) -> &mut u64 {
        // SAFETY: usize and u64 have identical layout on every Flux target;
        // exclusive access excludes atomic operations for this borrow.
        unsafe { &mut *(self.value.get_mut() as *mut usize as *mut u64) }
    }
    pub fn into_inner(self) -> u64 { self.value.into_inner() as u64 }
    #[track_caller]
    pub fn load(&self, order: Ordering) -> u64 { self.value.load(order) as u64 }
    #[track_caller]
    pub fn store(&self, value: u64, order: Ordering) { self.value.store(value as usize, order); }
    #[track_caller]
    pub fn fetch_add(&self, value: u64, order: Ordering) -> u64 { self.value.fetch_add(value as usize, order) as u64 }
    #[track_caller]
    pub fn fetch_sub(&self, value: u64, order: Ordering) -> u64 { self.value.fetch_sub(value as usize, order) as u64 }
    #[track_caller]
    pub fn compare_exchange(&self, current: u64, new: u64, success: Ordering, failure: Ordering) -> Result<u64, u64> {
        match self.value.compare_exchange(current as usize, new as usize, success, failure) {
            Ok(old) => Ok(old as u64), Err(old) => Err(old as u64),
        }
    }
    #[track_caller]
    pub fn compare_exchange_weak(&self, current: u64, new: u64, success: Ordering, failure: Ordering) -> Result<u64, u64> {
        self.compare_exchange(current, new, success, failure)
    }
}
impl crate::default::Default for AtomicU64 {
    fn default() -> Self { Self::new(0) }
}
impl crate::convert::From<u64> for AtomicU64 {
    fn from(value: u64) -> Self { Self::new(value) }
}
impl crate::fmt::Debug for AtomicU64 {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Debug::fmt(&self.load(Relaxed), f)
    }
}

/// A naturally aligned, sized atomic 32-bit unsigned word.
/// Shared accesses use only native atomic operations; exclusive access may
/// use `get_mut` or `into_inner`. No wider or overlapping access is permitted.
#[repr(C, align(4))]
pub struct AtomicU32 {
    value: UnsafeCell<u32>,
}

// SAFETY: every shared access to the UnsafeCell is a native atomic operation.
// Send propagates structurally from UnsafeCell<u32>; no generic payload is hidden.
unsafe impl Sync for AtomicU32 {}

impl AtomicU32 {
    /// # Safety
    /// The pointer must be valid and aligned for Self for the returned lifetime.
    /// Shared accesses must use compatible atomic operations for that lifetime.
    pub const unsafe fn from_ptr<'a>(pointer:*mut u32)->&'a Self {
        // SAFETY: the caller supplies layout, lifetime and access guarantees.
        unsafe {&*(pointer as *const Self)}
    }
    atomic_updates!(u32);
    atomic_integer_ops!(u32);
    native_cas!(u32,__atomic_u32_cas_relaxed,__atomic_u32_cas_acquire,__atomic_u32_cas_release,__atomic_u32_cas_acqrel,__atomic_u32_cas_seqcst);
    #[track_caller]
    pub fn fetch_sub(&self,value:u32,order:Ordering)->u32 {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_sub(value))) {Ok(old)|Err(old)=>old}}
    /// Initializes private storage; subsequent shared accesses are atomic.
    pub const fn new(value: u32) -> AtomicU32 {
        AtomicU32 {
            value: UnsafeCell::new(value),
        }
    }

    /// Exclusive access, with no concurrent atomic accesses possible.
    pub fn get_mut(&mut self) -> &mut u32 {
        self.value.get_mut()
    }

    /// Consumes the atomic and returns its value.
    pub fn into_inner(self) -> u32 {
        self.value.into_inner()
    }

    /// Loads with Relaxed, Acquire or SeqCst; other orders panic.
    #[track_caller]
    pub fn load(&self, order: Ordering) -> u32 {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_u32_load_relaxed(self.value.get()),
                Acquire => intrinsics::__atomic_u32_load_acquire(self.value.get()),
                SeqCst => intrinsics::__atomic_u32_load_seqcst(self.value.get()),
                Release => panic("there is no such thing as a release load"),
                AcqRel => panic("there is no such thing as an acquire-release load"),
            }
        }
    }

    /// Stores with Relaxed, Release or SeqCst; other orders panic.
    #[track_caller]
    pub fn store(&self, value: u32, order: Ordering) {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_u32_store_relaxed(self.value.get(), value),
                Release => intrinsics::__atomic_u32_store_release(self.value.get(), value),
                SeqCst => intrinsics::__atomic_u32_store_seqcst(self.value.get(), value),
                Acquire => panic("there is no such thing as an acquire store"),
                AcqRel => panic("there is no such thing as an acquire-release store"),
            }
        }
    }

    /// Adds with wrapping arithmetic and returns the previous value.
    #[track_caller]
    pub fn fetch_add(&self, value: u32, order: Ordering) -> u32 {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_u32_add_relaxed(self.value.get(), value),
                Acquire => intrinsics::__atomic_u32_add_acquire(self.value.get(), value),
                Release => intrinsics::__atomic_u32_add_release(self.value.get(), value),
                AcqRel => intrinsics::__atomic_u32_add_acqrel(self.value.get(), value),
                SeqCst => intrinsics::__atomic_u32_add_seqcst(self.value.get(), value),
            }
        }
    }
}

/// A byte-sized atomic with the same size and alignment as u8.
#[repr(C, align(1))]
pub struct AtomicU8 { value: UnsafeCell<u8> }
// SAFETY: every shared storage access uses a native byte atomic instruction.
unsafe impl Sync for AtomicU8 {}
impl AtomicU8 {
    /// # Safety
    /// The pointer must be valid and aligned for Self for the returned lifetime.
    /// Shared accesses must use compatible atomic operations for that lifetime.
    pub const unsafe fn from_ptr<'a>(pointer:*mut u8)->&'a Self {
        // SAFETY: the caller supplies layout, lifetime and access guarantees.
        unsafe {&*(pointer as *const Self)}
    }
    atomic_updates!(u8);
    atomic_integer_ops!(u8);
    #[track_caller]
    pub fn fetch_add(&self,value:u8,order:Ordering)->u8 {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_add(value))) {Ok(old)|Err(old)=>old}}
    #[track_caller]
    pub fn fetch_sub(&self,value:u8,order:Ordering)->u8 {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_sub(value))) {Ok(old)|Err(old)=>old}}
    pub const fn new(value:u8)->Self{Self{value:UnsafeCell::new(value)}}
    pub fn get_mut(&mut self)->&mut u8{self.value.get_mut()}
    pub fn into_inner(self)->u8{self.value.into_inner()}
    #[track_caller]
    pub fn load(&self,order:Ordering)->u8{
        // SAFETY: the live byte storage is initialized; each branch uses an
        // atomic load of exactly its declared width and a valid ordering.
        unsafe{match order{
            Relaxed=>intrinsics::__atomic_u8_load_relaxed(self.value.get()),
            Acquire=>intrinsics::__atomic_u8_load_acquire(self.value.get()),
            SeqCst=>intrinsics::__atomic_u8_load_seqcst(self.value.get()),
            Release=>panic("there is no such thing as a release load"),
            AcqRel=>panic("there is no such thing as an acquire-release load"),
        }}
    }
    #[track_caller]
    pub fn store(&self,value:u8,order:Ordering){
        // SAFETY: the live byte storage is initialized and all shared accesses
        // use atomic stores of exactly one byte with a validated ordering.
        unsafe{match order{
            Relaxed=>intrinsics::__atomic_u8_store_relaxed(self.value.get(),value),
            Release=>intrinsics::__atomic_u8_store_release(self.value.get(),value),
            SeqCst=>intrinsics::__atomic_u8_store_seqcst(self.value.get(),value),
            Acquire=>panic("there is no such thing as an acquire store"),
            AcqRel=>panic("there is no such thing as an acquire-release store"),
        }}
    }
    /// Replaces `current`, returning the old value in Ok on success or Err
    /// on failure. Failure Release and AcqRel panic; all other pairs are valid.
    #[track_caller]
    pub fn compare_exchange(
        &self,
        current: u8,
        new: u8,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u8, u8> {
        // Keel has one order: join both Rust orders without weakening either.
        // Validate failure before mapping; all fifteen remaining pairs are valid.
        let order = cas_order(success,failure);
        // SAFETY: aligned, live atomic storage. Pinned Keel CAS is strong and
        // returns the old scalar on BOTH paths, not a boolean or tuple.
        let old = unsafe {
            match order {
                Relaxed => intrinsics::__atomic_u8_cas_relaxed(self.value.get(), current, new),
                Acquire => intrinsics::__atomic_u8_cas_acquire(self.value.get(), current, new),
                Release => intrinsics::__atomic_u8_cas_release(self.value.get(), current, new),
                AcqRel => intrinsics::__atomic_u8_cas_acqrel(self.value.get(), current, new),
                SeqCst => intrinsics::__atomic_u8_cas_seqcst(self.value.get(), current, new),
            }
        };
        if old == current { Ok(old) } else { Err(old) }
    }

    /// This bounded implementation uses strong CAS, which is allowed for weak CAS.
    #[track_caller]
    pub fn compare_exchange_weak(
        &self,
        current: u8,
        new: u8,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u8, u8> {
        self.compare_exchange(current, new, success, failure)
    }
}
impl crate::default::Default for AtomicU8{fn default()->Self{Self::new(0)}}
impl crate::convert::From<u8> for AtomicU8{fn from(value:u8)->Self{Self::new(value)}}
impl crate::fmt::Debug for AtomicU8{
    fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result{crate::fmt::Debug::fmt(&self.load(Relaxed),f)}
}

/// A two-byte atomic with the same size and alignment as u16.
#[repr(C, align(2))]
pub struct AtomicU16 { value: UnsafeCell<u16> }
// SAFETY: every shared storage access uses a native halfword atomic instruction.
unsafe impl Sync for AtomicU16 {}
impl AtomicU16 {
    /// # Safety
    /// The pointer must be valid and aligned for Self for the returned lifetime.
    /// Shared accesses must use compatible atomic operations for that lifetime.
    pub const unsafe fn from_ptr<'a>(pointer:*mut u16)->&'a Self {
        // SAFETY: the caller supplies layout, lifetime and access guarantees.
        unsafe {&*(pointer as *const Self)}
    }
    atomic_updates!(u16);
    atomic_integer_ops!(u16);
    native_cas!(u16,__atomic_u16_cas_relaxed,__atomic_u16_cas_acquire,__atomic_u16_cas_release,__atomic_u16_cas_acqrel,__atomic_u16_cas_seqcst);
    #[track_caller]
    pub fn fetch_add(&self,value:u16,order:Ordering)->u16 {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_add(value))) {Ok(old)|Err(old)=>old}}
    #[track_caller]
    pub fn fetch_sub(&self,value:u16,order:Ordering)->u16 {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_sub(value))) {Ok(old)|Err(old)=>old}}
    pub const fn new(value:u16)->Self{Self{value:UnsafeCell::new(value)}}
    pub fn get_mut(&mut self)->&mut u16{self.value.get_mut()}
    pub fn into_inner(self)->u16{self.value.into_inner()}
    #[track_caller]
    pub fn load(&self,order:Ordering)->u16{
        // SAFETY: the live halfword storage is initialized; each branch uses an
        // atomic load of exactly its declared width and a valid ordering.
        unsafe{match order{
            Relaxed=>intrinsics::__atomic_u16_load_relaxed(self.value.get()),
            Acquire=>intrinsics::__atomic_u16_load_acquire(self.value.get()),
            SeqCst=>intrinsics::__atomic_u16_load_seqcst(self.value.get()),
            Release=>panic("there is no such thing as a release load"),
            AcqRel=>panic("there is no such thing as an acquire-release load"),
        }}
    }
    #[track_caller]
    pub fn store(&self,value:u16,order:Ordering){
        // SAFETY: the live halfword storage is initialized and all shared accesses
        // use atomic stores of exactly two bytes with a validated ordering.
        unsafe{match order{
            Relaxed=>intrinsics::__atomic_u16_store_relaxed(self.value.get(),value),
            Release=>intrinsics::__atomic_u16_store_release(self.value.get(),value),
            SeqCst=>intrinsics::__atomic_u16_store_seqcst(self.value.get(),value),
            Acquire=>panic("there is no such thing as an acquire store"),
            AcqRel=>panic("there is no such thing as an acquire-release store"),
        }}
    }
}
impl crate::default::Default for AtomicU16{fn default()->Self{Self::new(0)}}
impl crate::convert::From<u16> for AtomicU16{fn from(value:u16)->Self{Self::new(value)}}
impl crate::fmt::Debug for AtomicU16{
    fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result{crate::fmt::Debug::fmt(&self.load(Relaxed),f)}
}

/// An atomic fence. Relaxed is invalid and panics.
#[track_caller]
pub fn fence(order: Ordering) {
    // SAFETY: fences have no storage preconditions.
    unsafe {
        match order {
            Acquire => intrinsics::__atomic_usize_fence_acquire(),
            Release => intrinsics::__atomic_usize_fence_release(),
            AcqRel => intrinsics::__atomic_usize_fence_acqrel(),
            SeqCst => intrinsics::__atomic_usize_fence_seqcst(),
            Relaxed => panic("there is no such thing as a relaxed fence"),
        }
    }
}

/// An atomic thin pointer to a sized value. This does not own the pointee.
/// Shared accesses use only native atomic operations; exclusive access may
/// use `get_mut` or `into_inner`. No wider or overlapping access is permitted.
#[repr(C, align(8))]
pub struct AtomicPtr<T> {
    value: UnsafeCell<*mut T>,
}

// SAFETY: every shared access to the UnsafeCell is a native atomic operation.
unsafe impl<T> Sync for AtomicPtr<T> {}
// SAFETY: transferring pointer storage does not access or transfer the pointee.
unsafe impl<T> Send for AtomicPtr<T> {}

impl<T> AtomicPtr<T> {
    /// # Safety
    /// The pointer must remain valid, aligned and accessed atomically for 'a.
    pub const unsafe fn from_ptr<'a>(pointer:*mut *mut T)->&'a Self {
        // SAFETY: the caller supplies the allocation and atomic-access contract.
        unsafe {&*(pointer as *const Self)}
    }
    atomic_updates!(*mut T);
    native_cas!(*mut T,__atomic_ptr_cas_relaxed,__atomic_ptr_cas_acquire,__atomic_ptr_cas_release,__atomic_ptr_cas_acqrel,__atomic_ptr_cas_seqcst);
    /// Initializes private storage; subsequent shared accesses are atomic.
    pub const fn new(value: *mut T) -> AtomicPtr<T> {
        AtomicPtr {
            value: UnsafeCell::new(value),
        }
    }

    /// Exclusive access, with no concurrent atomic accesses possible.
    pub fn get_mut(&mut self) -> &mut *mut T {
        self.value.get_mut()
    }

    /// Consumes the atomic and returns its value.
    pub fn into_inner(self) -> *mut T {
        self.value.into_inner()
    }

    /// Loads with Relaxed, Acquire or SeqCst; other orders panic.
    #[track_caller]
    pub fn load(&self, order: Ordering) -> *mut T {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_ptr_load_relaxed(self.value.get()),
                Acquire => intrinsics::__atomic_ptr_load_acquire(self.value.get()),
                SeqCst => intrinsics::__atomic_ptr_load_seqcst(self.value.get()),
                Release => panic("there is no such thing as a release load"),
                AcqRel => panic("there is no such thing as an acquire-release load"),
            }
        }
    }

    /// Stores with Relaxed, Release or SeqCst; other orders panic.
    #[track_caller]
    pub fn store(&self, value: *mut T, order: Ordering) {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_ptr_store_relaxed(self.value.get(), value),
                Release => intrinsics::__atomic_ptr_store_release(self.value.get(), value),
                SeqCst => intrinsics::__atomic_ptr_store_seqcst(self.value.get(), value),
                Acquire => panic("there is no such thing as an acquire store"),
                AcqRel => panic("there is no such thing as an acquire-release store"),
            }
        }
    }
}

// Signed atomics retain exactly the unsigned storage width. All bit patterns
// are valid in both signed and unsigned integers of the same width.
macro_rules! signed_atomic {
    ($name:ident, $signed:ty, $unsigned:ty, $storage:ident, $alignment:literal) => {
        #[repr(C, align($alignment))]
        pub struct $name { value: $storage }
        impl $name {
            /// # Safety
            /// The storage must be valid, aligned and accessed atomically for 'a.
            pub const unsafe fn from_ptr<'a>(pointer:*mut $signed)->&'a Self {
                // SAFETY: the caller supplies compatible storage and its lifetime.
                unsafe {&*(pointer as *const Self)}
            }
            atomic_updates!($signed);
            atomic_integer_ops!($signed);
            #[track_caller]
            pub fn compare_exchange(&self,current:$signed,new:$signed,success:Ordering,failure:Ordering)->Result<$signed,$signed> {
                match self.value.compare_exchange(current as $unsigned,new as $unsigned,success,failure) {Ok(old)=>Ok(old as $signed),Err(old)=>Err(old as $signed)}
            }
            #[track_caller]
            pub fn compare_exchange_weak(&self,current:$signed,new:$signed,success:Ordering,failure:Ordering)->Result<$signed,$signed> {
                self.compare_exchange(current,new,success,failure)
            }
            #[track_caller]
            pub fn fetch_add(&self,value:$signed,order:Ordering)->$signed {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_add(value))) {Ok(old)|Err(old)=>old}}
            #[track_caller]
            pub fn fetch_sub(&self,value:$signed,order:Ordering)->$signed {match self.fetch_update(order,Relaxed,|old|Some(old.wrapping_sub(value))) {Ok(old)|Err(old)=>old}}
            pub const fn new(value:$signed)->Self { Self { value:$storage::new(value as $unsigned) } }
            pub fn get_mut(&mut self)->&mut $signed {
                // SAFETY: signed/unsigned types have identical size/alignment
                // and every bit pattern is valid. Access is exclusive.
                unsafe { &mut *(self.value.get_mut() as *mut $unsigned as *mut $signed) }
            }
            pub fn into_inner(self)->$signed { self.value.into_inner() as $signed }
            #[track_caller]
            pub fn load(&self,order:Ordering)->$signed { self.value.load(order) as $signed }
            #[track_caller]
            pub fn store(&self,value:$signed,order:Ordering) { self.value.store(value as $unsigned,order); }
        }
        impl crate::default::Default for $name { fn default()->Self { Self::new(0) } }
        impl crate::convert::From<$signed> for $name { fn from(value:$signed)->Self { Self::new(value) } }
        impl crate::fmt::Debug for $name {
            fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result { crate::fmt::Debug::fmt(&self.load(Relaxed),f) }
        }
    };
}
signed_atomic!(AtomicI8,i8,u8,AtomicU8,1);
signed_atomic!(AtomicI16,i16,u16,AtomicU16,2);
signed_atomic!(AtomicI32,i32,u32,AtomicU32,4);
signed_atomic!(AtomicI64,i64,u64,AtomicU64,8);
signed_atomic!(AtomicIsize,isize,usize,AtomicUsize,8);

/// A one-byte boolean atomic. Its private backing byte always contains 0 or 1.
#[repr(C,align(1))]
pub struct AtomicBool { value:AtomicU8 }
impl AtomicBool {
    /// # Safety
    /// The pointer must be valid and aligned for Self for the returned lifetime.
    /// Shared accesses must use compatible atomic operations for that lifetime.
    pub const unsafe fn from_ptr<'a>(pointer:*mut bool)->&'a Self {
        // SAFETY: the caller supplies layout, lifetime and access guarantees.
        unsafe {&*(pointer as *const Self)}
    }
    #[track_caller]
    pub fn fetch_nand(&self,value:bool,order:Ordering)->bool {match self.fetch_update(order,Relaxed,|old|Some(!(old & value))) {Ok(old)|Err(old)=>old}}
    #[track_caller]
    pub fn fetch_xor(&self,value:bool,order:Ordering)->bool {match self.fetch_update(order,Relaxed,|old|Some(old ^ value)) {Ok(old)|Err(old)=>old}}
    #[track_caller]
    pub fn fetch_or(&self,value:bool,order:Ordering)->bool {match self.fetch_update(order,Relaxed,|old|Some(old | value)) {Ok(old)|Err(old)=>old}}
    #[track_caller]
    pub fn fetch_and(&self,value:bool,order:Ordering)->bool {match self.fetch_update(order,Relaxed,|old|Some(old & value)) {Ok(old)|Err(old)=>old}}
    atomic_updates!(bool);
    #[track_caller]
    pub fn compare_exchange(&self,current:bool,new:bool,success:Ordering,failure:Ordering)->Result<bool,bool> {
        match self.value.compare_exchange(current as u8,new as u8,success,failure) {Ok(old)=>Ok(old!=0),Err(old)=>Err(old!=0)}
    }
    #[track_caller]
    pub fn compare_exchange_weak(&self,current:bool,new:bool,success:Ordering,failure:Ordering)->Result<bool,bool> {self.compare_exchange(current,new,success,failure)}

    pub const fn new(value:bool)->Self { Self { value:AtomicU8::new(value as u8) } }
    pub fn get_mut(&mut self)->&mut bool {
        // SAFETY: only bool constructors/stores initialize this private byte,
        // so it has a valid bool representation. Exclusive access is retained.
        unsafe { &mut *(self.value.get_mut() as *mut u8 as *mut bool) }
    }
    pub fn into_inner(self)->bool { self.value.into_inner()!=0 }
    #[track_caller]
    pub fn load(&self,order:Ordering)->bool { self.value.load(order)!=0 }
    #[track_caller]
    pub fn store(&self,value:bool,order:Ordering) { self.value.store(value as u8,order); }
}
impl crate::default::Default for AtomicBool { fn default()->Self { Self::new(false) } }
impl crate::convert::From<bool> for AtomicBool { fn from(value:bool)->Self { Self::new(value) } }
impl crate::fmt::Debug for AtomicBool {
    fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result { crate::fmt::Debug::fmt(&self.load(Relaxed),f) }
}

impl crate::panic::RefUnwindSafe for AtomicUsize {}
impl crate::panic::RefUnwindSafe for AtomicU64 {}
impl crate::panic::RefUnwindSafe for AtomicU32 {}
impl crate::panic::RefUnwindSafe for AtomicU16 {}
impl crate::panic::RefUnwindSafe for AtomicU8 {}
impl crate::panic::RefUnwindSafe for AtomicBool {}
impl<T> crate::panic::RefUnwindSafe for AtomicPtr<T> {}

impl crate::fmt::Debug for AtomicUsize {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Debug::fmt(&self.load(Relaxed), f)
    }
}
impl crate::fmt::Debug for AtomicU32 {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Debug::fmt(&self.load(Relaxed), f)
    }
}

impl crate::default::Default for AtomicUsize { fn default() -> Self { Self::new(0) } }
impl crate::convert::From<usize> for AtomicUsize { fn from(value: usize) -> Self { Self::new(value) } }
impl crate::default::Default for AtomicU32 { fn default() -> Self { Self::new(0) } }
impl crate::convert::From<u32> for AtomicU32 { fn from(value: u32) -> Self { Self::new(value) } }
impl<T> crate::default::Default for AtomicPtr<T> { fn default() -> Self { Self::new(crate::ptr::null_mut()) } }
impl<T> crate::convert::From<*mut T> for AtomicPtr<T> { fn from(value: *mut T) -> Self { Self::new(value) } }
impl<T> crate::fmt::Debug for AtomicPtr<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Debug::fmt(&self.load(Relaxed), f)
    }
}
impl<T> crate::fmt::Pointer for AtomicPtr<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Pointer::fmt(&self.load(Relaxed), f)
    }
}

/// A fence for the compiler's reordering only. Flux emits a full fence of
/// the same order, which is at least as strong. Relaxed is invalid and panics.
#[track_caller]
pub fn compiler_fence(order: Ordering) {
    match order {
        Relaxed => panic("there is no such thing as a relaxed fence"),
        order => fence(order),
    }
}

/// Signals a spin-wait loop to the processor.
#[deprecated(note = "use hint::spin_loop instead")]
pub fn spin_loop_hint() { crate::hint::spin_loop() }

#[deprecated(note = "the `new` function is now preferred")]
pub const ATOMIC_BOOL_INIT: AtomicBool = AtomicBool::new(false);
#[deprecated(note = "the `new` function is now preferred")]
pub const ATOMIC_USIZE_INIT: AtomicUsize = AtomicUsize::new(0);
#[deprecated(note = "the `new` function is now preferred")]
pub const ATOMIC_ISIZE_INIT: AtomicIsize = AtomicIsize::new(0);
