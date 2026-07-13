//! Overnight-rate averaging conventions.
//!
//! Port of `ql/cashflows/rateaveraging.hpp`. An overnight coupon either
//! compounds its daily fixings ([`Compound`](RateAveraging::Compound), the
//! default) or averages them arithmetically ([`Simple`](RateAveraging::Simple)).

/// How the daily overnight fixings of a coupon are combined.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RateAveraging {
    /// Arithmetic average of the daily fixings.
    Simple,
    /// Daily compounding of the fixings (the coupon default).
    Compound,
}
