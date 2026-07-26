//! Flat smile section.
//!
//! Port of `ql/termstructures/volatility/flatsmilesection.{hpp,cpp}`. A
//! [`FlatSmileSection`] quotes one constant volatility across every strike: the
//! minimal instantiable [`SmileSection`] and the oracle vehicle for the layer.

use crate::errors::QlResult;
use crate::termstructures::volatility::VolatilityType;
use crate::termstructures::volatility::smilesection::{SmileSection, SmileSectionBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};

/// A [`SmileSection`] with one volatility for every strike.
#[derive(Clone, Debug)]
pub struct FlatSmileSection {
    base: SmileSectionBase,
    vol: Volatility,
    atm_level: Option<Rate>,
}

impl FlatSmileSection {
    /// Flat section anchored to a fixed reference date (C++'s `Date`
    /// constructor).
    ///
    /// # Errors
    ///
    /// Propagates [`SmileSectionBase::with_reference_date`].
    pub fn with_reference_date(
        exercise_date: Date,
        vol: Volatility,
        day_counter: DayCounter,
        reference_date: Date,
        atm_level: Option<Rate>,
        volatility_type: VolatilityType,
        shift: Rate,
    ) -> QlResult<FlatSmileSection> {
        let base = SmileSectionBase::with_reference_date(
            exercise_date,
            day_counter,
            reference_date,
            volatility_type,
            shift,
        )?;
        Ok(FlatSmileSection {
            base,
            vol,
            atm_level,
        })
    }

    /// Flat section built from an exercise time (C++'s `Time` constructor).
    ///
    /// # Errors
    ///
    /// Propagates [`SmileSectionBase::with_exercise_time`].
    pub fn with_exercise_time(
        exercise_time: Time,
        vol: Volatility,
        day_counter: DayCounter,
        atm_level: Option<Rate>,
        volatility_type: VolatilityType,
        shift: Rate,
    ) -> QlResult<FlatSmileSection> {
        let base = SmileSectionBase::with_exercise_time(
            exercise_time,
            day_counter,
            volatility_type,
            shift,
        )?;
        Ok(FlatSmileSection {
            base,
            vol,
            atm_level,
        })
    }
}

impl SmileSection for FlatSmileSection {
    fn base(&self) -> &SmileSectionBase {
        &self.base
    }

    fn volatility_impl(&self, _strike: Rate) -> QlResult<Volatility> {
        Ok(self.vol)
    }

    fn min_strike(&self) -> Rate {
        Real::MIN - self.shift()
    }

    fn max_strike(&self) -> Rate {
        Real::MAX
    }

    fn atm_level(&self) -> Option<Rate> {
        self.atm_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::option::OptionType;
    use crate::pricingengines::blackformula::black_formula;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    const VOL: Volatility = 0.25;
    const TIME: Time = 2.0;
    const ATM: Rate = 0.04;

    fn flat_time_form() -> FlatSmileSection {
        FlatSmileSection::with_exercise_time(
            TIME,
            VOL,
            Actual360::new(),
            Some(ATM),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap()
    }

    #[test]
    fn variance_is_vol_squared_times_time() {
        let section = flat_time_form();
        let expected = VOL * VOL * TIME;
        for strike in [0.01, 0.02, 0.04, 0.08, 0.20] {
            assert!((section.variance(strike).unwrap() - expected).abs() < 1e-15);
        }
    }

    #[test]
    fn volatility_is_flat_across_strikes() {
        let section = flat_time_form();
        for strike in [0.001, 0.04, 1.0, 100.0] {
            assert_eq!(section.volatility(strike).unwrap(), VOL);
        }
    }

    #[test]
    fn option_price_matches_black_formula() {
        let section = flat_time_form();
        let std_dev = VOL * TIME.sqrt();
        for strike in [0.02, 0.04, 0.08] {
            let expected = black_formula(OptionType::Call, strike, ATM, std_dev, 1.0, 0.0).unwrap();
            let got = section.option_price(strike, OptionType::Call, 1.0).unwrap();
            assert!((got - expected).abs() < 1e-15, "strike={strike}");
        }
    }

    #[test]
    fn date_ctor_computes_exercise_time() {
        let reference = Date::new(15, Month::June, 2026);
        let exercise = Date::new(15, Month::June, 2028);
        let dc = Actual360::new();
        let section = FlatSmileSection::with_reference_date(
            exercise,
            VOL,
            dc.clone(),
            reference,
            Some(ATM),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap();
        let expected = dc.year_fraction(reference, exercise);
        assert_eq!(section.exercise_time(), expected);
        assert_eq!(section.reference_date().unwrap(), reference);
    }

    #[test]
    fn option_price_without_atm_errors() {
        let section = FlatSmileSection::with_exercise_time(
            TIME,
            VOL,
            Actual360::new(),
            None,
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap();
        assert!(section.option_price(0.04, OptionType::Call, 1.0).is_err());
    }

    #[test]
    fn reference_date_errors_for_time_form() {
        assert!(flat_time_form().reference_date().is_err());
    }

    #[test]
    fn both_ctors_agree_for_the_same_effective_time() {
        let reference = Date::new(15, Month::June, 2026);
        let exercise = Date::new(15, Month::June, 2028);
        let dc = Actual360::new();
        let dated = FlatSmileSection::with_reference_date(
            exercise,
            VOL,
            dc.clone(),
            reference,
            Some(ATM),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap();
        let timed = FlatSmileSection::with_exercise_time(
            dc.year_fraction(reference, exercise),
            VOL,
            dc.clone(),
            Some(ATM),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap();
        for strike in [0.02, 0.04, 0.08] {
            assert_eq!(
                dated.volatility(strike).unwrap(),
                timed.volatility(strike).unwrap()
            );
            assert_eq!(
                dated.variance(strike).unwrap(),
                timed.variance(strike).unwrap()
            );
        }
    }

    #[test]
    fn null_reference_date_is_rejected() {
        let err = FlatSmileSection::with_reference_date(
            Date::new(15, Month::June, 2028),
            VOL,
            Actual360::new(),
            Date::null(),
            Some(ATM),
            VolatilityType::ShiftedLognormal,
            0.0,
        )
        .unwrap_err();
        assert!(err.message().contains("#586"));
    }

    #[test]
    fn exercise_before_reference_is_rejected() {
        assert!(
            FlatSmileSection::with_reference_date(
                Date::new(15, Month::June, 2025),
                VOL,
                Actual360::new(),
                Date::new(15, Month::June, 2026),
                Some(ATM),
                VolatilityType::ShiftedLognormal,
                0.0,
            )
            .is_err()
        );
    }

    #[test]
    fn strike_bounds_match_quantlib() {
        let section = flat_time_form();
        assert_eq!(section.min_strike(), Real::MIN);
        assert_eq!(section.max_strike(), Real::MAX);
    }
}
