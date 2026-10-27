//! Monotonic elapsed time for compiler profiling on supported Unix targets.
use core::ops::{Add, Sub};

const NANOS_PER_SEC: u32 = 1_000_000_000;

/// A nonnegative span of time with nanosecond precision.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration {
    secs: u64,
    nanos: u32,
}

impl Duration {
    pub const ZERO: Self = Self::new(0, 0);
    pub const MAX: Self = Self::new(u64::MAX, NANOS_PER_SEC - 1);

    /// Creates a duration, carrying whole seconds from `nanos`.
    ///
    /// # Panics
    /// Panics if the normalized seconds exceed `u64::MAX`.
    pub const fn new(secs: u64, nanos: u32) -> Self {
        let carry = (nanos / NANOS_PER_SEC) as u64;
        let secs = match secs.checked_add(carry) {
            Some(secs) => secs,
            None => panic!("overflow in Duration::new"),
        };
        Self {
            secs,
            nanos: nanos % NANOS_PER_SEC,
        }
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self { secs, nanos: 0 }
    }
    pub const fn from_millis(millis: u64) -> Self {
        Self::new(millis / 1_000, ((millis % 1_000) * 1_000_000) as u32)
    }
    pub const fn from_micros(micros: u64) -> Self {
        Self::new(micros / 1_000_000, ((micros % 1_000_000) * 1_000) as u32)
    }
    pub const fn from_nanos(nanos: u64) -> Self {
        Self::new(
            nanos / NANOS_PER_SEC as u64,
            (nanos % NANOS_PER_SEC as u64) as u32,
        )
    }
    pub const fn as_secs(&self) -> u64 {
        self.secs
    }
    pub const fn subsec_nanos(&self) -> u32 {
        self.nanos
    }
    pub const fn as_nanos(&self) -> u128 {
        self.secs as u128 * NANOS_PER_SEC as u128 + self.nanos as u128
    }
    pub const fn as_micros(&self) -> u128 {
        self.as_nanos() / 1_000
    }
    pub const fn as_millis(&self) -> u128 {
        self.as_nanos() / 1_000_000
    }
    pub fn as_secs_f64(&self) -> f64 {
        self.secs as f64 + self.nanos as f64 / NANOS_PER_SEC as f64
    }
    pub const fn is_zero(&self) -> bool {
        self.secs == 0 && self.nanos == 0
    }

    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        let nanos = self.nanos + rhs.nanos;
        let carry = (nanos / NANOS_PER_SEC) as u64;
        let secs = match self.secs.checked_add(rhs.secs) {
            Some(secs) => secs,
            None => return None,
        };
        match secs.checked_add(carry) {
            Some(secs) => Some(Self {
                secs,
                nanos: nanos % NANOS_PER_SEC,
            }),
            None => None,
        }
    }

    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        let secs = match self.secs.checked_sub(rhs.secs) {
            Some(secs) => secs,
            None => return None,
        };
        if self.nanos >= rhs.nanos {
            Some(Self {
                secs,
                nanos: self.nanos - rhs.nanos,
            })
        } else {
            match secs.checked_sub(1) {
                Some(secs) => Some(Self {
                    secs,
                    nanos: self.nanos + NANOS_PER_SEC - rhs.nanos,
                }),
                None => None,
            }
        }
    }
}

impl Default for Duration {
    fn default() -> Self {
        Self::ZERO
    }
}
impl Add for Duration {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs)
            .expect("overflow when adding durations")
    }
}
impl Sub for Duration {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs)
            .expect("overflow when subtracting durations")
    }
}

#[repr(C)]
struct Timespec {
    secs: i64,
    nanos: i64,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
}
#[cfg(target_os = "macos")]
// Rust uses CLOCK_UPTIME_RAW on Apple: monotonic, excluding system sleep.
const MONOTONIC: i32 = 8;
#[cfg(target_os = "linux")]
const MONOTONIC: i32 = 1;

/// A timestamp from the platform's monotonic clock.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    secs: i64,
    nanos: u32,
}

impl Instant {
    /// Reads the monotonic clock.
    ///
    /// # Panics
    /// Panics if the platform cannot provide a valid monotonic timestamp.
    pub fn now() -> Self {
        let mut time = Timespec { secs: 0, nanos: 0 };
        // SAFETY: Timespec has the native signed 64-bit time_t/long layout on
        // supported macOS/Linux targets; the pointer names writable storage.
        let result = unsafe { clock_gettime(MONOTONIC, &mut time) };
        assert!(result == 0, "could not read monotonic clock");
        assert!(
            time.nanos >= 0 && time.nanos < NANOS_PER_SEC as i64,
            "invalid monotonic clock timestamp"
        );
        Self {
            secs: time.secs,
            nanos: time.nanos as u32,
        }
    }

    pub fn elapsed(&self) -> Duration {
        Self::now().duration_since(*self)
    }
    pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
        let mut secs = self.secs as i128 - earlier.secs as i128;
        let nanos = if self.nanos >= earlier.nanos {
            self.nanos - earlier.nanos
        } else {
            secs -= 1;
            self.nanos + NANOS_PER_SEC - earlier.nanos
        };
        if secs < 0 {
            None
        } else {
            Some(Duration::new(secs as u64, nanos))
        }
    }
    pub fn duration_since(&self, earlier: Self) -> Duration {
        self.checked_duration_since(earlier)
            .unwrap_or(Duration::ZERO)
    }
    pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
        self.duration_since(earlier)
    }
    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        let nanos = self.nanos + duration.nanos;
        let secs = self.secs as i128 + duration.secs as i128 + (nanos / NANOS_PER_SEC) as i128;
        if secs > i64::MAX as i128 {
            None
        } else {
            Some(Self {
                secs: secs as i64,
                nanos: nanos % NANOS_PER_SEC,
            })
        }
    }
    pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
        let borrow = self.nanos < duration.nanos;
        let secs = self.secs as i128 - duration.secs as i128 - borrow as i128;
        if secs < i64::MIN as i128 {
            return None;
        }
        let nanos = if borrow {
            self.nanos + NANOS_PER_SEC - duration.nanos
        } else {
            self.nanos - duration.nanos
        };
        Some(Self {
            secs: secs as i64,
            nanos,
        })
    }
}
impl Add<Duration> for Instant {
    type Output = Self;
    fn add(self, rhs: Duration) -> Self {
        self.checked_add(rhs)
            .expect("overflow when adding duration to instant")
    }
}
impl Sub<Duration> for Instant {
    type Output = Self;
    fn sub(self, rhs: Duration) -> Self {
        self.checked_sub(rhs)
            .expect("overflow when subtracting duration from instant")
    }
}
impl Sub for Instant {
    type Output = Duration;
    fn sub(self, rhs: Self) -> Duration {
        self.duration_since(rhs)
    }
}
