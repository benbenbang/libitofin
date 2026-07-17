//! Swaption pricing engines.
//!
//! Port of `ql/pricingengines/swaption/`: the shared
//! [`BlackStyleSwaptionEngine`] template with its shifted-lognormal
//! ([`BlackSwaptionEngine`]) and normal ([`BachelierSwaptionEngine`])
//! instantiations, plus the model-based [`JamshidianSwaptionEngine`] (European
//! swaption pricing under Hull-White via the Jamshidian decomposition).

mod blackswaptionengine;
mod jamshidianswaptionengine;

pub use blackswaptionengine::{
    BachelierSpec, BachelierSwaptionEngine, Black76Spec, BlackStyleSpec, BlackStyleSwaptionEngine,
    BlackSwaptionEngine, CashAnnuityModel,
};
pub use jamshidianswaptionengine::JamshidianSwaptionEngine;
