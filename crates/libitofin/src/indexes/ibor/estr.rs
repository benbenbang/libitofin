//! The ESTR index.
//!
//! Port of `ql/indexes/ibor/estr.{hpp,cpp}`. [`Estr`] is the Euro Short-Term
//! Rate fixed by the ECB: an [`OvernightIndex`] fixing "ESTR" on the [`Target`]
//! calendar in [`EUR`](Currency::eur) with zero settlement days and an
//! [`Actual360`] day counter. It adds no behaviour over [`OvernightIndex`] - it
//! is pure configuration, so [`Estr::new`] returns a plain [`OvernightIndex`].

use crate::currency::Currency;
use crate::handle::Handle;
use crate::indexes::iborindex::OvernightIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::target::Target;
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;

/// The ESTR index (`ql/indexes/ibor/estr.hpp`).
///
/// A zero-sized namespace for the ESTR constructor.
pub struct Estr;

impl Estr {
    /// Builds an ESTR index over the `forwarding` curve.
    ///
    /// Mirrors the C++ `Estr::Estr(h)` constructor (`estr.cpp:27`): family name
    /// "ESTR", zero settlement days, [`EUR`](Currency::eur), the [`Target`]
    /// calendar, and an [`Actual360`] day counter.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> OvernightIndex {
        OvernightIndex::new(
            "ESTR".into(),
            0,
            Currency::eur(),
            Target::new(),
            Actual360::new(),
            forwarding,
            settings,
        )
    }
}
