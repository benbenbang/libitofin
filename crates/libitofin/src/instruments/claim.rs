//! Default-event claims.
//!
//! Port of `ql/instruments/claim.{hpp,cpp}`: the [`Claim`] contract a
//! credit-default swap settles its protection leg against, the
//! [`FaceValueClaim`] used by every standard contract, and the
//! [`FaceValueAccrualClaim`] that settles against a reference security.
//!
//! ## Divergences from QuantLib
//!
//! - C++ derives `Claim` from `Observable` and `Observer`, with an `update()`
//!   that only forwards notifications (`claim.hpp:32-39`), and
//!   `FaceValueAccrualClaim` registers with its reference security
//!   (`claim.cpp:32-35`). The port drops both roles from every claim: they hold
//!   no derived state to invalidate, so each reads its reference through on
//!   every call rather than caching behind a notification. Nothing here
//!   subscribes, so nothing has to be told.
//! - [`Claim::amount`] returns [`QlResult`] where C++ returns a bare `Real`
//!   (`claim.hpp:33-35`): [`FaceValueAccrualClaim`] reads its accrual off a
//!   fallible [`Bond`] API, and D4 propagates that rather than unwrapping it on
//!   the happy path.
//! - [`FaceValueAccrualClaim::amount`] fails on a zero reference notional,
//!   where `claim.cpp:41-43` divides by it unguarded and returns `NaN`. The
//!   port's [`Bond::notional`] reports a redeemed bond as `Ok(0.0)`
//!   (`bond.rs:279`), so the quotient is reachable from ordinary use rather
//!   than from a malformed bond alone, and D4 names it instead of propagating a
//!   `NaN` into a protection leg.
//!
//! [`Bond`]: crate::instruments::Bond
//! [`Bond::notional`]: crate::instruments::Bond::notional

use crate::errors::QlResult;
use crate::fail;
use crate::instruments::Bond;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::types::Real;

/// The amount a default event pays out (`Claim`).
///
/// Implementors are held as `Shared<dyn Claim>` by the instruments that settle
/// against them, so the trait stays dyn-compatible.
pub trait Claim {
    /// The claim paid on a default at `default_date`, for a contract of
    /// `notional` recovering `recovery_rate` of it.
    ///
    /// # Errors
    ///
    /// A claim reading a reference security propagates that security's
    /// lookups; one settling on the notional alone cannot fail.
    fn amount(&self, default_date: &Date, notional: Real, recovery_rate: Real) -> QlResult<Real>;

    /// The claim as [`Any`](std::any::Any), for the callers that must recover
    /// its concrete type from a `dyn Claim`.
    ///
    /// The port of C++'s `dynamic_pointer_cast<FaceValueClaim>`
    /// (`isdacdsengine.cpp:97`): `Rc` carries no downcast of its own, so a claim
    /// opts in by overriding this. The default `None` reads as "this claim
    /// declines to be introspected", which
    /// [`IsdaCdsEngine`](crate::pricingengines::credit::IsdaCdsEngine) - the
    /// only caller - treats as the C++ null-cast arm.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

/// A claim on the notional alone (`FaceValueClaim`).
///
/// Pays the non-recovered fraction of the notional, `N (1 - R)`
/// (`claim.cpp:24-28`).
#[derive(Debug, Clone, Copy)]
pub struct FaceValueClaim;

impl Claim for FaceValueClaim {
    /// Ignores the default date: the face value does not accrue
    /// (`claim.cpp:24-28`).
    fn amount(&self, _default_date: &Date, notional: Real, recovery_rate: Real) -> QlResult<Real> {
        Ok(notional * (1.0 - recovery_rate))
    }

    /// Opts into the downcast seam: the ISDA engine settles face-value claims
    /// alone (`isdacdsengine.cpp:97-98`).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

/// A claim on the notional of a reference security, including its accrual
/// (`FaceValueAccrualClaim`).
///
/// Pays `N (1 - R - a)`, where `a` is the reference security's accrual at the
/// default date normalised by its notional there (`claim.cpp:36-44`). The
/// accrual the protection buyer no longer collects is surrendered out of the
/// claim, so the payout falls short of the [`FaceValueClaim`] one by it.
#[derive(Clone)]
pub struct FaceValueAccrualClaim {
    reference_security: Shared<Bond>,
}

impl FaceValueAccrualClaim {
    /// A claim settling against `reference_security` (`claim.cpp:32-35`).
    #[must_use]
    pub fn new(reference_security: Shared<Bond>) -> FaceValueAccrualClaim {
        FaceValueAccrualClaim { reference_security }
    }
}

impl Claim for FaceValueAccrualClaim {
    /// # Errors
    ///
    /// Propagates the reference security's accrual and notional lookups, and
    /// fails where C++ would divide by a zero notional.
    fn amount(&self, default_date: &Date, notional: Real, recovery_rate: Real) -> QlResult<Real> {
        let reference_notional = self.reference_security.notional(Some(*default_date))?;
        if reference_notional == 0.0 {
            fail!("the reference security has no notional at {default_date}");
        }
        let accrual = self
            .reference_security
            .accrued_amount(Some(*default_date))?
            / reference_notional;
        Ok(notional * (1.0 - recovery_rate - accrual))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflow::{CashFlow, Leg};
    use crate::cashflows::FixedRateCoupon;
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn a_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    #[test]
    fn face_value_claim_pays_the_non_recovered_notional() {
        let claim = FaceValueClaim;
        assert_eq!(claim.amount(&a_date(), 100.0, 0.5).unwrap(), 50.0);
        assert_eq!(
            claim.amount(&a_date(), 1_000_000.0, 0.25).unwrap(),
            750_000.0
        );
    }

    #[test]
    fn face_value_claim_at_the_recovery_extremes() {
        let claim = FaceValueClaim;
        assert_eq!(claim.amount(&a_date(), 100.0, 0.0).unwrap(), 100.0);
        assert_eq!(claim.amount(&a_date(), 100.0, 1.0).unwrap(), 0.0);
    }

    #[test]
    fn face_value_claim_ignores_the_default_date() {
        let claim = FaceValueClaim;
        let early = Date::new(1, Month::January, 2026);
        let late = Date::new(31, Month::December, 2030);
        assert_eq!(
            claim.amount(&early, 100.0, 0.5).unwrap(),
            claim.amount(&late, 100.0, 0.5).unwrap()
        );
    }

    #[test]
    fn claim_is_dyn_compatible() {
        let claim: Shared<dyn Claim> = shared(FaceValueClaim);
        assert_eq!(claim.amount(&a_date(), 100.0, 0.5).unwrap(), 50.0);
    }

    fn issue_date() -> Date {
        Date::new(7, Month::July, 2026)
    }

    /// Two annual 5% coupons on a notional of 100, running 7 Jul 2026 to
    /// 7 Jul 2028 (the `bond.rs` par-bond fixture).
    fn par_bond() -> Shared<Bond> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(issue_date());
        let day_counter = Actual360::new();
        let coupons: Leg = vec![
            shared(FixedRateCoupon::from_rate(
                Date::new(7, Month::July, 2027),
                100.0,
                0.05,
                day_counter.clone(),
                issue_date(),
                Date::new(7, Month::July, 2027),
                None,
                None,
                None,
            )) as Shared<dyn CashFlow>,
            shared(FixedRateCoupon::from_rate(
                Date::new(7, Month::July, 2028),
                100.0,
                0.05,
                day_counter,
                Date::new(7, Month::July, 2027),
                Date::new(7, Month::July, 2028),
                None,
                None,
                None,
            )) as Shared<dyn CashFlow>,
        ];
        shared(
            Bond::from_coupons(
                2,
                NullCalendar::new(),
                Some(Date::new(1, Month::July, 2026)),
                coupons,
                settings,
            )
            .unwrap(),
        )
    }

    /// `claim.cpp:36-44` on a default half way through the first coupon: 184
    /// days of a 5% Actual/360 coupon have accrued on the notional of 100, so
    /// the reference security surrenders `5 x 184 / 360 / 100` of the claim.
    #[test]
    fn face_value_accrual_claim_surrenders_the_reference_accrual() {
        let claim = FaceValueAccrualClaim::new(par_bond());
        let mid_first_period = Date::new(7, Month::January, 2027);

        let amount = claim.amount(&mid_first_period, 100.0, 0.4).unwrap();

        assert!(
            (amount - 57.444_444_444_444_44).abs() < 1.0e-12,
            "the accrual claim paid {amount} rather than 57.44444444444444"
        );
    }

    /// The accrual is what separates the two claims: the face value alone pays
    /// the whole non-recovered notional (`claim.cpp:24-28`).
    #[test]
    fn face_value_accrual_claim_pays_under_the_face_value_claim() {
        let accrual_claim = FaceValueAccrualClaim::new(par_bond());
        let face_value_claim = FaceValueClaim;
        let mid_first_period = Date::new(7, Month::January, 2027);

        let accrual_amount = accrual_claim.amount(&mid_first_period, 100.0, 0.4).unwrap();
        let face_value_amount = face_value_claim
            .amount(&mid_first_period, 100.0, 0.4)
            .unwrap();

        assert_eq!(face_value_amount, 60.0);
        assert!(
            accrual_amount < face_value_amount,
            "the accrual claim paid {accrual_amount}, not under the face value {face_value_amount}"
        );
    }

    /// The divergence documented above: a reference security past its last
    /// notional date reports a zero notional, which C++ divides by.
    #[test]
    fn face_value_accrual_claim_fails_on_a_redeemed_reference() {
        let claim = FaceValueAccrualClaim::new(par_bond());
        let past_redemption = Date::new(8, Month::July, 2028);

        assert!(claim.amount(&past_redemption, 100.0, 0.4).is_err());
    }

    /// The accrual claim declines the downcast seam, so the ISDA engine reads
    /// it as the C++ null cast (`isdacdsengine.cpp:97-98`).
    #[test]
    fn face_value_accrual_claim_declines_the_downcast_seam() {
        let claim = FaceValueAccrualClaim::new(par_bond());

        assert!(claim.as_any().is_none());
    }
}
