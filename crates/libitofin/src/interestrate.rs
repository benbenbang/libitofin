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
//! - QuantLib lets `compoundFactor` return non-positive (or, for compounded
//!   conventions with `1 + r/f <= 0`, NaN) factors, which turn into infinite
//!   or negative discount factors downstream. Since this API is already
//!   fallible, such out-of-domain inputs are rejected instead: simple factors
//!   `1 + r t <= 0` and compounding bases `1 + r/f <= 0` return an error.

use std::fmt;

use crate::errors::QlResult;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Rate, Real, Time};
use crate::utilities::dataformatters;
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
        if Self::freq_makes_sense(compounding) {
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
        })
    }

    fn freq_makes_sense(compounding: Compounding) -> bool {
        matches!(
            compounding,
            Compounding::Compounded
                | Compounding::SimpleThenCompounded
                | Compounding::CompoundedThenSimple
        )
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
        if Self::freq_makes_sense(self.compounding) {
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
    ///
    /// # Errors
    ///
    /// Fails on negative or NaN time, and - diverging from QuantLib, which
    /// returns the resulting garbage values - when the rate leaves the
    /// compounding domain: a simple factor `1 + r t <= 0` or a compounding
    /// base `1 + r/f <= 0`. Ordinary negative rates are unaffected. The
    /// composite conventions fail as in QuantLib 1.44: their factor is defined
    /// only stepwise over a leg, by the [`CashFlows`](crate::cashflows::CashFlows)
    /// methods.
    pub fn compound_factor(&self, t: Time) -> QlResult<Real> {
        if t.is_nan() || t < 0.0 {
            fail!("negative time ({t}) not allowed");
        }
        let r = self.rate;
        let f = self.freq_real();
        let factor = match self.compounding {
            Compounding::Simple => Self::simple_factor(r, t)?,
            Compounding::Compounded => Self::compounded_factor(r, f, t)?,
            Compounding::Continuous => (r * t).exp(),
            Compounding::SimpleThenCompounded | Compounding::CompoundedThenSimple => {
                return Self::not_direct(self.compounding);
            }
        };
        Ok(factor)
    }

    fn simple_factor(r: Rate, t: Time) -> QlResult<Real> {
        let factor = 1.0 + r * t;
        if factor.is_nan() || factor <= 0.0 {
            fail!("non-positive compound factor ({factor}) for rate {r} at time {t}");
        }
        Ok(factor)
    }

    fn compounded_factor(r: Rate, f: Real, t: Time) -> QlResult<Real> {
        let base = 1.0 + r / f;
        if base.is_nan() || base <= 0.0 {
            fail!("non-positive compounding base ({base}) for rate {r} at frequency {f}");
        }
        Ok(base.powf(f * t))
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

    /// The derivative of [`discount_factor`](Self::discount_factor) at time
    /// `t` with respect to the rate.
    ///
    /// # Errors
    ///
    /// As [`discount_factor`](Self::discount_factor).
    pub fn discount_factor_first_derivative(&self, t: Time) -> QlResult<Real> {
        let discount = self.discount_factor(t)?;
        Ok(match self.compounding {
            Compounding::Simple => -t * discount * discount,
            Compounding::Continuous => -t * discount,
            _ => -t * discount / (1.0 + self.rate / self.freq_real()),
        })
    }

    /// The second derivative of [`discount_factor`](Self::discount_factor) at
    /// time `t` with respect to the rate.
    ///
    /// # Errors
    ///
    /// As [`discount_factor_first_derivative`](Self::discount_factor_first_derivative).
    pub fn discount_factor_second_derivative(&self, t: Time) -> QlResult<Real> {
        let discount = self.discount_factor(t)?;
        Ok(match self.compounding {
            Compounding::Simple => 2.0 * t * t * discount * discount * discount,
            Compounding::Continuous => t * t * discount,
            _ => {
                let freq = self.freq_real();
                let compound = 1.0 + self.rate / freq;
                discount * t * (freq * t + 1.0) / (freq * compound * compound)
            }
        })
    }

    fn not_direct<T>(compounding: Compounding) -> QlResult<T> {
        fail!("{compounding} is not supported for direct calculations: use the CashFlow methods");
    }

    /// Implied interest rate for a given compound factor at a given time.
    ///
    /// The resulting rate carries the day counter provided as input; time must
    /// be measured using that same day counter.
    ///
    /// # Errors
    ///
    /// For the composite conventions unless `compound` is `1.0`, whose rate
    /// is `0.0` under any convention (`interestrate.cpp:112-130`).
    pub fn implied_rate(
        compound: Real,
        result_dc: DayCounter,
        comp: Compounding,
        freq: Frequency,
        t: Time,
    ) -> QlResult<InterestRate> {
        if compound.is_nan() || compound <= 0.0 {
            fail!("positive compound factor required");
        }
        let rate = if compound == 1.0 {
            if t.is_nan() || t < 0.0 {
                fail!("non negative time ({t}) required");
            }
            0.0
        } else {
            if t.is_nan() || t <= 0.0 {
                fail!("positive time ({t}) required");
            }
            let f = freq as i16 as Real;
            match comp {
                Compounding::Simple => (compound - 1.0) / t,
                Compounding::Compounded => (compound.powf(1.0 / (f * t)) - 1.0) * f,
                Compounding::Continuous => compound.ln() / t,
                Compounding::SimpleThenCompounded | Compounding::CompoundedThenSimple => {
                    return Self::not_direct(comp);
                }
            }
        };
        InterestRate::new(rate, result_dc, comp, freq)
    }

    /// Implied rate for a given compound factor between two dates.
    pub fn implied_rate_between(
        compound: Real,
        result_dc: DayCounter,
        comp: Compounding,
        freq: Frequency,
        d1: Date,
        d2: Date,
    ) -> QlResult<InterestRate> {
        Self::implied_rate_between_ref(
            compound,
            result_dc,
            comp,
            freq,
            d1,
            d2,
            Date::null(),
            Date::null(),
        )
    }

    /// Implied rate between two dates, given an explicit reference period for
    /// schedule-aware day counters.
    #[allow(clippy::too_many_arguments)]
    pub fn implied_rate_between_ref(
        compound: Real,
        result_dc: DayCounter,
        comp: Compounding,
        freq: Frequency,
        d1: Date,
        d2: Date,
        ref_start: Date,
        ref_end: Date,
    ) -> QlResult<InterestRate> {
        require!(d2 >= d1, "d1 ({d1}) later than d2 ({d2})");
        let t = result_dc.year_fraction_ref(d1, d2, ref_start, ref_end);
        Self::implied_rate(compound, result_dc, comp, freq, t)
    }

    /// Equivalent interest rate for a compounding period `t`.
    ///
    /// The result shares the day counter of this instance; time must be
    /// measured using this instance's own day counter.
    pub fn equivalent_rate(
        &self,
        comp: Compounding,
        freq: Frequency,
        t: Time,
    ) -> QlResult<InterestRate> {
        Self::implied_rate(
            self.compound_factor(t)?,
            self.day_counter.clone(),
            comp,
            freq,
            t,
        )
    }

    /// Equivalent rate for a compounding period between two dates.
    ///
    /// The result is calculated taking the requested day-counting rule into
    /// account.
    pub fn equivalent_rate_between(
        &self,
        result_dc: DayCounter,
        comp: Compounding,
        freq: Frequency,
        d1: Date,
        d2: Date,
    ) -> QlResult<InterestRate> {
        self.equivalent_rate_between_ref(result_dc, comp, freq, d1, d2, Date::null(), Date::null())
    }

    /// Equivalent rate between two dates, given an explicit reference period
    /// for schedule-aware day counters.
    #[allow(clippy::too_many_arguments)]
    pub fn equivalent_rate_between_ref(
        &self,
        result_dc: DayCounter,
        comp: Compounding,
        freq: Frequency,
        d1: Date,
        d2: Date,
        ref_start: Date,
        ref_end: Date,
    ) -> QlResult<InterestRate> {
        require!(d2 >= d1, "d1 ({d1}) later than d2 ({d2})");
        let t1 = self
            .day_counter
            .year_fraction_ref(d1, d2, ref_start, ref_end);
        let t2 = result_dc.year_fraction_ref(d1, d2, ref_start, ref_end);
        Self::implied_rate(self.compound_factor(t1)?, result_dc, comp, freq, t2)
    }
}

impl fmt::Display for InterestRate {
    /// Renders the rate as QuantLib's `operator<<` does, e.g.
    /// `8.000000 % Actual/360 Quarterly compounding`, reusing
    /// [`dataformatters::rate`] (the `io::rate` port the C++ calls here).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} ",
            dataformatters::rate(self.rate),
            self.day_counter.name()
        )?;
        match self.compounding {
            Compounding::Simple => write!(f, "simple compounding"),
            Compounding::Compounded => write!(f, "{} compounding", self.frequency),
            Compounding::Continuous => write!(f, "continuous compounding"),
            Compounding::SimpleThenCompounded => write!(
                f,
                "simple compounding up to {} months, then {} compounding",
                12 / self.frequency as i16,
                self.frequency
            ),
            Compounding::CompoundedThenSimple => write!(
                f,
                "compounding up to {} months, then {} simple compounding",
                12 / self.frequency as i16,
                self.frequency
            ),
        }
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
    fn interest_rate_display_matches_quantlib_format() {
        assert_eq!(
            quarterly_compounded(0.08).to_string(),
            "8.000000 % Actual/360 Quarterly compounding"
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

    /// `interestrate.cpp:63-101`: each closed form against a central finite
    /// difference of [`InterestRate::discount_factor`] in the rate.
    #[test]
    fn discount_factor_derivatives_match_finite_differences() {
        let (t, r, h) = (1.75, 0.045, 1e-4);
        for (comp, freq) in [
            (Compounding::Simple, Frequency::Annual),
            (Compounding::Compounded, Frequency::Quarterly),
            (Compounding::Continuous, Frequency::Annual),
        ] {
            let at = |rate: Rate| InterestRate::new(rate, Actual360::new(), comp, freq).unwrap();
            let df = |rate: Rate| at(rate).discount_factor(t).unwrap();
            let first = (df(r + h) - df(r - h)) / (2.0 * h);
            let second = (df(r + h) - 2.0 * df(r) + df(r - h)) / (h * h);
            let rate = at(r);
            let d1 = rate.discount_factor_first_derivative(t).unwrap();
            let d2 = rate.discount_factor_second_derivative(t).unwrap();
            assert!((d1 - first).abs() < 1e-7, "{comp}: {d1} vs {first}");
            assert!((d2 - second).abs() < 1e-5, "{comp}: {d2} vs {second}");
        }
    }

    fn composite_error(comp: Compounding) -> String {
        format!("{comp} is not supported for direct calculations: use the CashFlow methods")
    }

    /// `interestrates.cpp:172-182`: the composite conventions have no direct
    /// discount factor, derivatives or equivalent rate.
    #[test]
    fn composite_conventions_are_not_supported_directly() {
        use crate::time::date::Month;
        let (d1, d2) = (
            Date::new(6, Month::July, 2026),
            Date::new(6, Month::July, 2027),
        );
        for comp in [
            Compounding::SimpleThenCompounded,
            Compounding::CompoundedThenSimple,
        ] {
            let rate = InterestRate::new(0.03, Actual360::new(), comp, Frequency::Semiannual)
                .expect("valid interest rate");
            for error in [
                rate.discount_factor(1.0).unwrap_err(),
                rate.discount_factor_first_derivative(1.0).unwrap_err(),
                rate.discount_factor_second_derivative(1.0).unwrap_err(),
                rate.equivalent_rate_between(
                    rate.day_counter().clone(),
                    Compounding::Simple,
                    Frequency::Annual,
                    d1,
                    d2,
                )
                .unwrap_err(),
            ] {
                assert_eq!(error.message(), composite_error(comp));
            }
        }
    }

    /// `interestrate.cpp:117-120`: a unit compound factor implies a zero rate
    /// before the composite conventions are rejected.
    #[test]
    fn unit_compound_factor_implies_zero_composite_rate() {
        for comp in [
            Compounding::SimpleThenCompounded,
            Compounding::CompoundedThenSimple,
        ] {
            let implied =
                InterestRate::implied_rate(1.0, Actual360::new(), comp, Frequency::Semiannual, 0.5)
                    .expect("unit compound factor");
            assert_eq!(implied.rate(), 0.0);
            assert_eq!(implied.compounding(), comp);
            let error = InterestRate::implied_rate(
                1.02,
                Actual360::new(),
                comp,
                Frequency::Semiannual,
                0.5,
            )
            .unwrap_err();
            assert_eq!(error.message(), composite_error(comp));
        }
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

    #[test]
    fn nan_inputs_are_rejected() {
        let ir = quarterly_compounded(0.08);
        assert!(ir.compound_factor(Real::NAN).is_err());
        assert!(ir.discount_factor(Real::NAN).is_err());

        let simple = (Compounding::Simple, Frequency::Annual);
        assert!(
            InterestRate::implied_rate(Real::NAN, Actual360::new(), simple.0, simple.1, 1.0)
                .is_err()
        );
        assert!(
            InterestRate::implied_rate(1.02, Actual360::new(), simple.0, simple.1, Real::NAN)
                .is_err()
        );
        assert!(
            InterestRate::implied_rate(1.0, Actual360::new(), simple.0, simple.1, Real::NAN)
                .is_err()
        );
    }

    #[test]
    fn non_positive_compound_factors_are_rejected() {
        let simple = InterestRate::new(
            -1.0,
            Actual360::new(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .expect("valid interest rate");
        assert!(simple.compound_factor(1.0).is_err());
        assert!(simple.discount_factor(1.0).is_err());
        assert!(simple.compound_factor(0.5).is_ok());

        let compounded = quarterly_compounded(-4.0);
        assert!(compounded.compound_factor(1.0).is_err());
        assert!(compounded.discount_factor(1.0).is_err());

        let even_power = quarterly_compounded(-8.0);
        assert!(even_power.compound_factor(1.0).is_err());
    }

    #[test]
    fn ordinary_negative_rates_still_work() {
        for comp in [
            Compounding::Simple,
            Compounding::Compounded,
            Compounding::Continuous,
        ] {
            let ir = InterestRate::new(-0.01, Actual360::new(), comp, Frequency::Quarterly)
                .expect("valid interest rate");
            let compound = ir.compound_factor(1.0).expect("in-domain negative rate");
            assert!(compound > 0.0 && compound < 1.0, "{comp}: {compound}");
            let discount = ir.discount_factor(1.0).expect("in-domain negative rate");
            assert!(discount > 1.0, "{comp}: {discount}");
        }
    }

    fn time_to_days(t: Time) -> crate::time::date::SerialNumber {
        (t * 360.0).round() as crate::time::date::SerialNumber
    }

    fn round_closest(value: Real, precision: i32) -> Real {
        let scale = 10f64.powi(precision);
        (value * scale).round() / scale
    }

    #[test]
    #[rustfmt::skip]
    fn conversions_match_quantlib() {
        use crate::time::date::Month;
        use Compounding::{Compounded, Continuous, Simple, SimpleThenCompounded};
        use Frequency::{Annual, Bimonthly, EveryFourthMonth, Monthly, Quarterly, Semiannual};

        type Row = (Rate, Compounding, Frequency, Time, Compounding, Frequency, Rate, i32);
        let cases: [Row; 16] = [
            (0.0800, Compounded, Quarterly, 1.00, Continuous, Annual, 0.0792, 4),
            (0.1200, Continuous, Annual, 1.00, Compounded, Annual, 0.1275, 4),
            (0.0800, Compounded, Quarterly, 1.00, Compounded, Annual, 0.0824, 4),
            (0.0700, Compounded, Quarterly, 1.00, Compounded, Semiannual, 0.0706, 4),
            (0.0100, Compounded, Annual, 1.00, Simple, Annual, 0.0100, 4),
            (0.0200, Simple, Annual, 1.00, Compounded, Annual, 0.0200, 4),
            (0.0300, Compounded, Semiannual, 0.50, Simple, Annual, 0.0300, 4),
            (0.0400, Simple, Annual, 0.50, Compounded, Semiannual, 0.0400, 4),
            (0.0500, Compounded, EveryFourthMonth, 1.0 / 3.0, Simple, Annual, 0.0500, 4),
            (0.0600, Simple, Annual, 1.0 / 3.0, Compounded, EveryFourthMonth, 0.0600, 4),
            (0.0500, Compounded, Quarterly, 0.25, Simple, Annual, 0.0500, 4),
            (0.0600, Simple, Annual, 0.25, Compounded, Quarterly, 0.0600, 4),
            (0.0700, Compounded, Bimonthly, 1.0 / 6.0, Simple, Annual, 0.0700, 4),
            (0.0800, Simple, Annual, 1.0 / 6.0, Compounded, Bimonthly, 0.0800, 4),
            (0.0900, Compounded, Monthly, 1.0 / 12.0, Simple, Annual, 0.0900, 4),
            (0.1000, Simple, Annual, 1.0 / 12.0, Compounded, Monthly, 0.1000, 4),
        ];

        let d1 = Date::new(6, Month::July, 2026);
        for &(r, comp, freq, t, comp2, freq2, expected, precision) in &cases {
            let ir = InterestRate::new(r, Actual360::new(), comp, freq)
                .expect("valid interest rate");
            let d2 = d1 + time_to_days(t);

            let compound = ir.compound_factor_between(d1, d2).expect("valid dates");
            let disc = ir.discount_factor_between(d1, d2).expect("valid dates");
            let error = (disc - 1.0 / compound).abs();
            assert!(
                error <= 1e-15,
                "{ir}: discount {disc} is not the reciprocal of compound {compound} ({error})"
            );

            let ir2 = ir
                .equivalent_rate_between(
                    ir.day_counter().clone(),
                    ir.compounding(),
                    ir.frequency(),
                    d1,
                    d2,
                )
                .expect("valid conversion");
            let error = (ir.rate() - ir2.rate()).abs();
            assert!(error <= 1e-15, "roundtrip of {ir} gave {ir2} ({error})");
            assert_eq!(ir.day_counter(), ir2.day_counter(), "roundtrip of {ir}");
            assert_eq!(ir.compounding(), ir2.compounding(), "roundtrip of {ir}");
            assert_eq!(ir.frequency(), ir2.frequency(), "roundtrip of {ir}");

            let ir3 = ir
                .equivalent_rate_between(ir.day_counter().clone(), comp2, freq2, d1, d2)
                .expect("valid conversion");
            let expected_ir =
                InterestRate::new(expected, ir.day_counter().clone(), comp2, freq2)
                    .expect("valid interest rate");
            let r3 = round_closest(ir3.rate(), precision);
            let error = (r3 - expected_ir.rate()).abs();
            assert!(
                error <= 1e-17,
                "{ir} converted to {ir3}, truncated to {r3}, expected {expected_ir} ({error})"
            );
            assert_eq!(ir3.day_counter(), expected_ir.day_counter(), "conversion of {ir}");
            assert_eq!(ir3.compounding(), expected_ir.compounding(), "conversion of {ir}");
            assert_eq!(ir3.frequency(), expected_ir.frequency(), "conversion of {ir}");
        }

        type CompositeRow = (Rate, Compounding, Frequency, Time, Compounding, Frequency);
        let composite: [CompositeRow; 15] = [
            (0.0300, SimpleThenCompounded, Semiannual, 0.25, Simple, Annual),
            (0.0300, SimpleThenCompounded, Semiannual, 0.25, Simple, Semiannual),
            (0.0300, SimpleThenCompounded, Semiannual, 0.25, Simple, Quarterly),
            (0.0300, SimpleThenCompounded, Semiannual, 0.50, Simple, Annual),
            (0.0300, SimpleThenCompounded, Semiannual, 0.50, Simple, Semiannual),
            (0.0300, SimpleThenCompounded, Semiannual, 0.75, Compounded, Semiannual),
            (0.0400, Simple, Semiannual, 0.25, SimpleThenCompounded, Quarterly),
            (0.0400, Simple, Semiannual, 0.25, SimpleThenCompounded, Semiannual),
            (0.0400, Simple, Semiannual, 0.25, SimpleThenCompounded, Annual),
            (0.0400, Compounded, Quarterly, 0.50, SimpleThenCompounded, Quarterly),
            (0.0400, Simple, Semiannual, 0.50, SimpleThenCompounded, Semiannual),
            (0.0400, Simple, Semiannual, 0.50, SimpleThenCompounded, Annual),
            (0.0400, Compounded, Quarterly, 0.75, SimpleThenCompounded, Quarterly),
            (0.0400, Compounded, Semiannual, 0.75, SimpleThenCompounded, Semiannual),
            (0.0400, Simple, Semiannual, 0.75, SimpleThenCompounded, Annual),
        ];
        for &(r, comp, freq, t, comp2, freq2) in &composite {
            let ir = InterestRate::new(r, Actual360::new(), comp, freq)
                .expect("valid interest rate");
            let d2 = d1 + time_to_days(t);
            let direct = comp != SimpleThenCompounded;
            assert_eq!(ir.discount_factor_between(d1, d2).is_ok(), direct, "{ir}");
            let error = ir
                .equivalent_rate_between(ir.day_counter().clone(), comp2, freq2, d1, d2)
                .unwrap_err();
            let rejected = if direct { comp2 } else { comp };
            assert_eq!(error.message(), composite_error(rejected), "{ir} to {comp2}");
        }
    }
}
