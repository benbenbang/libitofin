//! The US Consumer Price Index.
//!
//! Port of `ql/indexes/inflation/uscpi.hpp:33-58`. [`UsCpi`] is the US
//! consumer price inflation index: "CPI" over the [`USA`](Region::us) region,
//! unrevised, published [`Monthly`](Frequency::Monthly) with a one-month
//! availability lag in [`USD`](Currency::usd). It adds no behaviour over
//! [`ZeroInflationIndex`], so [`UsCpi::new`] returns a plain one.
//!
//! [`YyUsCpi`] is its quoted year-on-year sibling `YYUSCPI`
//! (`uscpi.hpp:47-58`): the same metadata bar the family name, "YY_CPI", and a
//! [`YoYInflationIndex`] rather than a zero one. Quoted, not a ratio - it is
//! published as a year-on-year rate in its own right and keeps its own history.

use crate::currency::Currency;
use crate::indexes::inflationindex::{YoYInflationIndex, ZeroInflationIndex};
use crate::indexes::region::Region;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// The US CPI index (`ql/indexes/inflation/uscpi.hpp`).
///
/// A zero-sized namespace for the US CPI constructor.
pub struct UsCpi;

impl UsCpi {
    /// Builds the US CPI index, mirroring the C++ `USCPI::USCPI(ts)`
    /// constructor (`uscpi.hpp:36-43`) less the inflation curve, which
    /// [`with_term_structure`](ZeroInflationIndex::with_term_structure) or
    /// [`clone_linked_to`](ZeroInflationIndex::clone_linked_to) links.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> ZeroInflationIndex {
        ZeroInflationIndex::new(
            "CPI".into(),
            Region::us(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::usd(),
            settings,
        )
    }
}

/// The quoted year-on-year US CPI index (`ql/indexes/inflation/uscpi.hpp`).
///
/// A zero-sized namespace for the YY US CPI constructor.
pub struct YyUsCpi;

impl YyUsCpi {
    /// Builds the quoted year-on-year US CPI index, mirroring the C++
    /// `YYUSCPI::YYUSCPI(ts)` constructor (`uscpi.hpp:50-57`) less the
    /// year-on-year curve, which
    /// [`with_term_structure`](YoYInflationIndex::with_term_structure) links.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> YoYInflationIndex {
        YoYInflationIndex::new(
            "YY_CPI".into(),
            Region::us(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::usd(),
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

    /// The metadata `uscpi.hpp:36-43` configures, read the way the ukrpi tests
    /// read UK RPI's. The composed name is `region.name() + " " + family_name`
    /// (`inflationindex.cpp:131`), so the USA region name - not its "US" code -
    /// is what the fixing-store key carries.
    #[test]
    fn construction_matches_the_cpp_header() {
        let index = UsCpi::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "USA CPI");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "USD");
    }

    /// The quoted year-on-year sibling's metadata (`uscpi.hpp:50-57`). The
    /// composed name is what discriminates the family: it is the only field
    /// separating one named quoted index from another.
    #[test]
    fn the_year_on_year_construction_matches_the_cpp_header() {
        let index = YyUsCpi::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "USA YY_CPI");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert!(!index.ratio());
        assert!(index.underlying_index().is_none());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "USD");
    }
}
