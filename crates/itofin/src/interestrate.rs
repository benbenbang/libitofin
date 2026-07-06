//! Interest-rate compounding algebra.
//!
//! Port of `ql/compounding.hpp` and `ql/interestrate.{hpp,cpp}`: the
//! [`Compounding`] conventions and the [`InterestRate`] class, which bundles a
//! rate with its day-counting and compounding conventions and converts between
//! them (discount/compound factors, implied and equivalent rates).
//!
//! ## Divergences from QuantLib
//!
//! - QuantLib's default constructor builds a *null* interest rate
//!   (`Null<Rate>`), rejected at use time by `compoundFactor`. The null state
//!   is not ported: an [`InterestRate`] always holds an actual rate, and call
//!   sites that need "not yet set" use `Option<InterestRate>` (same approach
//!   as the always-valid [`DayCounter`]).
//! - Constructor and conversion preconditions (`QL_REQUIRE`) surface as
//!   [`QlResult`] per D4 instead of exceptions.

use std::fmt;

use crate::errors::QlResult;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Rate, Real, Time};
use crate::{fail, require};

/// Interest-rate compounding rule.
///
/// The variants carry QuantLib's discriminants (`Simple = 0`, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Compounding {
    /// `1 + r t`.
    Simple = 0,
    /// `(1 + r / f)^(f t)`.
    Compounded = 1,
    /// `e^(r t)`.
    Continuous = 2,
    /// Simple up to the first period, then compounded.
    SimpleThenCompounded = 3,
    /// Compounded up to the first period, then simple.
    CompoundedThenSimple = 4,
}

impl fmt::Display for Compounding {
    /// Renders the QuantLib label, e.g. `Simple` or `SimpleThenCompounded`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Compounding::Simple => "Simple",
            Compounding::Compounded => "Compounded",
            Compounding::Continuous => "Continuous",
            Compounding::SimpleThenCompounded => "SimpleThenCompounded",
            Compounding::CompoundedThenSimple => "CompoundedThenSimple",
        };
        f.write_str(name)
    }
}

/// Concrete interest rate: a rate plus its day-counting convention,
/// compounding convention and compounding frequency.
///
/// Encapsulates the interest-rate compounding algebra: conversions between
/// conventions, discount/compound factor calculations, and implied/equivalent
/// rate calculations.
#[derive(Clone, Debug)]
pub struct InterestRate {
    rate: Rate,
    day_counter: DayCounter,
    compounding: Compounding,
    frequency: Frequency,
    freq_makes_sense: bool,
}

impl InterestRate {
    /// Builds an interest rate from its conventions.
    ///
    /// The frequency only matters for [`Compounded`](Compounding::Compounded),
    /// [`SimpleThenCompounded`](Compounding::SimpleThenCompounded) and
    /// [`CompoundedThenSimple`](Compounding::CompoundedThenSimple); for those,
    /// [`Once`](Frequency::Once) and [`NoFrequency`](Frequency::NoFrequency)
    /// are rejected (`QL_REQUIRE` in QuantLib).
    pub fn new(
        rate: Rate,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
    ) -> QlResult<InterestRate> {
        let freq_makes_sense = matches!(
            compounding,
            Compounding::Compounded
                | Compounding::SimpleThenCompounded
                | Compounding::CompoundedThenSimple
        );
        if freq_makes_sense {
            require!(
                frequency != Frequency::Once && frequency != Frequency::NoFrequency,
                "frequency not allowed for this interest rate"
            );
        }
        Ok(InterestRate {
            rate,
            day_counter,
            compounding,
            frequency,
            freq_makes_sense,
        })
    }

    /// The rate itself.
    pub fn rate(&self) -> Rate {
        self.rate
    }

    /// The day-counting convention the rate is quoted with.
    pub fn day_counter(&self) -> &DayCounter {
        &self.day_counter
    }

    /// The compounding convention.
    pub fn compounding(&self) -> Compounding {
        self.compounding
    }

    /// The compounding frequency, or
    /// [`NoFrequency`](Frequency::NoFrequency) when the compounding convention
    /// does not use one (simple or continuous rates).
    pub fn frequency(&self) -> Frequency {
        if self.freq_makes_sense {
            self.frequency
        } else {
            Frequency::NoFrequency
        }
    }

    fn freq_real(&self) -> Real {
        self.frequency as i16 as Real
    }

    /// Compound (a.k.a. capitalization) factor implied by the rate compounded
    /// at time `t`.
    ///
    /// Time must be measured using the rate's own day counter.
    pub fn compound_factor(&self, t: Time) -> QlResult<Real> {
        if t < 0.0 {
            fail!("negative time ({t}) not allowed");
        }
        let r = self.rate;
        let f = self.freq_real();
        let factor = match self.compounding {
            Compounding::Simple => 1.0 + r * t,
            Compounding::Compounded => (1.0 + r / f).powf(f * t),
            Compounding::Continuous => (r * t).exp(),
            Compounding::SimpleThenCompounded => {
                if t <= 1.0 / f {
                    1.0 + r * t
                } else {
                    (1.0 + r / f).powf(f * t)
                }
            }
            Compounding::CompoundedThenSimple => {
                if t > 1.0 / f {
                    1.0 + r * t
                } else {
                    (1.0 + r / f).powf(f * t)
                }
            }
        };
        Ok(factor)
    }

    /// Compound factor implied by the rate compounded between two dates.
    pub fn compound_factor_between(&self, d1: Date, d2: Date) -> QlResult<Real> {
        self.compound_factor_between_ref(d1, d2, Date::null(), Date::null())
    }

    /// Compound factor between two dates, given an explicit reference period
    /// for schedule-aware day counters.
    pub fn compound_factor_between_ref(
        &self,
        d1: Date,
        d2: Date,
        ref_start: Date,
        ref_end: Date,
    ) -> QlResult<Real> {
        require!(d2 >= d1, "d1 ({d1}) later than d2 ({d2})");
        let t = self
            .day_counter
            .year_fraction_ref(d1, d2, ref_start, ref_end);
        self.compound_factor(t)
    }

    /// Discount factor implied by the rate compounded at time `t`.
    ///
    /// Time must be measured using the rate's own day counter.
    pub fn discount_factor(&self, t: Time) -> QlResult<DiscountFactor> {
        Ok(1.0 / self.compound_factor(t)?)
    }

    /// Discount factor implied by the rate compounded between two dates.
    pub fn discount_factor_between(&self, d1: Date, d2: Date) -> QlResult<DiscountFactor> {
        self.discount_factor_between_ref(d1, d2, Date::null(), Date::null())
    }

    /// Discount factor between two dates, given an explicit reference period
    /// for schedule-aware day counters.
    pub fn discount_factor_between_ref(
        &self,
        d1: Date,
        d2: Date,
        ref_start: Date,
        ref_end: Date,
    ) -> QlResult<DiscountFactor> {
        Ok(1.0 / self.compound_factor_between_ref(d1, d2, ref_start, ref_end)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::daycounters::actual360::Actual360;

    fn quarterly_compounded(rate: Rate) -> InterestRate {
        InterestRate::new(
            rate,
            Actual360::new(),
            Compounding::Compounded,
            Frequency::Quarterly,
        )
        .expect("valid interest rate")
    }

    #[test]
    fn compounding_display_matches_quantlib_labels() {
        assert_eq!(Compounding::Simple.to_string(), "Simple");
        assert_eq!(Compounding::Compounded.to_string(), "Compounded");
        assert_eq!(Compounding::Continuous.to_string(), "Continuous");
        assert_eq!(
            Compounding::SimpleThenCompounded.to_string(),
            "SimpleThenCompounded"
        );
        assert_eq!(
            Compounding::CompoundedThenSimple.to_string(),
            "CompoundedThenSimple"
        );
    }

    #[test]
    fn constructor_rejects_degenerate_frequency_when_compounded() {
        for freq in [Frequency::Once, Frequency::NoFrequency] {
            for comp in [
                Compounding::Compounded,
                Compounding::SimpleThenCompounded,
                Compounding::CompoundedThenSimple,
            ] {
                let result = InterestRate::new(0.05, Actual360::new(), comp, freq);
                assert!(result.is_err());
            }
            assert!(InterestRate::new(0.05, Actual360::new(), Compounding::Simple, freq).is_ok());
        }
    }

    #[test]
    fn frequency_is_no_frequency_unless_compounded() {
        let simple = InterestRate::new(
            0.05,
            Actual360::new(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .expect("valid interest rate");
        assert_eq!(simple.frequency(), Frequency::NoFrequency);
        assert_eq!(quarterly_compounded(0.08).frequency(), Frequency::Quarterly);
    }

    #[test]
    fn compound_factor_formulas_match_conventions() {
        let t = 2.0;
        let simple = InterestRate::new(
            0.04,
            Actual360::new(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .expect("valid interest rate");
        assert_eq!(simple.compound_factor(t).expect("valid time"), 1.08);

        let compounded = quarterly_compounded(0.04);
        assert!(
            (compounded.compound_factor(t).expect("valid time") - 1.01_f64.powi(8)).abs() < 1e-15
        );

        let continuous = InterestRate::new(
            0.04,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
        .expect("valid interest rate");
        assert!(
            (continuous.compound_factor(t).expect("valid time") - 0.08_f64.exp()).abs() < 1e-15
        );
    }

    #[test]
    fn hybrid_conventions_switch_at_first_period() {
        let stc = InterestRate::new(
            0.06,
            Actual360::new(),
            Compounding::SimpleThenCompounded,
            Frequency::Semiannual,
        )
        .expect("valid interest rate");
        assert_eq!(stc.compound_factor(0.25).expect("valid time"), 1.015);
        assert!(
            (stc.compound_factor(0.75).expect("valid time") - 1.03_f64.powf(1.5)).abs() < 1e-15
        );

        let cts = InterestRate::new(
            0.06,
            Actual360::new(),
            Compounding::CompoundedThenSimple,
            Frequency::Semiannual,
        )
        .expect("valid interest rate");
        assert!(
            (cts.compound_factor(0.25).expect("valid time") - 1.03_f64.powf(0.5)).abs() < 1e-15
        );
        assert_eq!(cts.compound_factor(0.75).expect("valid time"), 1.045);
    }

    #[test]
    fn discount_factor_is_reciprocal_of_compound_factor() {
        let ir = quarterly_compounded(0.08);
        let compound = ir.compound_factor(1.5).expect("valid time");
        let discount = ir.discount_factor(1.5).expect("valid time");
        assert!((discount - 1.0 / compound).abs() < 1e-15);
    }

    #[test]
    fn negative_time_is_rejected() {
        let ir = quarterly_compounded(0.08);
        assert!(ir.compound_factor(-0.5).is_err());
        assert!(ir.discount_factor(-0.5).is_err());
    }
}
