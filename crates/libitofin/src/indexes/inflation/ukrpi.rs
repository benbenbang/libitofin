//! The UK Retail Price Index.
//!
//! Port of `ql/indexes/inflation/ukrpi.hpp:33-39`. [`UkRpi`] is the UK retail
//! price inflation index: "RPI" over the [`UK`](Region::uk) region, unrevised,
//! published [`Monthly`](Frequency::Monthly) with a one-month availability lag
//! in [`GBP`](Currency::gbp). It adds no behaviour over [`ZeroInflationIndex`],
//! so [`UkRpi::new`] returns a plain one.
//!
//! Deferred: `YYUKRPI` (`ukrpi.hpp:42-53`), the quoted year-on-year sibling,
//! which needs the `YoYInflationIndex` this batch of #705 does not have.

use crate::currency::Currency;
use crate::indexes::inflationindex::ZeroInflationIndex;
use crate::indexes::region::Region;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// The UK RPI index (`ql/indexes/inflation/ukrpi.hpp`).
///
/// A zero-sized namespace for the UK RPI constructor.
pub struct UkRpi;

impl UkRpi {
    /// Builds the UK RPI index, mirroring the C++ `UKRPI::UKRPI(ts)`
    /// constructor (`ukrpi.hpp:37-39`) less the inflation curve, which arrives
    /// with the term structure in batch 2 of #705.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> ZeroInflationIndex {
        ZeroInflationIndex::new(
            "RPI".into(),
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

    /// `testZeroIndex`'s UK RPI block (`inflation.cpp:230-240`).
    #[test]
    fn construction_matches_the_cpp_table() {
        let index = UkRpi::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "UK RPI");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "GBP");
    }
}
