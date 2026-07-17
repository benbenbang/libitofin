//! Pricing engines and their numeric cores.
//!
//! Port of the `ql/pricingengines` layer: the Black 1976 formula family,
//! the [`BlackCalculator`] greeks core, and the analytic vanilla engines
//! built on them.

pub mod blackcalculator;
pub mod blackformula;
pub mod bond;
pub mod capfloor;
pub mod swap;
pub mod swaption;
pub mod vanilla;

pub use blackcalculator::BlackCalculator;
pub use bond::{BondFunctions, DiscountingBondEngine};
pub use capfloor::BlackCapFloorEngine;
pub use swap::DiscountingSwapEngine;
pub use swaption::{
    BachelierSpec, BachelierSwaptionEngine, Black76Spec, BlackStyleSpec, BlackStyleSwaptionEngine,
    BlackSwaptionEngine, CashAnnuityModel, JamshidianSwaptionEngine,
};
pub use vanilla::AnalyticEuropeanEngine;

pub use blackformula::{
    black_formula, black_formula_asset_itm_probability, black_formula_cash_itm_probability,
    black_formula_forward_derivative, black_formula_std_dev_derivative,
    black_formula_std_dev_second_derivative, black_formula_vol_derivative,
};

#[cfg(test)]
pub(crate) mod hull_fixture {
    //! Hull's S=42, K=40, r=10%, q=0, sigma=20%, T=0.5 European option,
    //! shared by the blackformula and blackcalculator oracle tests.

    use crate::types::{Real, Time};

    pub(crate) const SPOT: Real = 42.0;
    pub(crate) const STRIKE: Real = 40.0;
    pub(crate) const MATURITY: Time = 0.5;
    pub(crate) const FORWARD: Real = 44.15338604779301;
    pub(crate) const DISCOUNT: Real = 0.951229424500714;
    pub(crate) const STD_DEV: Real = 0.14142135623730953;
}
