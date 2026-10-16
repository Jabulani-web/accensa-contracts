//! Vesting schedule model and math (issue #467).
//!
//! A schedule is a fixed token allocation that unlocks **linearly** from
//! `start` to `start + duration`, with nothing available until the **cliff**
//! at `start + cliff` has passed. The canonical team/investor allocation is a
//! one-year cliff followed by four years of linear release
//! ([`ONE_YEAR_SECS`] / [`FOUR_YEARS_SECS`]), so a quarter of the allocation
//! is unlocked the moment the cliff expires and the rest drips in over the
//! remaining three years.
//!
//! ```text
//! total              ┤                              ╭───────────
//!                    │                          ╭───╯
//!                    │                     ╭────╯
//!                    │             ╭───────╯
//!   0 ───────────────┴─────────────┴──────────────────────────────
//!                    start      start+cliff              start+duration
//! ```
//!
//! All arithmetic is checked: a schedule whose product of `total` and elapsed
//! time would overflow `i128` is rejected with [`Error::MathOverflow`] rather
//! than silently wrapping (the workspace compiles with `overflow-checks = true`
//! in dev, but release builds do not panic on overflow — so the check is
//! explicit here).

use crate::Error;
use soroban_sdk::{contracttype, Address};

/// Seconds in a 365-day year. The vesting contract measures time in Unix
/// seconds (`env.ledger().timestamp()`), not ledgers, so schedules keep their
/// meaning across a ledger-rate change.
pub const ONE_YEAR_SECS: u64 = 365 * 24 * 60 * 60;

/// The canonical four-year linear release window.
pub const FOUR_YEARS_SECS: u64 = 4 * ONE_YEAR_SECS;

/// A single beneficiary's token allocation and its claim bookkeeping.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingSchedule {
    /// The only address that may claim from this schedule.
    pub beneficiary: Address,
    /// Total allocation, in the token's smallest unit.
    pub total: i128,
    /// Amount already paid out by [`crate::Treasury::claim_vested`].
    pub claimed: i128,
    /// Unix timestamp the linear release is measured from.
    pub start: u64,
    /// Seconds after `start` before *anything* unlocks. May be `0` for a
    /// schedule with no cliff.
    pub cliff: u64,
    /// Length of the linear release window, in seconds. Must be `> 0` and
    /// `>= cliff`.
    pub duration: u64,
}

impl VestingSchedule {
    /// Whether the schedule's parameters form a valid allocation: a positive
    /// total, a non-zero release window, and a cliff that fits inside it.
    pub fn is_valid(&self) -> bool {
        self.total > 0 && self.duration > 0 && self.cliff <= self.duration
    }

    /// The cumulative amount unlocked at `now`, ignoring what has already been
    /// claimed.
    ///
    /// - `0` before the cliff (`now < start + cliff`);
    /// - `total` at or after the end (`now >= start + duration`);
    /// - otherwise `floor(total * (now - start) / duration)`.
    ///
    /// # Errors
    ///
    /// [`Error::MathOverflow`] if `start + cliff` or `start + duration` does
    /// not fit in a `u64`, or if scaling `total` by the elapsed time overflows
    /// `i128`.
    pub fn vested(&self, now: u64) -> Result<i128, Error> {
        let cliff_end = self
            .start
            .checked_add(self.cliff)
            .ok_or(Error::MathOverflow)?;
        if now < cliff_end {
            return Ok(0);
        }

        let end = self
            .start
            .checked_add(self.duration)
            .ok_or(Error::MathOverflow)?;
        if now >= end {
            return Ok(self.total);
        }

        let elapsed = now - self.start;
        self.total
            .checked_mul(elapsed as i128)
            .ok_or(Error::MathOverflow)
            .map(|scaled| scaled / self.duration as i128)
    }

    /// The amount unlocked now but not yet claimed.
    ///
    /// Saturates at zero: a stored `claimed` above the currently vested amount
    /// (only reachable if a schedule were mutated out of band) reports nothing
    /// claimable rather than a negative number.
    ///
    /// # Errors
    ///
    /// Propagates [`Error::MathOverflow`] from [`Self::vested`].
    pub fn claimable(&self, now: u64) -> Result<i128, Error> {
        // `.max(0)` is what makes the saturation documented above true:
        // `saturating_sub` alone only saturates at `i128::MIN`, so a record
        // whose `claimed` was mutated past `vested` would otherwise hand back
        // a negative amount (issue #647).
        Ok(self.vested(now)?.saturating_sub(self.claimed).max(0))
    }
}

#[cfg(test)]
mod tests {
    //! Direct unit tests for the vesting model (issue #647).
    //!
    //! The integration harness in `crate::test` only reaches this module
    //! through `add_schedule` / `claim_vested` with small fixed timestamps, so
    //! three documented behaviours had no coverage anywhere: the saturation in
    //! [`VestingSchedule::claimable`] (which needs a schedule mutated out of
    //! band — the harness can only ever advance `claimed` to exactly `vested`)
    //! and the two [`Error::MathOverflow`] returns for `start + cliff` /
    //! `start + duration` (which the harness never feeds). This module calls
    //! the model directly, following the in-contract probe pattern of
    //! `contracts/common/src/storage.rs`.

    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    /// Arbitrary but fixed Unix timestamp schedules start from.
    const START: u64 = 1_700_000_000;

    fn schedule(env: &Env, total: i128, start: u64, cliff: u64, duration: u64) -> VestingSchedule {
        VestingSchedule {
            beneficiary: Address::generate(env),
            total,
            claimed: 0,
            start,
            cliff,
            duration,
        }
    }

    #[test]
    fn vested_is_zero_below_the_cliff() {
        let env = Env::default();
        let s = schedule(&env, 4_000, START, ONE_YEAR_SECS, FOUR_YEARS_SECS);
        let cliff_end = START + ONE_YEAR_SECS;

        // Well before the window, at the start of it, and one second short of
        // the cliff: nothing has unlocked.
        assert_eq!(s.vested(START - 1), Ok(0));
        assert_eq!(s.vested(START), Ok(0));
        assert_eq!(s.vested(cliff_end - 1), Ok(0));
    }

    #[test]
    fn vested_exactly_at_the_cliff_is_a_quarter_of_the_total() {
        let env = Env::default();
        let s = schedule(&env, 4_000, START, ONE_YEAR_SECS, FOUR_YEARS_SECS);

        // One year of a four-year release: floor(4_000 * 1y / 4y) = 1_000.
        assert_eq!(s.vested(START + ONE_YEAR_SECS), Ok(1_000));
        // One second earlier the cliff has not passed yet.
        assert_eq!(s.vested(START + ONE_YEAR_SECS - 1), Ok(0));
    }

    #[test]
    fn vested_mid_window_floors_the_linear_share() {
        let env = Env::default();
        let s = schedule(&env, 10, START, 0, 7);

        // floor(10 * 3 / 7) = floor(4.28…) = 4 — rounded down, never up.
        assert_eq!(s.vested(START + 3), Ok(4));
        // floor(10 * 4 / 7) = floor(5.71…) = 5.
        assert_eq!(s.vested(START + 4), Ok(5));
        // Second zero: the linear step starts only once the window has.
        assert_eq!(s.vested(START - 1), Ok(0));
    }

    #[test]
    fn vested_at_and_past_the_end_is_the_total() {
        let env = Env::default();
        let s = schedule(&env, 4_000, START, ONE_YEAR_SECS, FOUR_YEARS_SECS);
        let end = START + FOUR_YEARS_SECS;

        assert_eq!(s.vested(end - 1), Ok(3_999));
        assert_eq!(s.vested(end), Ok(4_000));
        assert_eq!(s.vested(end + 86_400), Ok(4_000));
    }

    #[test]
    fn claimable_after_partial_claims_is_vested_minus_claimed() {
        let env = Env::default();
        let mut s = schedule(&env, 4_000, START, ONE_YEAR_SECS, FOUR_YEARS_SECS);
        let at = START + ONE_YEAR_SECS; // 1_000 vested

        s.claimed = 250;
        assert_eq!(s.claimable(at), Ok(750));

        // Claiming the whole vested amount leaves nothing for the same instant.
        s.claimed = 1_000;
        assert_eq!(s.claimable(at), Ok(0));

        // And the rest of the curve keeps accruing after that claim.
        s.claimed = 1_000;
        assert_eq!(s.claimable(START + 2 * ONE_YEAR_SECS), Ok(1_000));
    }

    #[test]
    fn claimable_saturates_at_zero_when_claimed_exceeds_vested() {
        let env = Env::default();
        let mut s = schedule(&env, 4_000, START, ONE_YEAR_SECS, FOUR_YEARS_SECS);

        // Only reachable by mutating a stored schedule out of band — the
        // harness cannot build it — and the doc comment promises zero rather
        // than a negative amount.
        s.claimed = 10_000;
        assert_eq!(s.claimable(START + ONE_YEAR_SECS), Ok(0));
        assert_eq!(s.claimable(START), Ok(0));
        assert_eq!(s.claimable(START + FOUR_YEARS_SECS), Ok(0));
    }

    #[test]
    fn vested_reports_math_overflow_for_start_plus_cliff() {
        let env = Env::default();
        let s = schedule(&env, 4_000, u64::MAX - 5, 10, FOUR_YEARS_SECS);

        assert_eq!(s.vested(u64::MAX), Err(Error::MathOverflow));
    }

    #[test]
    fn vested_reports_math_overflow_for_start_plus_duration() {
        let env = Env::default();
        let s = schedule(&env, 4_000, u64::MAX - 5, 0, 10);

        // `start + cliff` fits, so the call gets as far as `start + duration`.
        assert_eq!(s.vested(u64::MAX), Err(Error::MathOverflow));
        // Before the window opens the cliff arm short-circuits first — and
        // with no cliff that is `start` itself, which still fits.
        assert_eq!(s.vested(u64::MAX - 6), Ok(0));
    }

    #[test]
    fn is_valid_pins_each_of_its_three_rules() {
        let env = Env::default();

        // Baseline: every rule holds.
        assert!(schedule(&env, 4_000, START, ONE_YEAR_SECS, FOUR_YEARS_SECS).is_valid());

        // Rule 1: a positive total.
        assert!(!schedule(&env, 0, START, ONE_YEAR_SECS, FOUR_YEARS_SECS).is_valid());
        assert!(!schedule(&env, -1, START, ONE_YEAR_SECS, FOUR_YEARS_SECS).is_valid());

        // Rule 2: a non-zero release window (with a cliff that would fit).
        assert!(!schedule(&env, 4_000, START, 0, 0).is_valid());

        // Rule 3: a cliff that fits inside the window (equal is allowed).
        assert!(!schedule(&env, 4_000, START, FOUR_YEARS_SECS + 1, FOUR_YEARS_SECS).is_valid());
        assert!(schedule(&env, 4_000, START, FOUR_YEARS_SECS, FOUR_YEARS_SECS).is_valid());
    }
}
