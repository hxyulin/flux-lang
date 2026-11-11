//! Reusable rendezvous with one leader for each completed generation.
use super::{Condvar, Mutex};

pub struct Barrier {
    state: Mutex<State>,
    changed: Condvar,
    threads: usize,
}

struct State {
    arrived: usize,
    generation: usize,
}

impl core::panic::RefUnwindSafe for Barrier {}

pub struct BarrierWaitResult(bool);

impl Barrier {
    pub const fn new(threads: usize) -> Self {
        Self {
            state: Mutex::new(State { arrived: 0, generation: 0 }),
            changed: Condvar::new(),
            threads,
        }
    }

    pub fn wait(&self) -> BarrierWaitResult {
        let mut state = self.state.lock().unwrap();
        let generation = state.generation;
        state.arrived += 1;
        if state.arrived < self.threads {
            while state.generation == generation {
                state = self.changed.wait(state).unwrap();
            }
            BarrierWaitResult(false)
        } else {
            state.arrived = 0;
            state.generation = state.generation.wrapping_add(1);
            self.changed.notify_all();
            BarrierWaitResult(true)
        }
    }
}

impl BarrierWaitResult {
    pub fn is_leader(&self) -> bool { self.0 }
}

impl core::fmt::Debug for Barrier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Barrier").finish_non_exhaustive()
    }
}

impl core::fmt::Debug for BarrierWaitResult {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BarrierWaitResult").field("is_leader", &self.is_leader()).finish()
    }
}
