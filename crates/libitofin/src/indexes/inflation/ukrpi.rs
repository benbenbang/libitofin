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
    use crate::errors::QlResult;
    use crate::indexes::index::Index;
    use crate::indexes::inflationindex::{
        Cpi, CpiInterpolationType, InflationIndex, inflation_period,
    };
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedkingdom::{Market, UnitedKingdom};
    use crate::time::date::Month::{
        August, December, February, January, June, March, May, November, October,
    };
    use crate::time::schedule::MakeSchedule;
    use crate::types::Rate;

    /// The 32 published figures of `testZeroIndex` (`inflation.cpp:272-279`),
    /// January 2005 through August 2007.
    const FIX_DATA: [Rate; 32] = [
        189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1,
        193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
        203.1, 204.4, 205.4, 206.2, 207.3, 206.1,
    ];

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

    /// `testZeroIndex`'s retrieval half (`inflation.cpp:255-311`): 32 monthly
    /// figures loaded off a schedule, one per publication date.
    ///
    /// Two things are pinned. The latest figure fixes
    /// [`last_fixing_date`](ZeroInflationIndex::last_fixing_date) at 1 August
    /// 2007 - the *start* of the period it describes, where the store's own
    /// last date is 31 August. And each figure reads back constant on every day
    /// of its period, which is the daily expansion `add_fixing` performs. The
    /// last schedule date is excluded, as in C++, and so are days at or past the
    /// publication horizon `inflationPeriod(today - lag).first` = 1 July 2007:
    /// beyond it the index forecasts, and there is no curve to forecast off.
    #[test]
    fn a_loaded_history_reads_back_constant_within_each_period() {
        let settings = shared(Settings::<Date>::new());
        let evaluation_date = UnitedKingdom::new(Market::Settlement).adjust(
            Date::new(13, August, 2007),
            BusinessDayConvention::Following,
        );
        settings.set_evaluation_date(evaluation_date);
        let index = UkRpi::new(settings);

        let schedule = MakeSchedule::new()
            .from(Date::new(1, January, 2005))
            .to(Date::new(1, August, 2007))
            .with_frequency(Frequency::Monthly)
            .build();
        let dates = schedule.dates();
        assert_eq!(dates.len(), FIX_DATA.len());
        assert_eq!(dates[0], Date::new(1, January, 2005));

        for (date, value) in dates.iter().zip(FIX_DATA) {
            index
                .add_fixing(*date, value)
                .expect("adding a published figure");
        }

        assert_eq!(
            index.last_fixing_date().expect("the index has a history"),
            Date::new(1, August, 2007)
        );

        let horizon = inflation_period(
            evaluation_date - index.availability_lag(),
            index.frequency(),
        )
        .expect("a monthly index")
        .0;

        for (date, value) in dates.iter().zip(FIX_DATA).take(dates.len() - 1) {
            let (first, last) =
                inflation_period(*date, index.frequency()).expect("a monthly index");
            for day in (0..=(last - first)).map(|offset| first + offset) {
                if day < horizon {
                    let fixing = index.fixing(day, false).expect("the period is published");
                    assert!((fixing - value).abs() < 1e-8, "{fixing} at {day}");
                }
            }
        }
    }

    /// `testZeroIndex`'s quarterly tail (`inflation.cpp:313-318`): a figure is
    /// attributed to the *first* day of its period, which only a non-monthly
    /// index discriminates.
    ///
    /// C++ uses `AUCPI`, which would drag an Australian region and currency in
    /// for this one assertion; a custom quarterly index stands in, since the
    /// attribution is a property of the frequency rather than of the country.
    /// It is deliberately keyed on its own name and settings: sharing "UK RPI"
    /// with the monthly history above would read that history through a
    /// quarterly period. `inflationindex.rs`'s
    /// `the_last_fixing_date_is_the_start_of_the_published_period` pins the same
    /// mechanism on the base type; this is the oracle's own tail.
    #[test]
    fn a_quarterly_figure_is_attributed_to_the_start_of_its_quarter() {
        let index = ZeroInflationIndex::new(
            "CPI".into(),
            Region::new("Custom", "XX"),
            false,
            Frequency::Quarterly,
            Period::new(1, TimeUnit::Months),
            Currency::gbp(),
            shared(Settings::<Date>::new()),
        );
        index
            .add_fixing(Date::new(15, December, 2007), 100.0)
            .expect("adding a fixing on a quarterly index");

        assert_eq!(
            index.last_fixing_date().expect("the index has a history"),
            Date::new(1, October, 2007)
        );
    }

    /// The fixture shared by `testCpiFlatInterpolation` and
    /// `testCpiLinearInterpolation` (`inflation.cpp:1346-1408`): it is 10
    /// February 2022 and UK RPI has published November 2020 through March 2021.
    /// Every date the two tests read is far behind the publication horizon, so
    /// nothing forecasts and a gap is an error.
    fn a_ukrpi_with_2021_fixings() -> ZeroInflationIndex {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(10, February, 2022));
        let index = UkRpi::new(settings);
        for (date, value) in [
            (Date::new(1, November, 2020), 293.5),
            (Date::new(1, December, 2020), 295.4),
            (Date::new(1, January, 2021), 294.6),
            (Date::new(1, February, 2021), 296.0),
            (Date::new(1, March, 2021), 296.9),
        ] {
            index
                .add_fixing(date, value)
                .expect("adding a published figure");
        }
        index
    }

    fn lagged_fixing(
        index: &ZeroInflationIndex,
        date: Date,
        interpolation_type: CpiInterpolationType,
    ) -> QlResult<Rate> {
        Cpi::lagged_fixing(
            index,
            date,
            Period::new(3, TimeUnit::Months),
            interpolation_type,
        )
    }

    /// `testCpiFlatInterpolation` (`inflation.cpp:1346-1373`): the observation
    /// is the fixing of the period the lagged date falls in, whatever day of
    /// that period it is.
    #[test]
    fn a_flat_observation_reads_the_lagged_period() {
        let index = a_ukrpi_with_2021_fixings();
        for (date, expected) in [
            (Date::new(10, February, 2021), 293.5),
            (Date::new(12, May, 2021), 296.0),
            (Date::new(25, June, 2021), 296.9),
        ] {
            let calculated = lagged_fixing(&index, date, CpiInterpolationType::Flat)
                .expect("the observed period is published");
            assert!(
                (calculated - expected).abs() < 1e-8,
                "{calculated} at {date}"
            );
        }
    }

    /// `testCpiLinearInterpolation` (`inflation.cpp:1375-1391`): the two
    /// bracketing fixings weighted by how far the date has run into its own
    /// (unlagged) period, whose length sets the denominator - 28 days for
    /// February 2021, 31 for May.
    #[test]
    fn a_linear_observation_interpolates_the_bracketing_fixings() {
        let index = a_ukrpi_with_2021_fixings();
        for (date, expected) in [
            (
                Date::new(10, February, 2021),
                293.5 * (19.0 / 28.0) + 295.4 * (9.0 / 28.0),
            ),
            (
                Date::new(12, May, 2021),
                296.0 * (20.0 / 31.0) + 296.9 * (11.0 / 31.0),
            ),
        ] {
            let calculated = lagged_fixing(&index, date, CpiInterpolationType::Linear)
                .expect("both observed periods are published");
            assert!(
                (calculated - expected).abs() < 1e-8,
                "{calculated} at {date}"
            );
        }
    }

    /// `inflation.cpp:1398-1401`: interpolating on 25 June needs the fixing
    /// after March's, i.e. April 2021 - a period old enough that it must have
    /// been published, so its absence is the missing-fixing error rather than a
    /// forecast. Asserting the message discriminates the two: routing the second
    /// fixing through the forecast branch also fails, with `empty Handle`.
    #[test]
    fn a_linear_observation_propagates_a_missing_second_fixing() {
        let index = a_ukrpi_with_2021_fixings();
        let error = lagged_fixing(
            &index,
            Date::new(25, June, 2021),
            CpiInterpolationType::Linear,
        )
        .expect_err("April 2021 was never published");
        assert!(error.to_string().contains("Missing"), "err was: {error}");
    }

    /// `inflation.cpp:1403-1407`: on the first day of the unlagged period the
    /// interpolation weight is zero, and C++ returns the first fixing without
    /// asking for the second - which is why 1 June succeeds where 25 June, one
    /// period and one missing April fixing later, does not.
    #[test]
    fn a_linear_observation_on_a_period_start_skips_the_second_fixing() {
        let index = a_ukrpi_with_2021_fixings();
        let calculated = lagged_fixing(
            &index,
            Date::new(1, June, 2021),
            CpiInterpolationType::Linear,
        )
        .expect("the special case never reads April");
        assert!((calculated - 296.9).abs() < 1e-8, "{calculated}");
    }
}
