//! Swaption pricing engines.
//!
//! Port of `ql/pricingengines/swaption/`: the shared
//! [`BlackStyleSwaptionEngine`] template with its shifted-lognormal
//! ([`BlackSwaptionEngine`]) and normal ([`BachelierSwaptionEngine`])
//! instantiations.

mod blackswaptionengine;

pub use blackswaptionengine::{
    BachelierSpec, BachelierSwaptionEngine, Black76Spec, BlackStyleSpec, BlackStyleSwaptionEngine,
    BlackSwaptionEngine, CashAnnuityModel,
};
