//! Futures type enumeration.
//!
//! Port of `ql/instruments/futures.hpp` (`struct Futures { enum Type }`). The
//! convention selects the date rule a futures contract settles on, and is read
//! only when building or validating a [`FuturesRateHelper`]'s schedule.
//!
//! [`FuturesRateHelper`]: crate::termstructures::yields::ratehelpers::FuturesRateHelper

/// The date convention a futures contract follows (`Futures::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuturesType {
    /// IMM dates: the third Wednesday of March, June, September, December.
    Imm,
    /// ASX dates: the second Friday of March, June, September, December.
    Asx,
    /// Any other rule; the start date is not validated and the maturity is the
    /// explicitly supplied end date.
    Custom,
}
