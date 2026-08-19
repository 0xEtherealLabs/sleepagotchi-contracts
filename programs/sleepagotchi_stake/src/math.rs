//! Wide multiply-divide.
//!
//! `reward_pool × weighted_stake_seconds` overflows `u128` at ordinary season
//! magnitudes — a 2M $SLEEP pool against a 90-day season is already a 138-bit
//! product — so the pro-rata share cannot be computed by multiplying first and
//! dividing after. Everything else in the program stays inside `u128`; this is
//! the one path that does not.
//!
//! Hand-rolled rather than taken from a big-integer crate: a dependency here
//! would be a dependency inside the audit boundary.

/// Mask for the low 64 bits of a `u128`.
const LOW_64: u128 = u64::MAX as u128;

/// The full 256-bit product of two `u128`s, as `(high, low)`.
///
/// Schoolbook, on 64-bit limbs: each partial product fits `u128` because its
/// operands are both under 2^64.
pub fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    let (a_lo, a_hi) = (a & LOW_64, a >> 64);
    let (b_lo, b_hi) = (b & LOW_64, b >> 64);

    // The two middle terms sum to at most 2^129, so their carry lands 64 bits
    // up in `high` rather than at bit 0.
    let (mid, mid_carry) = (a_lo * b_hi).overflowing_add(a_hi * b_lo);
    let (low, low_carry) = (a_lo * b_lo).overflowing_add(mid << 64);

    let mut high = a_hi * b_hi + (mid >> 64);
    if mid_carry {
        high += 1 << 64;
    }
    if low_carry {
        high += 1;
    }

    (high, low)
}

/// `floor(a × b / denom)`, exact, via a 256-bit intermediate.
///
/// `None` when `denom` is zero or the quotient will not fit in `u128`. Callers
/// map that to `MathOverflow` — it is unreachable for any real season, and
/// silently saturating a payout is not a thing this program should do.
///
/// Flooring is deliberate and load-bearing: every reward rounds down, so the
/// sum of all of them cannot exceed the pool that funded them.
pub fn mul_div_floor(a: u128, b: u128, denom: u128) -> Option<u128> {
    if denom == 0 {
        return None;
    }

    let (high, low) = mul_wide(a, b);
    if high == 0 {
        return Some(low / denom);
    }

    // (high·2^128 + low) / denom < 2^128  ⟺  high < denom.
    if high >= denom {
        return None;
    }

    // Binary long division, most significant bit first: 128 iterations of shift
    // and conditional subtract. Chosen over Knuth's algorithm D because this runs
    // once per claim rather than in any hot path.
    //
    // Invariant: `remainder` is the true remainder so far, always below `denom`.
    let mut remainder = high;
    let mut quotient: u128 = 0;

    for shift in (0..128).rev() {
        // `remainder·2 + bit` can reach 2^129, so the bit shifted off the top is
        // tracked rather than lost. When it is set the value certainly exceeds
        // `denom`, and the subtraction below brings it back under 2^128.
        let carried = remainder >> 127 == 1;
        remainder = (remainder << 1) | ((low >> shift) & 1);

        if carried || remainder >= denom {
            remainder = remainder.wrapping_sub(denom);
            quotient |= 1 << shift;
        }
    }

    Some(quotient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng::Rng;

    const MAX: u128 = u128::MAX;

    /// The definition, checked in 256-bit arithmetic: `q·denom ≤ a·b` and
    /// `(q+1)·denom > a·b`. True for exactly one `q`, so this pins the result
    /// without needing a second implementation to agree with.
    fn assert_is_floor(a: u128, b: u128, denom: u128, quotient: u128) {
        let product = mul_wide(a, b);

        let lower = mul_wide(quotient, denom);
        assert!(lower <= product, "q·denom > a·b for {a}·{b}/{denom}");

        let (upper_low, carry) = lower.1.overflowing_add(denom);
        let (upper_high, overflowed) = lower.0.overflowing_add(carry as u128);
        assert!(
            overflowed || (upper_high, upper_low) > product,
            "(q+1)·denom ≤ a·b for {a}·{b}/{denom}"
        );
    }

    #[test]
    fn multiplies_to_the_full_256_bit_product() {
        assert_eq!(mul_wide(0, 0), (0, 0));
        assert_eq!(mul_wide(1, 1), (0, 1));
        assert_eq!(mul_wide(MAX, 1), (0, MAX));
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        assert_eq!(mul_wide(MAX, MAX), (MAX - 1, 1));
        // Straddles the limb boundary in both directions.
        assert_eq!(mul_wide(1 << 64, 1 << 64), (1, 0));
        assert_eq!(mul_wide((1 << 64) - 1, (1 << 64) - 1), (0, MAX - (1 << 65) + 2));
    }

    /// Both middle partial products near maximum, the only input shape whose
    /// sum passes 2^128 and carries 64 bits up into `high`. Getting that carry
    /// wrong is off by 2^192, so anything that survives here has it right.
    #[test]
    fn carries_out_of_the_middle_term() {
        for (a, b) in [(MAX, MAX), (MAX, MAX - 12_345), (MAX - 1, MAX >> 1)] {
            let (a_lo, a_hi) = (a & LOW_64, a >> 64);
            let (b_lo, b_hi) = (b & LOW_64, b >> 64);
            assert!(
                (a_lo * b_hi).checked_add(a_hi * b_lo).is_none(),
                "{a}·{b} does not carry out of the middle"
            );

            assert_eq!(mul_div_floor(a, b, b), Some(a));
            assert_eq!(mul_div_floor(a, b, a), Some(b));
        }
    }

    #[test]
    fn agrees_with_native_arithmetic_across_the_u64_range() {
        // Where the product genuinely fits in u128 there is an independent
        // oracle, so use it.
        let mut rng = Rng::new(0x5EED);
        for _ in 0..20_000 {
            let a = rng.next_u64() as u128;
            let b = rng.next_u64() as u128;
            let denom = (rng.next_u64() as u128).max(1);
            assert_eq!(mul_div_floor(a, b, denom), Some(a * b / denom));
        }
    }

    #[test]
    fn is_the_floor_for_full_range_inputs() {
        let mut rng = Rng::new(0xC0FFEE);
        let mut wide = 0;

        for _ in 0..20_000 {
            let a = rng.next_u128();
            let b = rng.next_u128();
            let denom = rng.next_u128().max(1);

            match mul_div_floor(a, b, denom) {
                Some(quotient) => {
                    assert_is_floor(a, b, denom, quotient);
                    if mul_wide(a, b).0 != 0 {
                        wide += 1;
                    }
                }
                // Refused only because the quotient will not fit.
                None => assert!(mul_wide(a, b).0 >= denom),
            }
        }

        // The interesting path is the one that overflows u128; make sure the
        // random draw actually exercised it rather than passing on easy inputs.
        assert!(wide > 15_000, "only {wide} cases exceeded u128");
    }

    #[test]
    fn divides_evenly_when_it_should() {
        let mut rng = Rng::new(0xD1D1DE);
        for _ in 0..5_000 {
            let a = rng.next_u128();
            let b = rng.next_u128();
            assert_eq!(mul_div_floor(a, b, a), Some(b), "a·b/a");
            assert_eq!(mul_div_floor(a, b, b), Some(a), "a·b/b");
        }
        assert_eq!(mul_div_floor(MAX, MAX, MAX), Some(MAX));
    }

    #[test]
    fn rounds_down_rather_than_to_nearest() {
        // 7/2 = 3.5, and 1/2 = 0.5 — both go down, neither goes up.
        assert_eq!(mul_div_floor(7, 1, 2), Some(3));
        assert_eq!(mul_div_floor(1, 1, 2), Some(0));
        // Still rounding down at full width, not to nearest.
        assert_eq!(mul_div_floor(MAX, 1, 2), Some(MAX >> 1));
        assert_eq!(mul_div_floor(MAX, 2, 3), Some(226_854_911_280_625_642_308_916_404_954_512_140_970));
        assert_eq!(mul_div_floor(MAX - 1, MAX, MAX), Some(MAX - 1));
    }

    #[test]
    fn refuses_a_zero_denominator() {
        assert_eq!(mul_div_floor(1, 1, 0), None);
        assert_eq!(mul_div_floor(0, 0, 0), None);
        assert_eq!(mul_div_floor(MAX, MAX, 0), None);
    }

    #[test]
    fn refuses_a_quotient_that_will_not_fit() {
        // 2^127 · 4 / 2 = 2^128, one past the top.
        assert_eq!(mul_div_floor(1 << 127, 4, 2), None);
        assert_eq!(mul_div_floor(MAX, MAX, 1), None);
        assert_eq!(mul_div_floor(1 << 127, 2, 1), None);
        // Lands on 2^128 exactly, one past the largest representable quotient.
        assert_eq!(mul_div_floor(MAX, MAX, MAX - 1), None);

        // The boundary is exact: one less on either side still fits.
        assert_eq!(mul_div_floor(1 << 127, 4, 4), Some(1 << 127));
        assert_eq!(mul_div_floor(1 << 127, 4, 3), Some(226_854_911_280_625_642_308_916_404_954_512_140_970));
        assert_eq!(mul_div_floor(MAX, MAX, MAX), Some(MAX));
    }

    #[test]
    fn handles_zero() {
        assert_eq!(mul_div_floor(0, MAX, MAX), Some(0));
        assert_eq!(mul_div_floor(MAX, 0, MAX), Some(0));
    }

    /// A real season, at nine decimals: a 2M $SLEEP pool, one wallet holding 1M
    /// $SLEEP at 2x for ninety days, against six others of assorted size and
    /// weight. The product is 138 bits, which is the whole reason this module
    /// exists.
    #[test]
    fn computes_a_realistic_season_share() {
        let pool: u128 = 2_000_000_000_000_000;
        let position: u128 = 155_520_000_000_000_000_000_000_000;
        let season: u128 = 1_436_499_662_500_000_292_778_060_000;

        assert_ne!(mul_wide(pool, position).0, 0, "expected a >128-bit product");

        let reward = mul_div_floor(pool, position, season).unwrap();
        assert_eq!(reward, 216_526_330_022_719);
        assert_is_floor(pool, position, season, reward);
        assert!(reward < pool);
    }

    /// The property the solvency argument rests on: shares of one pool, summed,
    /// never exceed it. Flooring is what guarantees it.
    #[test]
    fn shares_of_a_pool_never_exceed_it() {
        let mut rng = Rng::new(0x5011D);

        for _ in 0..200 {
            let pool = (rng.next_u64() as u128).max(1);
            let positions: Vec<u128> = (0..8).map(|_| rng.next_u128() >> 8).collect();
            let season: u128 = positions.iter().sum();

            let paid: u128 = positions
                .iter()
                .map(|w| mul_div_floor(pool, *w, season).unwrap())
                .sum();

            assert!(paid <= pool, "paid {paid} out of {pool}");
        }
    }
}
