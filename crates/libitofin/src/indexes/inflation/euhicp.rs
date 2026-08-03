//! The EU HICP index.
//!
//! Port of `ql/indexes/inflation/euhicp.hpp:33-45`. [`EuHicp`] is the euro-area
//! harmonised index of consumer prices: "HICP" over the [`EU`](Region::eu)
//! region, unrevised, published [`Monthly`](Frequency::Monthly) with a
//! one-month availability lag in [`EUR`](Currency::eur). It adds no behaviour
//! over [`ZeroInflationIndex`], so [`EuHicp::new`] returns a plain one.
//!
//! Deferred: `EUHICPXT` (`euhicp.hpp:48-59`), the ex-tobacco variant, which
//! differs only in its family name and has no oracle in `inflation.cpp`.

use crate::currency::Currency;
use crate::indexes::inflationindex::ZeroInflationIndex;
use crate::indexes::region::Region;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// The EU HICP index (`ql/indexes/inflation/euhicp.hpp`).
///
/// A zero-sized namespace for the EU HICP constructor.
pub struct EuHicp;

impl EuHicp {
    /// Builds the EU HICP index, mirroring the C++ `EUHICP::EUHICP(ts)`
    /// constructor (`euhicp.hpp:36-45`) less the inflation curve, which
    /// arrives with the term structure in batch 2 of #705.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> ZeroInflationIndex {
        ZeroInflationIndex::new(
            "HICP".into(),
            Region::eu(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::eur(),
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

    /// `testZeroIndex`'s EU HICP block (`inflation.cpp:218-228`).
    #[test]
    fn construction_matches_the_cpp_table() {
        let index = EuHicp::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "EU HICP");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "EUR");
    }
}
