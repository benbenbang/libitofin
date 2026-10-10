//! A zero-coupon bond with one calendar-adjusted redemption.
//!
//! The contractual maturity stays unadjusted, as in QuantLib's
//! `ql/instruments/bonds/zerocouponbond.{hpp,cpp}`. The underlying [`Bond`]
//! supplies settlement, pricing, yield, accrual and expiry behavior.

use crate::errors::QlResult;
use crate::instruments::Bond;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::types::{Natural, Real};

/// A bond paying no coupons and redeeming its face amount once.
///
/// Composes the existing [`Bond`] rather than inheriting from it. Its notional
/// ends on the adjusted payment date, not the contractual maturity date.
pub struct ZeroCouponBond {
    bond: Bond,
}

impl ZeroCouponBond {
    /// Builds a single-redemption bond.
    ///
    /// `redemption` is expressed per 100 of face amount. QuantLib's defaults
    /// are [`BusinessDayConvention::Following`], `100.0`, and no issue date;
    /// Rust makes these arguments explicit. Settings are an explicit handle
    /// instead of the native global singleton.
    ///
    /// The issue date floors settlement but is not checked against payment:
    /// the native constructor initializes its base with an empty coupon leg.
    ///
    /// # Errors
    ///
    /// Propagates the base bond and redemption cash-flow construction errors,
    /// including a null maturity date or a null redemption amount. Face amount,
    /// redemption percentage and their scaled payment must also be finite.
    /// This finite-value guard is stronger than the native constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        calendar: Calendar,
        face_amount: Real,
        maturity_date: Date,
        payment_convention: BusinessDayConvention,
        redemption: Real,
        issue_date: Option<Date>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        require!(face_amount.is_finite(), "face amount must be finite");
        require!(redemption.is_finite(), "redemption must be finite");
        require!(
            (face_amount * redemption / 100.0).is_finite(),
            "redemption amount must be finite"
        );
        require!(maturity_date != Date::null(), "null maturity date");
        let payment_date = calendar.adjust(maturity_date, payment_convention);
        let mut bond = Bond::new(settlement_days, calendar, issue_date, Vec::new(), settings)?;
        bond.set_single_redemption(face_amount, redemption, payment_date)?;
        bond.set_maturity_date(maturity_date);
        Ok(Self { bond })
    }

    /// The underlying bond for settlement, cash-flow and analytical accessors.
    pub fn bond(&self) -> &Bond {
        &self.bond
    }

    /// Mutable access for attaching an engine and reading lazy prices.
    pub fn bond_mut(&mut self) -> &mut Bond {
        &mut self.bond
    }

    /// Consumes the wrapper, transferring ownership of the underlying bond.
    pub fn into_bond(self) -> Bond {
        self.bond
    }
}
