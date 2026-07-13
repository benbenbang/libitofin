//! The SOFR index.
//!
//! Port of `ql/indexes/ibor/sofr.{hpp,cpp}`. [`Sofr`] is the Secured Overnight
//! Financing Rate: an [`OvernightIndex`] fixing "SOFR" on the [`UnitedStates`]
//! SOFR calendar in [`USD`](Currency::usd) with zero settlement days and an
//! [`Actual360`] day counter. It adds no behaviour over [`OvernightIndex`] - it
//! is pure configuration, so [`Sofr::new`] returns a plain [`OvernightIndex`].

use crate::currency::Currency;
use crate::handle::Handle;
use crate::indexes::iborindex::OvernightIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::unitedstates::{Market, UnitedStates};
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;

/// The SOFR index (`ql/indexes/ibor/sofr.hpp`).
///
/// A zero-sized namespace for the SOFR constructor.
pub struct Sofr;

impl Sofr {
    /// Builds a SOFR index over the `forwarding` curve.
    ///
    /// Mirrors the C++ `Sofr::Sofr(h)` constructor (`sofr.cpp:27`): family name
    /// "SOFR", zero settlement days, [`USD`](Currency::usd), the
    /// [`UnitedStates`] SOFR calendar, and an [`Actual360`] day counter.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> OvernightIndex {
        OvernightIndex::new(
            "SOFR".into(),
            0,
            Currency::usd(),
            UnitedStates::new(Market::Sofr),
            Actual360::new(),
            forwarding,
            settings,
        )
    }
}
