//! Monotonic and system clocks on supported Unix targets.
use core::ops::{Add, Sub};
pub use core::time::Duration;

const NANOS_PER_SEC: u32 = 1_000_000_000;

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
        let nanos = self.nanos + duration.subsec_nanos();
        let secs = self.secs as i128 + duration.as_secs() as i128 + (nanos / NANOS_PER_SEC) as i128;
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
        let borrow = self.nanos < duration.subsec_nanos();
        let secs = self.secs as i128 - duration.as_secs() as i128 - borrow as i128;
        if secs < i64::MIN as i128 {
            return None;
        }
        let nanos = if borrow {
            self.nanos + NANOS_PER_SEC - duration.subsec_nanos()
        } else {
            self.nanos - duration.subsec_nanos()
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

/// A timestamp on the system's realtime clock, independent of the monotonic clock.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTime {
    secs: i64,
    nanos: u32,
}
/// The system-clock epoch: 1970-01-01 00:00:00 UTC.
pub const UNIX_EPOCH: SystemTime = SystemTime { secs: 0, nanos: 0 };
/// The positive duration by which an earlier timestamp preceded another.
#[derive(Clone, Copy, Debug)]
pub struct SystemTimeError(Duration);
impl SystemTimeError {
    pub fn duration(&self) -> Duration {
        self.0
    }
}
impl core::fmt::Display for SystemTimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("second time provided was later than self")
    }
}
impl core::error::Error for SystemTimeError {}
impl SystemTime {
    pub const UNIX_EPOCH: Self = UNIX_EPOCH;
    pub fn now() -> Self {
        let mut time = Timespec { secs: 0, nanos: 0 };
        // SAFETY: CLOCK_REALTIME is zero on admitted Unix platforms; the
        // pointer names a writable native timespec, as for Instant::now.
        let result = unsafe { clock_gettime(0, &mut time) };
        assert!(result == 0, "could not read system clock");
        assert!(
            time.nanos >= 0 && time.nanos < NANOS_PER_SEC as i64,
            "invalid system clock timestamp"
        );
        Self {
            secs: time.secs,
            nanos: time.nanos as u32,
        }
    }
    pub fn duration_since(&self, earlier: Self) -> Result<Duration, SystemTimeError> {
        let lhs = self.secs as i128 * NANOS_PER_SEC as i128 + self.nanos as i128;
        let rhs = earlier.secs as i128 * NANOS_PER_SEC as i128 + earlier.nanos as i128;
        let delta = lhs - rhs;
        let magnitude = if delta < 0 { -delta } else { delta };
        let duration = Duration::new(
            (magnitude / NANOS_PER_SEC as i128) as u64,
            (magnitude % NANOS_PER_SEC as i128) as u32,
        );
        if delta < 0 {
            Err(SystemTimeError(duration))
        } else {
            Ok(duration)
        }
    }
    pub fn elapsed(&self) -> Result<Duration, SystemTimeError> {
        Self::now().duration_since(*self)
    }
    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        let nanos = self.nanos + duration.subsec_nanos();
        let secs = self.secs as i128 + duration.as_secs() as i128 + (nanos / NANOS_PER_SEC) as i128;
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
        let borrow = self.nanos < duration.subsec_nanos();
        let secs = self.secs as i128 - duration.as_secs() as i128 - borrow as i128;
        if secs < i64::MIN as i128 {
            return None;
        }
        let nanos = if borrow {
            self.nanos + NANOS_PER_SEC - duration.subsec_nanos()
        } else {
            self.nanos - duration.subsec_nanos()
        };
        Some(Self {
            secs: secs as i64,
            nanos,
        })
    }
}
impl Add<Duration> for SystemTime {
    type Output = Self;
    fn add(self, rhs: Duration) -> Self {
        self.checked_add(rhs)
            .expect("overflow when adding duration to system time")
    }
}
impl Sub<Duration> for SystemTime {
    type Output = Self;
    fn sub(self, rhs: Duration) -> Self {
        self.checked_sub(rhs)
            .expect("overflow when subtracting duration from system time")
    }
}
impl core::ops::AddAssign<Duration> for SystemTime {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}
impl core::ops::SubAssign<Duration> for SystemTime {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}
impl core::fmt::Debug for SystemTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SystemTime")
            .field("tv_sec", &self.secs)
            .field("tv_nsec", &self.nanos)
            .finish()
    }
}
impl core::fmt::Debug for Instant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Instant").field("tv_sec", &self.secs).field("tv_nsec", &self.nanos).finish()
    }
}
