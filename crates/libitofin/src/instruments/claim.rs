//! Default-event claims.
//!
//! Port of `ql/instruments/claim.{hpp,cpp}`: the [`Claim`] contract a
//! credit-default swap settles its protection leg against, plus the
//! [`FaceValueClaim`] used by every standard contract.
//!
//! ## Divergences from QuantLib
//!
//! - C++ derives `Claim` from `Observable` and `Observer`, with an `update()`
//!   that only forwards notifications (`claim.hpp:32-39`). The port drops both
//!   roles: [`FaceValueClaim`] is stateless, so nothing can change and nothing
//!   subscribes. The wiring belongs to the one claim that needs it, and can be
//!   added to that type alone when it lands.
//! - `FaceValueAccrualClaim` (`claim.hpp:49-57`, `claim.cpp:32-44`) is
//!   deferred: it reads `accruedAmount(d)` and `notional(d)` off a reference
//!   [`Bond`](crate::instruments::Bond) and registers with it, so it needs both
//!   the bond accrual API and the observer wiring dropped above. It is deferred
//!   within EPIC Credit (#676).

use crate::time::date::Date;
use crate::types::Real;

/// The amount a default event pays out (`Claim`).
///
/// Implementors are held as `Shared<dyn Claim>` by the instruments that settle
/// against them, so the trait stays dyn-compatible.
pub trait Claim {
    /// The claim paid on a default at `default_date`, for a contract of
    /// `notional` recovering `recovery_rate` of it.
    fn amount(&self, default_date: &Date, notional: Real, recovery_rate: Real) -> Real;

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
    fn amount(&self, _default_date: &Date, notional: Real, recovery_rate: Real) -> Real {
        notional * (1.0 - recovery_rate)
    }

    /// Opts into the downcast seam: the ISDA engine settles face-value claims
    /// alone (`isdacdsengine.cpp:97-98`).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Shared, shared};
    use crate::time::date::Month;

    fn a_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    #[test]
    fn face_value_claim_pays_the_non_recovered_notional() {
        let claim = FaceValueClaim;
        assert_eq!(claim.amount(&a_date(), 100.0, 0.5), 50.0);
        assert_eq!(claim.amount(&a_date(), 1_000_000.0, 0.25), 750_000.0);
    }

    #[test]
    fn face_value_claim_at_the_recovery_extremes() {
        let claim = FaceValueClaim;
        assert_eq!(claim.amount(&a_date(), 100.0, 0.0), 100.0);
        assert_eq!(claim.amount(&a_date(), 100.0, 1.0), 0.0);
    }

    #[test]
    fn face_value_claim_ignores_the_default_date() {
        let claim = FaceValueClaim;
        let early = Date::new(1, Month::January, 2026);
        let late = Date::new(31, Month::December, 2030);
        assert_eq!(
            claim.amount(&early, 100.0, 0.5),
            claim.amount(&late, 100.0, 0.5)
        );
    }

    #[test]
    fn claim_is_dyn_compatible() {
        let claim: Shared<dyn Claim> = shared(FaceValueClaim);
        assert_eq!(claim.amount(&a_date(), 100.0, 0.5), 50.0);
    }
}
