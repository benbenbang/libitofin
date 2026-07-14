//! Swaption instrument and its settlement conventions.
//!
//! Port of `ql/instruments/swaption.{hpp,cpp}`. A [`Swaption`] is an option to
//! enter a [`FixedVsFloatingSwap`](crate::instruments::FixedVsFloatingSwap) at a
//! given [`Exercise`] date, settled per a [`SettlementType`] /
//! [`SettlementMethod`] pair.
//!
//! ## Divergences from QuantLib
//!
//! - `Swaption : Option` passes a null payoff (`swaption.cpp:137`) and
//!   `arguments::validate` never reaches `Option::arguments`'s payoff
//!   requirement, so there is no Rust option base: the type owns an
//!   [`InstrumentBase`](crate::instrument::InstrumentBase) directly and carries
//!   only the `exercise` that `Option::arguments` contributes.
//! - The `swap_` member is `SharedMut<FixedVsFloatingSwap>`, not the immutable
//!   [`Shared`], because the engine (#361, `blackswaptionengine.hpp:248-259`)
//!   sets a pricing engine on the swap and reads its `fairRate`, `fixedLegBPS`
//!   and `floatingLegBPS`, all of which take `&mut`. C++'s
//!   `shared_ptr<FixedVsFloatingSwap>` permits that mutation; the faithful Rust
//!   shared-mutable pointer is [`SharedMut`].
//! - `Settlement::Type`, `Settlement::Method` and
//!   `Settlement::checkTypeAndMethodConsistency` become the free
//!   [`SettlementType`], [`SettlementMethod`] and
//!   [`check_type_and_method_consistency`]; the consistency check returns a
//!   [`QlResult`] rather than throwing.
//! - `impliedVolatility` (needs the unported implied-vol solver family) and the
//!   `deepUpdate` observer optimisation are deferred; the ported tests reach
//!   neither. The `MakeSwaption` builder is deferred to #363 (it needs the
//!   unported `SwapIndex`).

use crate::errors::QlResult;
use crate::require;

/// How a swaption is settled on exercise (`Settlement::Type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettlementType {
    /// The holder enters the physical swap.
    #[default]
    Physical,
    /// The swap's value is settled in cash.
    Cash,
}

/// The convention used to settle a swaption (`Settlement::Method`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettlementMethod {
    /// Physical delivery, over-the-counter.
    #[default]
    PhysicalOTC,
    /// Physical delivery, cleared.
    PhysicalCleared,
    /// Cash settled at the collateralised cash price.
    CollateralizedCashPrice,
    /// Cash settled off the par-yield curve.
    ParYieldCurve,
}

/// Checks that a settlement type and method are compatible
/// (`Settlement::checkTypeAndMethodConsistency`, `swaption.cpp:207`).
///
/// Physical settlement pairs with [`PhysicalOTC`](SettlementMethod::PhysicalOTC)
/// or [`PhysicalCleared`](SettlementMethod::PhysicalCleared); cash settlement
/// pairs with
/// [`CollateralizedCashPrice`](SettlementMethod::CollateralizedCashPrice) or
/// [`ParYieldCurve`](SettlementMethod::ParYieldCurve).
///
/// # Errors
///
/// The type and method must match.
pub fn check_type_and_method_consistency(
    settlement_type: SettlementType,
    settlement_method: SettlementMethod,
) -> QlResult<()> {
    match settlement_type {
        SettlementType::Physical => require!(
            matches!(
                settlement_method,
                SettlementMethod::PhysicalOTC | SettlementMethod::PhysicalCleared
            ),
            "invalid settlement method for physical settlement"
        ),
        SettlementType::Cash => require!(
            matches!(
                settlement_method,
                SettlementMethod::CollateralizedCashPrice | SettlementMethod::ParYieldCurve
            ),
            "invalid settlement method for cash settlement"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Physical pairs with the two physical methods, cash with the two cash
    /// methods, and every cross pair is rejected (all eight combinations).
    #[test]
    fn consistency_accepts_matching_pairs_and_rejects_the_rest() {
        use SettlementMethod::{
            CollateralizedCashPrice, ParYieldCurve, PhysicalCleared, PhysicalOTC,
        };
        use SettlementType::{Cash, Physical};

        assert!(check_type_and_method_consistency(Physical, PhysicalOTC).is_ok());
        assert!(check_type_and_method_consistency(Physical, PhysicalCleared).is_ok());
        assert!(check_type_and_method_consistency(Cash, CollateralizedCashPrice).is_ok());
        assert!(check_type_and_method_consistency(Cash, ParYieldCurve).is_ok());

        assert_eq!(
            check_type_and_method_consistency(Physical, CollateralizedCashPrice)
                .unwrap_err()
                .message(),
            "invalid settlement method for physical settlement"
        );
        assert!(check_type_and_method_consistency(Physical, ParYieldCurve).is_err());
        assert_eq!(
            check_type_and_method_consistency(Cash, PhysicalOTC)
                .unwrap_err()
                .message(),
            "invalid settlement method for cash settlement"
        );
        assert!(check_type_and_method_consistency(Cash, PhysicalCleared).is_err());
    }
}
