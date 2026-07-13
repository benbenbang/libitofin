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

#[cfg(test)]
mod tests {
    //! `sofr.cpp` construction table, the coupon oracle's index (#328).

    use super::*;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    /// `Sofr::Sofr` (`sofr.cpp:27`): "SOFR", zero fixing days, USD, the SOFR
    /// fixing calendar, Actual/360, and the overnight configuration (one-day
    /// tenor, `Following`, no end-of-month) inherited from `OvernightIndex`.
    #[test]
    fn sofr_matches_the_quantlib_construction_table() {
        let settings = shared(Settings::<Date>::new());
        let index = Sofr::new(Handle::empty(), settings);

        assert_eq!(index.name(), "SOFRON Actual/360");
        assert_eq!(index.fixing_days(), 0);
        assert_eq!(*index.currency(), Currency::usd());
        assert_eq!(index.fixing_calendar().name(), "SOFR fixing calendar");
        assert_eq!(index.day_counter().name(), "Actual/360");
        assert_eq!(index.tenor(), Period::new(1, TimeUnit::Days));
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert!(!index.end_of_month());
    }
}
