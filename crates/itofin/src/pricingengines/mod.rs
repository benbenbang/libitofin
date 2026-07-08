//! Pricing engines and their numeric cores.
//!
//! Port of the `ql/pricingengines` layer. Currently holds the Black 1976
//! formula family; the analytic vanilla engines build on it.

pub mod blackformula;

pub use blackformula::{
    black_formula, black_formula_asset_itm_probability, black_formula_cash_itm_probability,
    black_formula_forward_derivative, black_formula_std_dev_derivative,
    black_formula_std_dev_second_derivative, black_formula_vol_derivative,
};
