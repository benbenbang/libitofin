//! Generic inflation indexes.
//!
//! Port of `ql/experimental/inflation/genericindexes.hpp`: [`YyGenericCpi`] is
//! the quoted year-on-year generic CPI `YYGenericCPI` (`genericindexes.hpp:57-71`),
//! a [`YoYInflationIndex`] parameterised by frequency, revision flag,
//! availability lag and currency over the `GenericRegion` (name `"Generic"`,
//! code `"GENERIC"`, `region.hpp:33-40` in that header's include chain). The
//! optionlet stripper builds one as a "fake index" whose lag and frequency
//! match the price surface it strips (`interpolatedyoyoptionletstripper.hpp:182-189`).
//!
//! ## Omitted (visible)
//!
//! The zero-coupon sibling `GenericCPI` (`genericindexes.hpp:43-53`) has no
//! consumer in the ported code and is omitted rather than carried unread; it
//! lands with whatever first needs it.

use crate::currency::Currency;
use crate::indexes::inflationindex::YoYInflationIndex;
use crate::indexes::region::Region;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::time::period::Period;

/// The quoted year-on-year generic CPI (`YYGenericCPI`).
///
/// A zero-sized namespace for the constructor, on the pattern of
/// [`YyUsCpi`](super::YyUsCpi).
pub struct YyGenericCpi;

impl YyGenericCpi {
    /// Builds the quoted year-on-year generic CPI, mirroring
    /// `YYGenericCPI::YYGenericCPI` (`genericindexes.hpp:59-70`) less the
    /// year-on-year curve, which
    /// [`with_term_structure`](YoYInflationIndex::with_term_structure) links.
    ///
    /// The family name is `"YY_CPI"` over the `GenericRegion`, so the composed
    /// index name reads `"Generic YY_CPI"`. Quoted, not a ratio: it forecasts
    /// off its curve and would read its own store for history.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        frequency: Frequency,
        revised: bool,
        availability_lag: Period,
        currency: Currency,
        settings: Shared<Settings<Date>>,
    ) -> YoYInflationIndex {
        YoYInflationIndex::new(
            "YY_CPI".into(),
            Region::new("Generic", "GENERIC"),
            revised,
            frequency,
            availability_lag,
            currency,
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
    use crate::time::timeunit::TimeUnit;

    /// The metadata `genericindexes.hpp:59-70` forwards, with the composed
    /// name carrying the `GenericRegion` name rather than its code.
    #[test]
    fn construction_matches_the_cpp_header() {
        let index = YyGenericCpi::new(
            Frequency::Monthly,
            false,
            Period::new(3, TimeUnit::Months),
            Currency::eur(),
            shared(Settings::<Date>::new()),
        );

        assert_eq!(index.name(), "Generic YY_CPI");
        assert_eq!(index.region().name(), "Generic");
        assert_eq!(index.region().code(), "GENERIC");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert!(!index.ratio());
        assert!(index.underlying_index().is_none());
        assert_eq!(index.availability_lag(), Period::new(3, TimeUnit::Months));
        assert_eq!(index.currency().code(), "EUR");
    }
}
