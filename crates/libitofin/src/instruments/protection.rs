//! Default-protection side.
//!
//! Port of `ql/default.hpp:32-34` (`struct Protection { enum Side }`), whose
//! wrapper struct exists only to scope the enum. The port flattens the two
//! names into one, following
//! [`OptionType`](crate::option::OptionType) (`Option::Type`) and
//! [`FuturesType`](crate::instruments::FuturesType) (`Futures::Type`).
//!
//! The side tells a credit-default swap which leg it pays: the buyer pays the
//! premium and receives the default payment, the seller the reverse.
//!
//! ## Divergences from QuantLib
//!
//! - No integer discriminants are pinned. QuantLib's own `Protection::Side(-1)`
//!   "not set" sentinel (`ql/experimental/credit/nthtodefault.hpp:138`) only
//!   appears in the experimental basket instruments, which are out of scope for
//!   EPIC Credit (#676); the ported engines switch on the enum by value
//!   (`ql/pricingengines/credit/midpointcdsengine.cpp:134-138`).

/// Which side of a default-protection contract a party holds
/// (`Protection::Side`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectionSide {
    /// Pays the premium leg and receives the default payment.
    Buyer,
    /// Receives the premium leg and pays the default payment.
    Seller,
}
