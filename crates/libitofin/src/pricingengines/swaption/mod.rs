//! Swaption pricing engines.
//!
//! Port of `ql/pricingengines/swaption/`. Only the shifted-lognormal
//! [`BlackSwaptionEngine`] is ported here; the Bachelier engine lands with #365.

mod blackswaptionengine;

pub use blackswaptionengine::{BlackSwaptionEngine, CashAnnuityModel};
