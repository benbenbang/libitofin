//! The EU HICP index.
//!
//! Port of `ql/indexes/inflation/euhicp.hpp:33-88`. [`EuHicp`] is the euro-area
//! harmonised index of consumer prices: "HICP" over the [`EU`](Region::eu)
//! region, unrevised, published [`Monthly`](Frequency::Monthly) with a
//! one-month availability lag in [`EUR`](Currency::eur). It adds no behaviour
//! over [`ZeroInflationIndex`], so [`EuHicp::new`] returns a plain one.
//!
//! [`YyEuHicp`] and [`YyEuHicpXt`] are the quoted year-on-year siblings
//! `YYEUHICP` (`euhicp.hpp:62-73`) and `YYEUHICPXT` (`euhicp.hpp:75-88`): the
//! same metadata bar the family names, "YY_HICP" and "YY_HICPXT", over a
//! [`YoYInflationIndex`]. Quoted, not ratios - each is published as a
//! year-on-year rate in its own right and keeps its own history.
//!
//! Deferred: `EUHICPXT` (`euhicp.hpp:48-59`), the *zero* ex-tobacco variant,
//! which differs only in its family name and has no oracle in `inflation.cpp`.

use crate::currency::Currency;
use crate::indexes::inflationindex::{YoYInflationIndex, ZeroInflationIndex};
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

/// The quoted year-on-year EU HICP index (`ql/indexes/inflation/euhicp.hpp`).
///
/// A zero-sized namespace for the YY EU HICP constructor.
pub struct YyEuHicp;

impl YyEuHicp {
    /// Builds the quoted year-on-year EU HICP index, mirroring the C++
    /// `YYEUHICP::YYEUHICP(ts)` constructor (`euhicp.hpp:64-73`) less the
    /// year-on-year curve, which
    /// [`with_term_structure`](YoYInflationIndex::with_term_structure) links.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> YoYInflationIndex {
        YoYInflationIndex::new(
            "YY_HICP".into(),
            Region::eu(),
            false,
            Frequency::Monthly,
            Period::new(1, TimeUnit::Months),
            Currency::eur(),
            settings,
        )
    }
}

/// The quoted year-on-year EU HICP ex-tobacco index
/// (`ql/indexes/inflation/euhicp.hpp`).
///
/// A zero-sized namespace for the YY EU HICPXT constructor.
pub struct YyEuHicpXt;

impl YyEuHicpXt {
    /// Builds the quoted year-on-year EU HICPXT index, mirroring the C++
    /// `YYEUHICPXT::YYEUHICPXT(ts)` constructor (`euhicp.hpp:77-88`) less the
    /// year-on-year curve, which
    /// [`with_term_structure`](YoYInflationIndex::with_term_structure) links.
    /// It differs from [`YyEuHicp`] in its family name alone.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(settings: Shared<Settings<Date>>) -> YoYInflationIndex {
        YoYInflationIndex::new(
            "YY_HICPXT".into(),
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
    use crate::time::date::Month::{April, December, February, January, March};

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

    /// The quoted year-on-year sibling's metadata (`euhicp.hpp:64-73`), read
    /// the way `testQuotedYYIndex` (`inflation.cpp:933-953`) reads it on the
    /// base type. The composed name is what discriminates the family from
    /// [`YyEuHicpXt`], which is identical in every other field.
    #[test]
    fn the_year_on_year_construction_matches_the_cpp_header() {
        let index = YyEuHicp::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "EU YY_HICP");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert!(!index.ratio());
        assert!(index.underlying_index().is_none());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "EUR");
    }

    /// The quoted year-on-year ex-tobacco metadata (`euhicp.hpp:77-88`). Only
    /// the name separates it from [`YyEuHicp`], so the equality above is what
    /// makes either test discriminating.
    #[test]
    fn the_ex_tobacco_year_on_year_construction_matches_the_cpp_header() {
        let index = YyEuHicpXt::new(shared(Settings::<Date>::new()));

        assert_eq!(index.name(), "EU YY_HICPXT");
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert!(!index.revised());
        assert!(!index.ratio());
        assert!(index.underlying_index().is_none());
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "EUR");
    }

    /// `testZeroIndexFutureFixing` (`inflation.cpp:847-891`), run end to end
    /// through the concrete index and its own settings.
    ///
    /// It is 10 April 2024 and the history stops at February, so the latest
    /// period that could have been published is March. February reads back;
    /// March forecasts while absent and reads back once published; April is
    /// beyond the horizon and forecasts *even after a figure is recorded for
    /// it* - the discriminating assertion, since consulting the store first
    /// would answer 100.4 there. With no inflation curve every forecast fails
    /// on the empty handle, which is what C++ checks for too.
    ///
    /// `inflationindex.rs` pins the same three branches directly on
    /// [`ZeroInflationIndex::needs_forecast`]; this is the oracle's own
    /// sequence, read through [`Index::fixing`] on the named index.
    #[test]
    fn a_stored_figure_beyond_the_publication_horizon_still_forecasts() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(10, April, 2024));
        let index = EuHicp::new(settings);

        for (date, value) in [
            (Date::new(1, December, 2023), 100.0),
            (Date::new(1, January, 2024), 100.1),
            (Date::new(1, February, 2024), 100.2),
        ] {
            index
                .add_fixing(date, value)
                .expect("adding a published figure");
        }

        let february = index
            .fixing(Date::new(1, February, 2024), false)
            .expect("February 2024 is stored");
        assert!((february - 100.2).abs() < 1e-12, "{february}");

        let march = Date::new(1, March, 2024);
        let error = index
            .fixing(march, false)
            .expect_err("March 2024 has no figure yet and no curve to forecast off");
        assert!(
            error.to_string().contains("empty Handle"),
            "err was: {error}"
        );

        index
            .add_fixing(march, 100.3)
            .expect("March gets published");
        let published = index.fixing(march, false).expect("March 2024 is stored");
        assert!((published - 100.3).abs() < 1e-12, "{published}");

        let april = Date::new(1, April, 2024);
        let error = index
            .fixing(april, false)
            .expect_err("April 2024 cannot have been published on 10 April");
        assert!(
            error.to_string().contains("empty Handle"),
            "err was: {error}"
        );

        index
            .add_fixing(april, 100.4)
            .expect("recording a figure ahead of its publication");
        let error = index
            .fixing(april, false)
            .expect_err("April 2024 is beyond the horizon, stored or not");
        assert!(
            error.to_string().contains("empty Handle"),
            "err was: {error}"
        );
    }
}
