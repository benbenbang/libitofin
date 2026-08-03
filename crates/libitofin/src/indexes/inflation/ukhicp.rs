//! The UK HICP index.
//!
//! Port of `ql/indexes/inflation/ukhicp.hpp:33-38`. [`UkHicp`] is the UK
//! harmonised index of consumer prices: "HICP" over the [`UK`](Region::uk)
//! region, unrevised, published [`Monthly`](Frequency::Monthly) with a
//! one-month availability lag in [`GBP`](Currency::gbp). It adds no behaviour
//! over [`ZeroInflationIndex`], so [`UkHicp::new`] returns a plain one.

use crate::currency::Currency;
use crate::indexes::inflationindex::ZeroInflationIndex;
use crate::indexes::region::Region;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// The UK HICP index (`ql/indexes/inflation/ukhicp.hpp`).
///
/// A zero-sized namespace for the UK HICP constructor.
pub struct UkHicp;

impl UkHicp {
    /// Builds the UK HICP index, mirroring the C++ `UKHICP::UKHICP(ts)`
    /// constructor (`ukhicp.hpp:36-38`) less the inflation curve, which
    /// arrives with the term structure in batch 2 of #705.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> ZeroInflationIndex {
        ZeroInflationIndex::new(
            "HICP".into(),
            Region::uk(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::gbp(),
            settings,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::index::Index;
    use crate::indexes::inflationindex::InflationIndex;
    use crate::shared::shared;

    /// `testZeroIndex`'s UK HICP block (`inflation.cpp:242-252`).
    #[test]
    fn construction_matches_the_cpp_table() {
        let index = UkHicp::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "UK HICP");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "GBP");
    }
}
