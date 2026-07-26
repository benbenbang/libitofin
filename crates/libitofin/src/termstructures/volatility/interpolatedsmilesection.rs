//! Interpolated smile section.
//!
//! Port of `ql/termstructures/volatility/interpolatedsmilesection.hpp`
//! (`Linear` specialization). An [`InterpolatedSmileSection`] is a
//! [`SmileSection`] built from a strike grid and the matching Black standard
//! deviations: it divides each standard deviation by `sqrt(exercise_time)` to
//! recover a volatility, interpolates those volatilities linearly across
//! strike, and floors the result at zero. This is the smile the interpolated
//! swaption volatility cube (#595) returns per (option date, swap tenor).
//!
//! ## Divergences from QuantLib
//!
//! - **D10 simplification (dropped handle/lazy plumbing).** C++ offers four
//!   constructors, two taking `std::vector<Handle<Quote>>` for live standard
//!   deviations, and derives from `LazyObject`: `performCalculations` refreshes
//!   `vols_[i] = stdDevHandles_[i]->value() / sqrt(exerciseTime)` and re-runs
//!   the interpolation whenever a quote notifies. The sole ported consumer
//!   (#595, `interpolatedswaptionvolatilitycube.cpp:105-107`) builds the plain
//!   `std::vector<Real>` form with values fixed at construction and never a
//!   live handle. This port keeps only that form: the divided volatilities are
//!   computed once in the constructor and the interpolation is built once, so
//!   the `Handle<Quote>` constructors and the whole `LazyObject` /
//!   `performCalculations` machinery are dropped. Live-quote smiles are tracked
//!   under #586.
//!
//! ## Deferred (visible)
//!
//! Tracked under #586:
//! - The `flatStrikeExtrapolation = true` branch of `volatilityImpl`
//!   (interpolatedsmilesection.hpp:229-235), which clamps a below-`minStrike`
//!   or above-`maxStrike` query to the boundary node instead of extending the
//!   end segment. The flag defaults to `false` in every C++ constructor and
//!   #595 never sets it, so only the `false` path is ported: the interpolation
//!   is queried with extrapolation enabled and the result floored at zero.
//! - The `Date`-based constructors are not ported; #595 passes an option time,
//!   so only the exercise-time form is provided.

use crate::errors::QlResult;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::linear::LinearInterpolation;
use crate::require;
use crate::termstructures::volatility::VolatilityType;
use crate::termstructures::volatility::smilesection::{SmileSection, SmileSectionBase};
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};

/// A [`SmileSection`] whose volatilities are a linear interpolation, across
/// strike, of standard deviations divided by `sqrt(exercise_time)`.
pub struct InterpolatedSmileSection {
    base: SmileSectionBase,
    interpolation: LinearInterpolation,
    atm_level: Rate,
}

impl InterpolatedSmileSection {
    /// Section built from an exercise time (C++'s `Time` constructor with the
    /// `Real` standard-deviation vector).
    ///
    /// `strikes` and `std_devs` are paired node-for-node; each standard
    /// deviation is divided by `sqrt(exercise_time)` once here, and the
    /// resulting volatilities are interpolated linearly with extrapolation
    /// enabled.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `strikes` is empty, when `strikes` and `std_devs`
    /// differ in length, or when `strikes` is not ascending. Propagates
    /// [`SmileSectionBase::with_exercise_time`] (negative exercise time) and
    /// [`LinearInterpolation::new`] (fewer than two nodes, non-strictly
    /// increasing strikes, or a non-finite node, e.g. from a zero exercise
    /// time).
    pub fn with_exercise_time(
        exercise_time: Time,
        strikes: Vec<Rate>,
        std_devs: Vec<Real>,
        atm_level: Rate,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shift: Rate,
    ) -> QlResult<InterpolatedSmileSection> {
        require!(!strikes.is_empty(), "strikes must not be empty");
        require!(
            strikes.len() == std_devs.len(),
            "strikes and std_devs must have equal length ({} vs {})",
            strikes.len(),
            std_devs.len()
        );
        require!(
            strikes.windows(2).all(|w| w[0] <= w[1]),
            "strikes must be sorted in ascending order"
        );

        let base = SmileSectionBase::with_exercise_time(
            exercise_time,
            day_counter,
            volatility_type,
            shift,
        )?;

        let sqrt_t = exercise_time.sqrt();
        let vols: Vec<Volatility> = std_devs.iter().map(|&sd| sd / sqrt_t).collect();
        let interpolation = LinearInterpolation::new(strikes, vols)?.with_extrapolation(true);

        Ok(InterpolatedSmileSection {
            base,
            interpolation,
            atm_level,
        })
    }
}

impl SmileSection for InterpolatedSmileSection {
    fn base(&self) -> &SmileSectionBase {
        &self.base
    }

    fn volatility_impl(&self, strike: Rate) -> QlResult<Volatility> {
        Ok(self.interpolation.value(strike)?.max(0.0))
    }

    fn min_strike(&self) -> Rate {
        self.interpolation.x_min()
    }

    fn max_strike(&self) -> Rate {
        self.interpolation.x_max()
    }

    fn atm_level(&self) -> Option<Rate> {
        Some(self.atm_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::option::OptionType;
    use crate::pricingengines::blackformula::black_formula;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    const TIME: Time = 4.0;
    const ATM: Rate = 0.04;

    fn strikes() -> Vec<Rate> {
        vec![0.02, 0.03, 0.04, 0.05, 0.06]
    }

    fn std_devs() -> Vec<Real> {
        vec![0.40, 0.36, 0.30, 0.32, 0.34]
    }

    fn section() -> InterpolatedSmileSection {
        InterpolatedSmileSection::with_exercise_time(
            TIME,
            strikes(),
            std_devs(),
            ATM,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap()
    }

    #[test]
    fn volatility_recovers_divided_std_dev_at_nodes() {
        let section = section();
        for (&k, &sd) in strikes().iter().zip(std_devs().iter()) {
            let expected = sd / TIME.sqrt();
            assert!(
                (section.volatility(k).unwrap() - expected).abs() < 1e-15,
                "strike={k}"
            );
        }
    }

    #[test]
    fn variance_round_trips_to_std_dev_squared() {
        let section = section();
        for (&k, &sd) in strikes().iter().zip(std_devs().iter()) {
            assert!(
                (section.variance(k).unwrap() - sd * sd).abs() < 1e-15,
                "strike={k}"
            );
        }
    }

    #[test]
    fn volatility_between_nodes_is_the_linear_blend() {
        let section = section();
        let mid = 0.035;
        let lo = std_devs()[1] / TIME.sqrt();
        let hi = std_devs()[2] / TIME.sqrt();
        let expected = 0.5 * (lo + hi);
        assert!((section.volatility(mid).unwrap() - expected).abs() < 1e-15);
    }

    #[test]
    fn extrapolation_extends_the_end_segments() {
        let section = section();
        let below = section.volatility(0.01).unwrap();
        let above = section.volatility(0.07).unwrap();
        assert!(below.is_finite());
        assert!(above.is_finite());

        let s0 = std_devs()[0] / TIME.sqrt();
        let s1 = std_devs()[1] / TIME.sqrt();
        let slope = (s1 - s0) / (strikes()[1] - strikes()[0]);
        let expected_below = s0 + (0.01 - strikes()[0]) * slope;
        assert!((below - expected_below).abs() < 1e-15);
    }

    #[test]
    fn far_extrapolation_is_floored_at_zero() {
        let section = InterpolatedSmileSection::with_exercise_time(
            TIME,
            vec![0.02, 0.03, 0.04],
            vec![0.60, 0.40, 0.20],
            ATM,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap();
        assert_eq!(section.volatility(1.0).unwrap(), 0.0);
    }

    #[test]
    fn strike_bounds_are_the_grid_ends() {
        let section = section();
        assert_eq!(section.min_strike(), 0.02);
        assert_eq!(section.max_strike(), 0.06);
    }

    #[test]
    fn atm_level_round_trips() {
        assert_eq!(section().atm_level(), Some(ATM));
    }

    #[test]
    fn option_price_prices_through_the_smile_with_shift() {
        let shift = 0.01;
        let section = InterpolatedSmileSection::with_exercise_time(
            TIME,
            strikes(),
            std_devs(),
            ATM,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            shift,
        )
        .unwrap();
        let strike = 0.05;
        let std_dev = section.variance(strike).unwrap().sqrt();
        let expected = black_formula(OptionType::Call, strike, ATM, std_dev, 1.0, shift).unwrap();
        let got = section.option_price(strike, OptionType::Call, 1.0).unwrap();
        assert!((got - expected).abs() < 1e-15);
    }

    #[test]
    fn unsorted_strikes_are_rejected() {
        assert!(
            InterpolatedSmileSection::with_exercise_time(
                TIME,
                vec![0.04, 0.03, 0.05],
                vec![0.30, 0.36, 0.32],
                ATM,
                Actual365Fixed::new(),
                VolatilityType::ShiftedLognormal,
                0.0,
            )
            .is_err()
        );
    }

    #[test]
    fn length_mismatch_is_rejected() {
        assert!(
            InterpolatedSmileSection::with_exercise_time(
                TIME,
                vec![0.02, 0.03, 0.04],
                vec![0.30, 0.36],
                ATM,
                Actual365Fixed::new(),
                VolatilityType::ShiftedLognormal,
                0.0,
            )
            .is_err()
        );
    }

    #[test]
    fn empty_strikes_are_rejected() {
        assert!(
            InterpolatedSmileSection::with_exercise_time(
                TIME,
                vec![],
                vec![],
                ATM,
                Actual365Fixed::new(),
                VolatilityType::ShiftedLognormal,
                0.0,
            )
            .is_err()
        );
    }
}
