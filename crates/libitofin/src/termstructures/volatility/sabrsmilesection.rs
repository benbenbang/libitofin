//! SABR smile section.
//!
//! Port of `ql/termstructures/volatility/sabrsmilesection.{hpp,cpp}`. A
//! [`SabrSmileSection`] is a [`SmileSection`] whose volatility comes from a fixed
//! set of Hagan SABR parameters: [`volatility_impl`](SmileSection::volatility_impl)
//! clamps the strike to the shifted domain floor and evaluates the closed-form
//! SABR formula ([`unsafe_sabr_volatility`]) at those parameters. There is no
//! calibration here (that is a separate track); this is the smile each expiry and
//! tenor of the SABR swaption vol cube hangs on.
//!
//! ## Parameters as explicit arguments
//!
//! QuantLib passes the four SABR parameters as a `std::vector<Real>` and indexes
//! `[0..3]`. This port takes `alpha`, `beta`, `nu`, `rho` as four explicit
//! arguments, matching the free functions in [`sabr`](super::sabr) and the rest
//! of the crate: the arity is fixed at four, so a slice buys nothing but a
//! runtime length check the type system already gives for free.
//!
//! ## Variance is the provided trait method
//!
//! C++ overrides `varianceImpl` to clamp the strike, evaluate the SABR vol, and
//! return `vol^2 * exerciseTime`. The [`SmileSection`] trait already provides
//! exactly that: `variance(strike) = volatility_impl(strike)^2 * exercise_time`.
//! Because this port's [`volatility_impl`](SmileSection::volatility_impl) performs
//! the same strike clamp before evaluating the vol, the provided `variance` is
//! numerically identical to the C++ override, so no override is needed.
//!
//! ## Divergences from QuantLib
//!
//! - The C++ `Time` constructor passes an empty `DayCounter()` to the base; it is
//!   never consulted, since the exercise time is supplied directly. This port has
//!   no empty-`DayCounter` state (see `daycounter.rs`), so the time form stores an
//!   inert [`Actual365Fixed`] placeholder and takes no day-counter argument,
//!   matching the C++ time-form signature.
//! - `QL_REQUIRE` guards become `Err` values per D4.
//!
//! ## Deferred to #586 (visible, no stubs)
//!
//! - A non-zero lognormal `shift` needs `shiftedSabrVolatility`, which #582
//!   deferred to #586. A non-zero shift is rejected in the constructor rather than
//!   silently accepted.
//! - [`VolatilityType::Normal`] needs the Normal/Bachelier SABR formula, also
//!   deferred to #586, and is likewise rejected in the constructor.

use crate::errors::QlResult;
use crate::termstructures::volatility::smilesection::{SmileSection, SmileSectionBase};
use crate::termstructures::volatility::{
    VolatilityType, unsafe_sabr_volatility, validate_sabr_parameters,
};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

/// A [`SmileSection`] whose volatility is the Hagan SABR formula at fixed
/// parameters.
#[derive(Clone, Debug)]
pub struct SabrSmileSection {
    base: SmileSectionBase,
    forward: Rate,
    alpha: Real,
    beta: Real,
    nu: Real,
    rho: Real,
}

impl SabrSmileSection {
    /// SABR section built from an exercise time (C++'s `Time` constructor).
    ///
    /// # Errors
    ///
    /// Returns `Err` when `shift` is non-zero or `volatility_type` is
    /// [`VolatilityType::Normal`] (both deferred to #586), when
    /// `forward + shift <= 0`, or when the SABR parameters are invalid.
    #[allow(clippy::too_many_arguments)]
    pub fn with_exercise_time(
        exercise_time: Time,
        forward: Rate,
        alpha: Real,
        beta: Real,
        nu: Real,
        rho: Real,
        shift: Rate,
        volatility_type: VolatilityType,
    ) -> QlResult<SabrSmileSection> {
        Self::initialise(forward, alpha, beta, nu, rho, shift, volatility_type)?;
        let base = SmileSectionBase::with_exercise_time(
            exercise_time,
            Actual365Fixed::new(),
            volatility_type,
            shift,
        )?;
        Ok(SabrSmileSection {
            base,
            forward,
            alpha,
            beta,
            nu,
            rho,
        })
    }

    /// SABR section anchored to a fixed reference date (C++'s `Date`
    /// constructor). The exercise time is the `day_counter` year fraction from
    /// `reference_date` to `exercise_date`.
    ///
    /// # Errors
    ///
    /// Returns `Err` for the same reasons as [`with_exercise_time`](Self::with_exercise_time),
    /// and additionally when `reference_date` is null (the floating path deferred
    /// to #586) or when `exercise_date` precedes `reference_date`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_reference_date(
        exercise_date: Date,
        forward: Rate,
        alpha: Real,
        beta: Real,
        nu: Real,
        rho: Real,
        day_counter: DayCounter,
        reference_date: Date,
        shift: Rate,
        volatility_type: VolatilityType,
    ) -> QlResult<SabrSmileSection> {
        Self::initialise(forward, alpha, beta, nu, rho, shift, volatility_type)?;
        let base = SmileSectionBase::with_reference_date(
            exercise_date,
            day_counter,
            reference_date,
            volatility_type,
            shift,
        )?;
        Ok(SabrSmileSection {
            base,
            forward,
            alpha,
            beta,
            nu,
            rho,
        })
    }

    /// Shared constructor guards, mirroring C++'s `initialise`: reject the
    /// deferred displaced and Normal paths, then require a positive shifted
    /// forward, then validate the SABR parameters once.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn initialise(
        forward: Rate,
        alpha: Real,
        beta: Real,
        nu: Real,
        rho: Real,
        shift: Rate,
        volatility_type: VolatilityType,
    ) -> QlResult<()> {
        require!(
            shift == 0.0,
            "a non-zero lognormal shift ({shift}) needs shiftedSabrVolatility, deferred to #586"
        );
        match volatility_type {
            VolatilityType::ShiftedLognormal => {}
            VolatilityType::Normal => {
                fail!("normal (Bachelier) SABR volatility is not yet ported (deferred to #586)")
            }
        }
        require!(
            forward + shift > 0.0,
            "at the money forward rate + shift must be positive: {forward} with shift {shift} \
             not allowed"
        );
        validate_sabr_parameters(alpha, beta, nu, rho)
    }

    /// The SABR `alpha` parameter.
    pub fn alpha(&self) -> Real {
        self.alpha
    }

    /// The SABR `beta` parameter.
    pub fn beta(&self) -> Real {
        self.beta
    }

    /// The SABR `nu` parameter.
    pub fn nu(&self) -> Real {
        self.nu
    }

    /// The SABR `rho` parameter.
    pub fn rho(&self) -> Real {
        self.rho
    }
}

impl SmileSection for SabrSmileSection {
    fn base(&self) -> &SmileSectionBase {
        &self.base
    }

    fn volatility_impl(&self, strike: Rate) -> QlResult<Volatility> {
        let strike = (0.00001 - self.shift()).max(strike);
        unsafe_sabr_volatility(
            strike,
            self.forward,
            self.exercise_time(),
            self.alpha,
            self.beta,
            self.nu,
            self.rho,
            self.volatility_type(),
        )
    }

    fn min_strike(&self) -> Rate {
        -self.shift()
    }

    fn max_strike(&self) -> Rate {
        Real::MAX
    }

    fn atm_level(&self) -> Option<Rate> {
        Some(self.forward)
    }
}
