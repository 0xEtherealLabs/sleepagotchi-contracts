//! Deterministic splitmix64, for tests only.
//!
//! Seeded rather than drawn from the environment so a failing case is
//! reproducible from the seed printed with it.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_u128(&mut self) -> u128 {
        ((self.next_u64() as u128) << 64) | self.next_u64() as u128
    }

    /// Uniform-ish over `0..bound`. Modulo bias is irrelevant here — these
    /// choose which branch a simulation takes, not a key.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}
