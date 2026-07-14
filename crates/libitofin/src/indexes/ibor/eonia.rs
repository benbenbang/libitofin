//! The Eonia index.
//!
//! Port of `ql/indexes/ibor/eonia.{hpp,cpp}`. [`Eonia`] is the Euro Overnight
//! Index Average fixed by the ECB: an [`OvernightIndex`] fixing "Eonia" on the
//! [`Target`] calendar in [`EUR`](Currency::eur) with zero settlement days and
//! an [`Actual360`] day counter. It adds no behaviour over [`OvernightIndex`] -
//! it is pure configuration, so [`Eonia::new`] returns a plain
//! [`OvernightIndex`].

use crate::currency::Currency;
use crate::handle::Handle;
use crate::indexes::iborindex::OvernightIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::target::Target;
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;

/// The Eonia index (`ql/indexes/ibor/eonia.hpp`).
///
/// A zero-sized namespace for the Eonia constructor.
pub struct Eonia;

impl Eonia {
    /// Builds an Eonia index over the `forwarding` curve.
    ///
    /// Mirrors the C++ `Eonia::Eonia(h)` constructor (`eonia.cpp:27`): family
    /// name "Eonia", zero settlement days, [`EUR`](Currency::eur), the
    /// [`Target`] calendar, and an [`Actual360`] day counter.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> OvernightIndex {
        OvernightIndex::new(
            "Eonia".into(),
            0,
            Currency::eur(),
            Target::new(),
            Actual360::new(),
            forwarding,
            settings,
        )
    }
}

#[cfg(test)]
mod tests {
    //! `eonia.cpp` construction table, the OIS swaption oracle's index (#361).

    use super::*;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    /// `Eonia::Eonia` (`eonia.cpp:27`): "Eonia", zero fixing days, EUR, TARGET,
    /// Actual/360, and the overnight configuration (one-day tenor, `Following`,
    /// no end-of-month) inherited from `OvernightIndex`.
    #[test]
    fn eonia_matches_the_quantlib_construction_table() {
        let settings = shared(Settings::<Date>::new());
        let index = Eonia::new(Handle::empty(), settings);

        assert_eq!(index.name(), "EoniaON Actual/360");
        assert_eq!(index.fixing_days(), 0);
        assert_eq!(*index.currency(), Currency::eur());
        assert_eq!(index.fixing_calendar().name(), "TARGET");
        assert_eq!(index.day_counter().name(), "Actual/360");
        assert_eq!(index.tenor(), Period::new(1, TimeUnit::Days));
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert!(!index.end_of_month());
    }
}
