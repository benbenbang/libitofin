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
