//! Smile-section base.
//!
//! Port of `ql/termstructures/volatility/smilesection.{hpp,cpp}`.
//! [`SmileSection`] is the volatility-smile abstraction the vol layer has
//! deferred since B1: a single option expiry, holding the volatility as a
//! function of strike plus enough state to turn a variance into an option
//! price through the Black (or Bachelier) formula.
//!
//! C++'s `SmileSection` keeps its common state (reference/exercise date, day
//! counter, exercise time, volatility type, shift) on the base class. The trait
//! cannot hold fields, so this port mirrors the [`TermStructure`] /
//! [`TermStructureBase`](crate::termstructures::TermStructureBase) precedent:
//! [`SmileSectionBase`] owns the shared state and every implementor exposes it
//! through the required [`base`](SmileSection::base) accessor, with the provided
//! methods delegating to it exactly as the C++ base class does.
//!
//! ## Divergences from QuantLib
//!
//! - `atmLevel()` returns `Null<Real>()` when a section has no at-the-money
//!   level; here the required hook is [`atm_level`](SmileSection::atm_level)
//!   returning [`Option<Rate>`], and [`option_price`](SmileSection::option_price)
//!   turns the missing level into an `Err` (C++'s `QL_REQUIRE`).
//! - `referenceDate()` throws when unavailable; the port returns `Err` per D4.
//!
//! ## Deferred (visible)
//!
//! Tracked under #586:
//! - The floating construction (C++'s `Date` constructor with a defaulted
//!   reference date) tracks `Settings`' evaluation date through the
//!   Observable/Observer graph and recomputes the exercise time on every
//!   change. That path and the observer plumbing are a single coupled unit; a
//!   null reference date is therefore rejected here rather than silently
//!   floated. Consumers in B3a ([`FlatSmileSection`](super::FlatSmileSection),
//!   `SabrSmileSection`) are stateless after construction and do not need it.
//! - `digitalOptionPrice`, `vega`, `density`, the volatility-type-converting
//!   `volatility(strike, type, shift)`, `InterpolatedSmileSection`, and
//!   `SmileSectionUtils` are not ported.

use crate::errors::QlResult;
use crate::option::OptionType;
use crate::pricingengines::blackformula::{bachelier_black_formula, black_formula};
use crate::termstructures::volatility::VolatilityType;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

/// Shared state every [`SmileSection`] carries, mirroring the fields C++ keeps
/// on the `SmileSection` base class.
#[derive(Clone, Debug)]
pub struct SmileSectionBase {
    reference_date: Option<Date>,
    day_counter: DayCounter,
    exercise_time: Time,
    volatility_type: VolatilityType,
    shift: Rate,
}

impl SmileSectionBase {
    /// Base pinned to a fixed reference date, computing the exercise time as the
    /// day counter's year fraction from `reference_date` to `exercise_date`
    /// (C++'s `Date` constructor with an explicit reference date).
    ///
    /// # Errors
    ///
    /// Returns `Err` when `reference_date` is null (the floating path deferred to
    /// #586) or when `exercise_date` precedes `reference_date`.
    pub fn with_reference_date(
        exercise_date: Date,
        day_counter: DayCounter,
        reference_date: Date,
        volatility_type: VolatilityType,
        shift: Rate,
    ) -> QlResult<SmileSectionBase> {
        require!(
            reference_date != Date::null(),
            "a null reference date selects QuantLib's floating smile section, which tracks the \
             evaluation date through the observer graph; that path is deferred to #586"
        );
        require!(
            exercise_date >= reference_date,
            "exercise date ({exercise_date}) must not precede the reference date \
             ({reference_date})"
        );
        let exercise_time = day_counter.year_fraction(reference_date, exercise_date);
        Ok(SmileSectionBase {
            reference_date: Some(reference_date),
            day_counter,
            exercise_time,
            volatility_type,
            shift,
        })
    }

    /// Base pinned to an exercise time directly, with no reference date
    /// (C++'s `Time` constructor).
    ///
    /// # Errors
    ///
    /// Returns `Err` when `exercise_time` is negative.
    pub fn with_exercise_time(
        exercise_time: Time,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shift: Rate,
    ) -> QlResult<SmileSectionBase> {
        if exercise_time < 0.0 {
            fail!("exercise time ({exercise_time}) must be non-negative");
        }
        Ok(SmileSectionBase {
            reference_date: None,
            day_counter,
            exercise_time,
            volatility_type,
            shift,
        })
    }
}

/// Interest-rate volatility smile section.
///
/// A single option expiry viewed as volatility against strike. Implementors
/// supply the state holder through [`base`](Self::base) and the smile shape
/// through [`volatility_impl`](Self::volatility_impl), [`min_strike`](Self::min_strike),
/// [`max_strike`](Self::max_strike) and [`atm_level`](Self::atm_level); the
/// provided methods derive variance, volatility and the option price from them.
pub trait SmileSection {
    /// The shared state holder.
    fn base(&self) -> &SmileSectionBase;

    /// Volatility at `strike`; the caller owns any range checking.
    ///
    /// # Errors
    ///
    /// Propagates an implementor's failure to evaluate the smile.
    fn volatility_impl(&self, strike: Rate) -> QlResult<Volatility>;

    /// The lowest strike the section can quote.
    fn min_strike(&self) -> Rate;

    /// The highest strike the section can quote.
    fn max_strike(&self) -> Rate;

    /// The at-the-money level, or `None` when the section provides none.
    fn atm_level(&self) -> Option<Rate>;

    /// Volatility at `strike`.
    ///
    /// # Errors
    ///
    /// Propagates [`volatility_impl`](Self::volatility_impl).
    fn volatility(&self, strike: Rate) -> QlResult<Volatility> {
        self.volatility_impl(strike)
    }

    /// Black variance at `strike`: `volatility^2 * exercise_time` (C++'s
    /// `varianceImpl`).
    ///
    /// # Errors
    ///
    /// Propagates [`volatility_impl`](Self::volatility_impl).
    fn variance(&self, strike: Rate) -> QlResult<Real> {
        let vol = self.volatility_impl(strike)?;
        Ok(vol * vol * self.exercise_time())
    }

    /// The exercise time this section was built for.
    fn exercise_time(&self) -> Time {
        self.base().exercise_time
    }

    /// The day counter used to turn dates into times.
    fn day_counter(&self) -> DayCounter {
        self.base().day_counter.clone()
    }

    /// The volatility model this section is quoted against.
    fn volatility_type(&self) -> VolatilityType {
        self.base().volatility_type
    }

    /// The lognormal shift applied to strike and forward.
    fn shift(&self) -> Rate {
        self.base().shift
    }

    /// The reference date the section is anchored to.
    ///
    /// # Errors
    ///
    /// Returns `Err` for a section built from an exercise time, which carries no
    /// reference date (C++'s `referenceDate()` throw).
    fn reference_date(&self) -> QlResult<Date> {
        match self.base().reference_date {
            Some(date) => Ok(date),
            None => fail!("reference date not available for this instance"),
        }
    }

    /// Undiscounted-times-`discount` price of a European option struck at
    /// `strike`, priced through the section's volatility model.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the section provides no at-the-money level, or when
    /// the underlying Black/Bachelier formula rejects its inputs.
    fn option_price(
        &self,
        strike: Rate,
        option_type: OptionType,
        discount: Real,
    ) -> QlResult<Real> {
        let Some(atm) = self.atm_level() else {
            fail!("smile section must provide an atm level to compute an option price");
        };
        match self.volatility_type() {
            VolatilityType::ShiftedLognormal => {
                let shift = self.shift();
                let std_dev = if (strike + shift).abs() < Real::EPSILON {
                    0.2
                } else {
                    self.variance(strike)?.sqrt()
                };
                black_formula(option_type, strike, atm, std_dev, discount, shift)
            }
            VolatilityType::Normal => {
                let std_dev = self.variance(strike)?.sqrt();
                bachelier_black_formula(option_type, strike, atm, std_dev, discount)
            }
        }
    }
}
