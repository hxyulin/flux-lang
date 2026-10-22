//! The generator's only source of randomness: a *choice tape*.
//!
//! Every decision the generator makes is one draw from a [`Source`]. A
//! fresh source draws from a seeded PRNG and records what it drew; a
//! replayed one reads a recorded tape back, and once the tape runs out
//! every draw is 0. Generation is deterministic in the tape alone, so a
//! program is reproduced from its seed, and the reducer shrinks a
//! program by editing its tape (deleting runs of draws, making draws
//! smaller) as Hypothesis does. Because the generator is type-directed,
//! *any* tape yields a valid program, so reduction never has to check
//! validity, only that the failure is still there.
//!
//! Draws are arranged so that 0 is always the simplest choice: the first
//! alternative of a weighted choice, no more statements, a leaf
//! expression, the literal 0.

/// SplitMix64: small, fast and good enough to drive a generator.
#[derive(Clone)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// A tape of choices, recorded from a PRNG or replayed.
#[derive(Clone)]
pub struct Source {
    tape: Vec<u32>,
    pos: usize,
    fresh: Option<SplitMix64>,
}

impl Source {
    /// Draws from a PRNG seeded with `seed`, recording each draw.
    pub fn random(seed: u64) -> Source {
        Source {
            tape: Vec::new(),
            pos: 0,
            fresh: Some(SplitMix64(seed ^ 0x005e_edf1_u64.rotate_left(17))),
        }
    }

    /// Replays `tape`; draws past its end are 0.
    pub fn replay(tape: Vec<u32>) -> Source {
        Source {
            tape,
            pos: 0,
            fresh: None,
        }
    }

    /// The draws made so far.
    pub fn tape(&self) -> &[u32] {
        &self.tape[..self.pos.min(self.tape.len())]
    }

    /// A draw in `0..=max`. A fresh draw is recorded already reduced, so
    /// that the reducer's "make this draw smaller" means a simpler choice.
    fn draw(&mut self, max: u32) -> u32 {
        let value = if let Some(&v) = self.tape.get(self.pos) {
            if max == u32::MAX { v } else { v % (max + 1) }
        } else if let Some(rng) = &mut self.fresh {
            let v = rng.next() as u32;
            let v = if max == u32::MAX { v } else { v % (max + 1) };
            self.tape.push(v);
            v
        } else {
            0
        };
        self.pos += 1;
        value
    }

    /// A number in `0..n` (0 if `n` is 0).
    pub fn below(&mut self, n: u32) -> u32 {
        self.draw(n.saturating_sub(1))
    }

    /// A number in `lo..=hi`.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.below(hi - lo + 1)
    }

    /// True with probability `percent`/100; a 0 draw is false.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) >= 100 - percent.min(100)
    }

    /// An index into `weights`, chosen in proportion to them; a 0 draw is
    /// the first alternative with a nonzero weight.
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        let mut v = self.below(total);
        for (i, &w) in weights.iter().enumerate() {
            if v < w {
                return i;
            }
            v -= w;
        }
        0
    }

    /// 64 random bits, from two draws.
    pub fn bits64(&mut self) -> u64 {
        let hi = self.draw(u32::MAX) as u64;
        let lo = self.draw(u32::MAX) as u64;
        (hi << 32) | lo
    }
}
