//! Deterministic RNG and per-event stream derivation (spec v2, Section 3.B).
//!
//! The engine never shares a mutable RNG between decisions. Each action draws
//! from a fresh stream keyed by `(match_seed, player_id, action_count)`, so the
//! outcome of an action is independent of evaluation order, of LOD and of how
//! many other actions happened in between.

use rand_xoshiro::rand_core::{Rng as _, SeedableRng as _};
use rand_xoshiro::Xoshiro256PlusPlus;

/// `SplitMix64` finaliser (Steele, Lea, Flood 2014). Bijective on u64, so
/// distinct inputs never collide before the final xoshiro expansion.
#[inline]
#[must_use]
pub const fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Seeded xoshiro256++ stream. Plain owned value (`Send + Sync`, no interior
/// mutability), so it is thread-safe by construction: each thread or event
/// owns its own instance instead of locking a shared one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(Xoshiro256PlusPlus);

impl Rng {
    #[must_use]
    pub fn seed_from_u64(seed: u64) -> Self {
        Self(Xoshiro256PlusPlus::seed_from_u64(seed))
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        // High bits of xoshiro++ have the best statistical quality.
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `[0, 1)` with 24 bits of precision (exactly representable
    /// in f32, so no rounding can produce 1.0).
    #[inline]
    #[allow(clippy::cast_precision_loss)] // value < 2^24: exact in f32
    pub fn next_f32(&mut self) -> f32 {
        const SCALE: f32 = 1.0 / (1u32 << 24) as f32;
        (self.next_u64() >> 40) as f32 * SCALE
    }

    /// Uniform in `[lo, hi)`.
    #[inline]
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    /// Uniform integer in `[0, n)` without modulo bias (Lemire's method).
    /// Returns 0 for `n == 0`.
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = u64::from(self.next_u32()) * u64::from(n);
            #[allow(clippy::cast_possible_truncation)] // low half on purpose
            let low = m as u32;
            if low >= threshold {
                return (m >> 32) as u32;
            }
        }
    }

    /// Bernoulli trial: true with probability `p` (clamped to `[0, 1]`).
    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
}

/// Independent stream for one action of one player in one match.
///
/// `player_id` and `action_count` are packed into disjoint halves of a u64
/// (asymmetric: `(3, 7)` and `(7, 3)` differ), mixed, then combined with the
/// match seed and mixed again, so the same action in another match draws
/// different numbers.
#[must_use]
pub fn rng_for_event(match_seed: u64, player_id: u32, action_count: u32) -> Rng {
    let packed = (u64::from(player_id) << 32) | u64::from(action_count);
    let seed = splitmix64(match_seed ^ splitmix64(packed));
    Rng::seed_from_u64(seed)
}

#[cfg(test)]
mod tests {
    use super::{rng_for_event, splitmix64, Rng};
    use std::collections::HashSet;

    fn take(mut r: Rng, n: usize) -> Vec<u64> {
        (0..n).map(|_| r.next_u64()).collect()
    }

    #[test]
    fn rng_is_send_and_sync() {
        fn assert_thread_safe<T: Send + Sync>() {}
        assert_thread_safe::<Rng>();
    }

    #[test]
    fn splitmix64_reference_values() {
        // Reference outputs of the canonical SplitMix64 sequence seeded with 0:
        // the first call hashes 0 + golden gamma.
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
        assert_ne!(splitmix64(1), splitmix64(2));
    }

    /// Pinned outputs; `fm-wasm/tests/web.rs` asserts the same values in WASM.
    const GOLDEN_SEED: u64 = 0x1234_5678_9ABC_DEF0;
    const GOLDEN_3_7: [u64; 4] = [
        0xE278_19E3_B95B_C0C3,
        0xAE2B_B331_92F8_3AF7,
        0xC8AC_F881_11D2_6C24,
        0x8AA5_2FF8_3857_FDA4,
    ];

    #[test]
    fn rng_for_event_matches_golden() {
        assert_eq!(take(rng_for_event(GOLDEN_SEED, 3, 7), 4), GOLDEN_3_7);
    }

    #[test]
    fn same_inputs_same_sequence() {
        let a = take(rng_for_event(42, 10, 5), 64);
        let b = take(rng_for_event(42, 10, 5), 64);
        assert_eq!(a, b);
    }

    #[test]
    fn asymmetric_in_player_and_action() {
        let a = take(rng_for_event(42, 3, 7), 8);
        let b = take(rng_for_event(42, 7, 3), 8);
        assert_ne!(a, b);
    }

    #[test]
    fn match_seed_changes_sequence() {
        let a = take(rng_for_event(1, 3, 7), 8);
        let b = take(rng_for_event(2, 3, 7), 8);
        assert_ne!(a, b);
    }

    #[test]
    fn thousand_pairs_no_collision_in_first_output() {
        // Pairs come from a seeded generator (deterministic test), deduplicated
        // so a repeated pair is not mistaken for a collision.
        let mut src = Rng::seed_from_u64(0xF00D);
        let mut pairs = HashSet::new();
        while pairs.len() < 1000 {
            pairs.insert((src.below(5000), src.below(200)));
        }
        // Include the mirrored pairs explicitly: the old XOR design collided here.
        for (p, a) in [(3, 7), (7, 3), (0, 1), (1, 0)] {
            pairs.insert((p, a));
        }
        let firsts: HashSet<u64> = pairs
            .iter()
            .map(|&(p, a)| rng_for_event(0xABCD_EF01, p, a).next_u64())
            .collect();
        assert_eq!(firsts.len(), pairs.len());
    }

    #[test]
    fn floats_and_ranges_stay_in_bounds() {
        let mut r = Rng::seed_from_u64(7);
        for _ in 0..10_000 {
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
            let g = r.range_f32(-2.0, 3.0);
            assert!((-2.0..3.0).contains(&g));
            assert!(r.below(17) < 17);
        }
        assert_eq!(r.below(0), 0);
        assert!(!r.chance(0.0));
        assert!(r.chance(1.0));
    }

    #[test]
    fn below_is_roughly_uniform() {
        let mut r = Rng::seed_from_u64(99);
        let mut buckets = [0_u32; 10];
        for _ in 0..100_000 {
            buckets[r.below(10) as usize] += 1;
        }
        for b in buckets {
            assert!((9_500..10_500).contains(&b), "bucket {b} out of range");
        }
    }
}
