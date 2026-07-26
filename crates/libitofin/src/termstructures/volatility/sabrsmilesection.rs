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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termstructures::volatility::sabr_volatility;
    use crate::time::date::Month;

    const FORWARD: Rate = 0.039;
    const EXPIRY: Time = 1.0;
    const ALPHA: Real = 0.3;
    const BETA: Real = 0.6;
    const NU: Real = 0.02;
    const RHO: Real = 0.01;

    fn fixture() -> SabrSmileSection {
        SabrSmileSection::with_exercise_time(
            EXPIRY,
            FORWARD,
            ALPHA,
            BETA,
            NU,
            RHO,
            0.0,
            VolatilityType::ShiftedLognormal,
        )
        .unwrap()
    }

    fn reference_vol(strike: Rate) -> Volatility {
        sabr_volatility(
            strike,
            FORWARD,
            EXPIRY,
            ALPHA,
            BETA,
            NU,
            RHO,
            VolatilityType::ShiftedLognormal,
        )
        .unwrap()
    }

    /// The section is a thin wrapper over #582's SABR formula, so its volatility
    /// must equal `sabr_volatility` exactly across the T1 fixture's 31 strikes
    /// plus the at-the-money forward.
    #[test]
    fn volatility_matches_sabr_volatility_across_strikes() {
        let section = fixture();
        for i in 0..31 {
            let strike = 0.03 + 0.002 * i as Real;
            let got = section.volatility(strike).unwrap();
            let expected = reference_vol(strike);
            assert!(
                (got - expected).abs() <= 1e-14,
                "strike {strike}: expected {expected}, got {got}"
            );
        }
        let atm = section.volatility(FORWARD).unwrap();
        assert!((atm - reference_vol(FORWARD)).abs() <= 1e-14);
    }

    #[test]
    fn atm_level_is_the_forward() {
        assert_eq!(fixture().atm_level(), Some(FORWARD));
    }

    /// The provided `variance` must equal `volatility^2 * exercise_time`, the
    /// C++ `varianceImpl` (identical because `volatility_impl` already clamps).
    #[test]
    fn variance_is_volatility_squared_times_time() {
        let section = fixture();
        for strike in [0.03, 0.039, 0.05, 0.07, 0.09] {
            let vol = section.volatility(strike).unwrap();
            assert_eq!(section.variance(strike).unwrap(), vol * vol * EXPIRY);
        }
    }

    /// Strikes below the clamp floor route through the unsafe SABR core: every
    /// sub-floor strike returns the vol at `0.00001`, and a strike `<= 0` (which
    /// #582's validated `sabr_volatility` rejects) still succeeds. If the section
    /// delegated to the validated function on the raw strike, `volatility(-0.01)`
    /// would be an `Err`, so its success proves the unsafe-core routing.
    #[test]
    fn sub_floor_strikes_route_through_unsafe_core() {
        let section = fixture();
        let floor_vol = section.volatility(0.00001).unwrap();
        assert_eq!(section.volatility(1e-9).unwrap(), floor_vol);
        assert_eq!(section.volatility(-0.01).unwrap(), floor_vol);
        assert!(
            sabr_volatility(
                -0.01,
                FORWARD,
                EXPIRY,
                ALPHA,
                BETA,
                NU,
                RHO,
                VolatilityType::ShiftedLognormal,
            )
            .is_err(),
            "the validated function must reject a non-positive strike"
        );
    }

    #[test]
    fn nonzero_shift_is_deferred_to_586() {
        let err = SabrSmileSection::with_exercise_time(
            EXPIRY,
            FORWARD,
            ALPHA,
            BETA,
            NU,
            RHO,
            0.01,
            VolatilityType::ShiftedLognormal,
        )
        .unwrap_err();
        assert!(err.message().contains("#586"), "{}", err.message());
    }

    #[test]
    fn normal_volatility_type_is_deferred_to_586() {
        let err = SabrSmileSection::with_exercise_time(
            EXPIRY,
            FORWARD,
            ALPHA,
            BETA,
            NU,
            RHO,
            0.0,
            VolatilityType::Normal,
        )
        .unwrap_err();
        assert!(err.message().contains("#586"), "{}", err.message());
    }

    #[test]
    fn bad_sabr_parameters_are_rejected() {
        assert!(
            SabrSmileSection::with_exercise_time(
                EXPIRY,
                FORWARD,
                0.0,
                BETA,
                NU,
                RHO,
                0.0,
                VolatilityType::ShiftedLognormal,
            )
            .is_err()
        );
        assert!(
            SabrSmileSection::with_exercise_time(
                EXPIRY,
                FORWARD,
                ALPHA,
                1.5,
                NU,
                RHO,
                0.0,
                VolatilityType::ShiftedLognormal,
            )
            .is_err()
        );
    }

    #[test]
    fn non_positive_forward_is_rejected() {
        assert!(
            SabrSmileSection::with_exercise_time(
                EXPIRY,
                0.0,
                ALPHA,
                BETA,
                NU,
                RHO,
                0.0,
                VolatilityType::ShiftedLognormal,
            )
            .is_err()
        );
    }

    #[test]
    fn accessors_return_the_stored_parameters() {
        let section = fixture();
        assert_eq!(section.alpha(), ALPHA);
        assert_eq!(section.beta(), BETA);
        assert_eq!(section.nu(), NU);
        assert_eq!(section.rho(), RHO);
    }

    /// The date form computes the exercise time from the day counter and agrees
    /// with the time form at that same effective time.
    #[test]
    fn date_form_matches_time_form_at_equal_time() {
        let reference = Date::new(15, Month::June, 2026);
        let exercise = Date::new(15, Month::June, 2027);
        let dc = Actual365Fixed::new();
        let dated = SabrSmileSection::with_reference_date(
            exercise,
            FORWARD,
            ALPHA,
            BETA,
            NU,
            RHO,
            dc.clone(),
            reference,
            0.0,
            VolatilityType::ShiftedLognormal,
        )
        .unwrap();
        let expected_time = dc.year_fraction(reference, exercise);
        assert_eq!(dated.exercise_time(), expected_time);

        let timed = SabrSmileSection::with_exercise_time(
            expected_time,
            FORWARD,
            ALPHA,
            BETA,
            NU,
            RHO,
            0.0,
            VolatilityType::ShiftedLognormal,
        )
        .unwrap();
        for strike in [0.03, 0.05, 0.09] {
            assert_eq!(
                dated.volatility(strike).unwrap(),
                timed.volatility(strike).unwrap()
            );
        }
    }
}
