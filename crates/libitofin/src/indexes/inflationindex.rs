//! The inflation index base.
//!
//! Port of `ql/indexes/inflationindex.{hpp,cpp}`'s `InflationIndex`
//! (`inflationindex.hpp:87`), plus the `inflationPeriod` free function it and
//! every inflation term structure depend on.
//!
//! An inflation index is not a rate index: it has no tenor, no fixing days and
//! no value-date algebra, so it derives from [`Index`] directly rather than
//! from [`InterestRateIndex`](super::InterestRateIndex) - whose name
//! composition (tenor plus day counter) would key the D11 fixing store on the
//! wrong shape. The name here is `region.name() + " " + family_name`
//! (`inflationindex.cpp:131`), i.e. `"UK RPI"`, and that string *is* the store
//! key.
//!
//! Shared state lives in [`InflationIndexBase`], which a concrete inflation
//! index embeds and hands back through
//! [`inflation_base`](InflationIndex::inflation_base); the inspectors are
//! provided against it. Unlike [`InterestRateIndex`](super::InterestRateIndex),
//! the [`Index`] surface is *not* filled in by a blanket impl: a second
//! `impl<T: InflationIndex> Index for T` would overlap the existing
//! `impl<T: InterestRateIndex> Index for T` and fail coherence. A concrete
//! inflation index therefore writes its own `impl Index`, delegating to the
//! base (see [`InflationIndexBase::add_fixing`] for the one delegation that
//! carries behaviour and must not be forgotten).
//!
//! ## Divergences from QuantLib
//!
//! - **No `referenceDate_` member.** `inflationindex.hpp:139` declares one that
//!   the base never reads or writes; it is omitted rather than carried dead.
//! - **`forceOverwrite` dropped**, as on [`Index::add_fixing`]: the D11 store
//!   rejects a conflicting value and accepts an identical one, and has no
//!   overwrite mode to switch on.

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::indexes::index::Index;
use crate::indexes::region::Region;
use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::calendar::Calendar;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::{Date, Month};
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::types::{Integer, Rate};

/// The inflation period `date` falls in, at the given `frequency`.
///
/// Port of the `inflationPeriod` free function
/// (`inflationtermstructure.cpp:261-290`, declared at
/// `inflationtermstructure.hpp:242`). It lands in this module because the
/// inflation *index* is its first consumer - [`InflationIndexBase::add_fixing`]
/// spreads one published figure over the period it describes; the inflation
/// term structures reach for it from here rather than re-deriving it.
///
/// The returned pair is the first and last day of the period, both inclusive:
/// `Monthly` gives the calendar month of `date`, and every coarser frequency
/// the aligned block of `12 / frequency` months counted from January - so
/// `Quarterly` on any December day gives `[1 Oct, 31 Dec]`.
///
/// Frequencies finer than monthly, and the two sentinels, are rejected
/// (`QL_FAIL` in C++).
pub fn inflation_period(date: Date, frequency: Frequency) -> QlResult<(Date, Date)> {
    let month = date.month().ordinal();
    let year = date.year();

    let (start_month, end_month) = match frequency {
        Frequency::Annual
        | Frequency::Semiannual
        | Frequency::EveryFourthMonth
        | Frequency::Quarterly
        | Frequency::Bimonthly => {
            let n_months = 12 / (frequency as Integer);
            let start = month - (month - 1) % n_months;
            (start, start + n_months - 1)
        }
        Frequency::Monthly => (month, month),
        _ => crate::fail!("frequency not handled: {frequency}"),
    };

    Ok((
        Date::new(1, Month::from_ordinal(start_month), year),
        Date::end_of_month(Date::new(1, Month::from_ordinal(end_month), year)),
    ))
}

/// Shared state of every inflation index (`InflationIndex`'s members).
///
/// Built by [`new`](InflationIndexBase::new), which composes the index name as
/// the C++ constructor does and wires the index's forwarding observer to the
/// evaluation date and to its own fixing history (the constructor's two
/// `registerWith` calls, `inflationindex.cpp:132-133`).
pub struct InflationIndexBase {
    family_name: String,
    region: Region,
    revised: bool,
    frequency: Frequency,
    availability_lag: Period,
    currency: Currency,
    name: String,
    settings: Shared<Settings<Date>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl InflationIndexBase {
    /// Builds the shared state, wiring the index's observation of the
    /// evaluation date and its fixing history.
    ///
    /// The name is `region.name() + " " + family_name`
    /// (`inflationindex.cpp:131`), which doubles as the D11 fixing-store key.
    pub fn new(
        family_name: String,
        region: Region,
        revised: bool,
        frequency: Frequency,
        availability_lag: Period,
        currency: Currency,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        let name = format!("{} {}", region.name(), family_name);

        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        settings.register_eval_date_observer(&observer);
        settings.register_fixing_observer(&name, &observer);

        InflationIndexBase {
            family_name,
            region,
            revised,
            frequency,
            availability_lag,
            currency,
            name,
            settings,
            observable,
            forwarder,
        }
    }

    /// The composed index name, e.g. `"UK RPI"`.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The fixing calendar, a [`NullCalendar`].
    ///
    /// Inflation figures belong to months or quarters, not to days, so an
    /// inflation index has no fixing calendar; `inflationindex.cpp:137-140`
    /// returns a null one because the [`Index`] interface demands one.
    pub fn fixing_calendar(&self) -> Calendar {
        NullCalendar::new()
    }

    /// The observable the index broadcasts its changes through.
    pub fn observable(&self) -> &Observable {
        &self.observable
    }

    /// The forwarding observer the index registers with its dependencies.
    ///
    /// Construction wires it to the evaluation date and the fixing history; a
    /// concrete index additionally registers it with its inflation-curve
    /// handle, so relinking the curve re-broadcasts through
    /// [`observable`](InflationIndexBase::observable).
    ///
    /// Public where [`InterestRateIndexBase`](super::InterestRateIndexBase)
    /// keeps its counterpart crate-private: there, the blanket impl does the
    /// wiring, whereas an inflation index assembles its own [`Index`] surface
    /// and so must reach this itself.
    pub fn observer(&self) -> SharedMut<dyn Observer> {
        self.forwarder.clone() as SharedMut<dyn Observer>
    }

    /// The evaluation-date and fixing-history settings this index reads.
    pub fn settings(&self) -> &Shared<Settings<Date>> {
        &self.settings
    }

    /// Records a published figure across the whole inflation period it
    /// describes.
    ///
    /// `InflationIndex::addFixing` (`inflationindex.cpp:141-156`): one monthly
    /// or quarterly publication becomes a *daily* entry on every date of
    /// [`inflation_period`], so a later read on any day inside the period finds
    /// it. Every such date passes validity, the fixing calendar being null and
    /// `isValidFixingDate` always true (`inflationindex.hpp:107`).
    ///
    /// This is the one piece of [`Index`] behaviour an inflation index changes,
    /// and the blanket impl that would carry it automatically is unavailable
    /// (see the module docs). **Every `impl Index` for an inflation index must
    /// route its `add_fixing` here**; inheriting the trait default instead
    /// silently stores a single entry and breaks every read outside the
    /// published day.
    pub fn add_fixing(&self, fixing_date: Date, value: Rate) -> QlResult<()> {
        let (first, last) = inflation_period(fixing_date, self.frequency)?;
        let days = last - first + 1;
        let fixings = (0..days).map(|i| (first + i, value));
        self.settings.add_fixings(&self.name, fixings)
    }
}

/// The inflation index interface (`InflationIndex`).
///
/// A concrete inflation index supplies
/// [`inflation_base`](InflationIndex::inflation_base) and its own `impl`
/// [`Index`]; the inspectors are provided here.
pub trait InflationIndex: Index {
    /// The embedded shared state.
    fn inflation_base(&self) -> &InflationIndexBase;

    /// The family name, e.g. `"RPI"`.
    fn family_name(&self) -> &str {
        &self.inflation_base().family_name
    }

    /// The region the index measures.
    fn region(&self) -> &Region {
        &self.inflation_base().region
    }

    /// Whether the published figures are subject to revision.
    fn revised(&self) -> bool {
        self.inflation_base().revised
    }

    /// The publication frequency.
    fn frequency(&self) -> Frequency {
        self.inflation_base().frequency
    }

    /// The lag between the period a figure describes and its publication.
    ///
    /// January's inflation may only be available in April; it remains the
    /// January fixing regardless (`inflationindex.hpp:130-135`).
    fn availability_lag(&self) -> Period {
        self.inflation_base().availability_lag
    }

    /// The currency the index is quoted in.
    fn currency(&self) -> &Currency {
        &self.inflation_base().currency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{shared, shared_mut};
    use crate::time::date::Month::{
        April, December, February, January, March, November, October, September,
    };
    use crate::time::timeunit::TimeUnit;

    /// The minimal concrete inflation index: the [`Index`] surface delegated to
    /// the base, exactly as `ZeroInflationIndex` will delegate it, so the base
    /// is exercised through the interface a caller actually uses.
    struct TestInflationIndex {
        base: InflationIndexBase,
    }

    impl TestInflationIndex {
        fn new(frequency: Frequency) -> Self {
            TestInflationIndex::with_settings(frequency, shared(Settings::<Date>::new()))
        }

        fn with_settings(frequency: Frequency, settings: Shared<Settings<Date>>) -> Self {
            TestInflationIndex {
                base: InflationIndexBase::new(
                    "RPI".into(),
                    Region::uk(),
                    false,
                    frequency,
                    Period::new(1, TimeUnit::Months),
                    Currency::gbp(),
                    settings,
                ),
            }
        }
    }

    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    impl Index for TestInflationIndex {
        fn name(&self) -> String {
            self.base.name()
        }
        fn fixing_calendar(&self) -> Calendar {
            self.base.fixing_calendar()
        }
        fn is_valid_fixing_date(&self, _fixing_date: Date) -> bool {
            true
        }
        fn fixing(&self, fixing_date: Date, _forecast_todays_fixing: bool) -> QlResult<Rate> {
            match self.past_fixing(fixing_date)? {
                Some(rate) => Ok(rate),
                None => crate::fail!("no fixing for {fixing_date:?}"),
            }
        }
        fn settings(&self) -> &Settings<Date> {
            self.base.settings()
        }
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
        fn add_fixing(&self, fixing_date: Date, value: Rate) -> QlResult<()> {
            self.base.add_fixing(fixing_date, value)
        }
    }

    impl InflationIndex for TestInflationIndex {
        fn inflation_base(&self) -> &InflationIndexBase {
            &self.base
        }
    }

    #[test]
    fn name_is_region_plus_family() {
        let index = TestInflationIndex::new(Frequency::Monthly);
        assert_eq!(index.name(), "UK RPI");
    }

    #[test]
    fn inspectors_round_trip_construction() {
        let index = TestInflationIndex::new(Frequency::Monthly);
        assert_eq!(index.family_name(), "RPI");
        assert_eq!(index.region(), &Region::uk());
        assert!(!index.revised());
        assert_eq!(index.frequency(), Frequency::Monthly);
        assert_eq!(index.availability_lag(), Period::new(1, TimeUnit::Months));
        assert_eq!(index.currency().code(), "GBP");
    }

    #[test]
    fn every_date_is_a_valid_fixing_date() {
        let index = TestInflationIndex::new(Frequency::Monthly);
        let sunday = Date::new(2, Month::December, 2007);
        assert!(index.is_valid_fixing_date(sunday));
        assert!(index.fixing_calendar().is_business_day(sunday));
    }

    #[test]
    fn monthly_period_is_the_calendar_month() {
        let (first, last) = inflation_period(Date::new(14, February, 2007), Frequency::Monthly)
            .expect("monthly is a handled frequency");
        assert_eq!(first, Date::new(1, February, 2007));
        assert_eq!(last, Date::new(28, February, 2007));
    }

    #[test]
    fn monthly_period_ends_on_the_leap_day() {
        let (first, last) = inflation_period(Date::new(14, February, 2008), Frequency::Monthly)
            .expect("monthly is a handled frequency");
        assert_eq!(first, Date::new(1, February, 2008));
        assert_eq!(last, Date::new(29, February, 2008));
    }

    #[test]
    fn quarterly_period_aligns_to_the_calendar_quarter() {
        let (first, last) = inflation_period(Date::new(15, December, 2007), Frequency::Quarterly)
            .expect("quarterly is a handled frequency");
        assert_eq!(first, Date::new(1, October, 2007));
        assert_eq!(last, Date::new(31, December, 2007));

        let (first, last) = inflation_period(Date::new(3, January, 2007), Frequency::Quarterly)
            .expect("quarterly is a handled frequency");
        assert_eq!(first, Date::new(1, January, 2007));
        assert_eq!(last, Date::new(31, March, 2007));
    }

    #[test]
    fn coarser_periods_align_from_january() {
        let (first, last) = inflation_period(Date::new(15, December, 2007), Frequency::Semiannual)
            .expect("semiannual is a handled frequency");
        assert_eq!(first, Date::new(1, Month::July, 2007));
        assert_eq!(last, Date::new(31, December, 2007));

        let (first, last) = inflation_period(Date::new(15, December, 2007), Frequency::Annual)
            .expect("annual is a handled frequency");
        assert_eq!(first, Date::new(1, January, 2007));
        assert_eq!(last, Date::new(31, December, 2007));
    }

    #[test]
    fn finer_than_monthly_is_rejected() {
        assert!(inflation_period(Date::new(15, December, 2007), Frequency::Weekly).is_err());
        assert!(inflation_period(Date::new(15, December, 2007), Frequency::Once).is_err());
    }

    /// The D11 write-side pin: one published figure must land on every day of
    /// its inflation period, not on the publication day alone and not on the
    /// period's first day alone. The two inner dates fail under either of those
    /// wrong stores; the outer date fails under an over-wide expansion.
    #[test]
    fn a_fixing_covers_its_whole_inflation_period() {
        let index = TestInflationIndex::new(Frequency::Quarterly);
        index
            .add_fixing(Date::new(15, December, 2007), 100.0)
            .expect("adding a fixing on a quarterly inflation index");

        assert!(index.has_historical_fixing(Date::new(1, October, 2007)));
        assert!(index.has_historical_fixing(Date::new(30, November, 2007)));
        assert!(index.has_historical_fixing(Date::new(31, December, 2007)));

        assert!(!index.has_historical_fixing(Date::new(30, September, 2007)));
        assert!(!index.has_historical_fixing(Date::new(1, January, 2008)));
    }

    #[test]
    fn last_fixing_date_is_the_end_of_the_published_period() {
        let index = TestInflationIndex::new(Frequency::Quarterly);
        index
            .add_fixing(Date::new(15, December, 2007), 100.0)
            .expect("adding a fixing on a quarterly inflation index");

        assert_eq!(
            index.settings().last_fixing_date("UK RPI"),
            Some(Date::new(31, December, 2007))
        );
    }

    #[test]
    fn last_fixing_date_is_none_without_a_history() {
        let index = TestInflationIndex::new(Frequency::Quarterly);
        assert_eq!(index.settings().last_fixing_date("UK RPI"), None);
    }

    /// The constructor's two `registerWith` calls (`inflationindex.cpp:132-133`)
    /// plus the hook a concrete index uses for its inflation curve: all three
    /// reach the index's own observers through the one forwarding observable.
    #[test]
    fn the_index_re_broadcasts_its_dependencies() {
        let settings = shared(Settings::<Date>::new());
        let index = TestInflationIndex::with_settings(Frequency::Monthly, settings.clone());
        let flag = shared_mut(Flag::default());
        index
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        settings.set_evaluation_date(Date::new(3, December, 2007));
        assert!(flag.borrow().up);

        flag.borrow_mut().up = false;
        let curve = Observable::new();
        curve.register_observer(&index.inflation_base().observer());
        curve.notify_observers();
        assert!(flag.borrow().up);
    }

    /// `last_fixing_date` must reach the store through the same case-folding
    /// key as every other fixing accessor; reading it raw would answer `None`
    /// for a differently-cased name.
    #[test]
    fn last_fixing_date_is_case_insensitive() {
        let index = TestInflationIndex::new(Frequency::Quarterly);
        index
            .add_fixing(Date::new(15, April, 2007), 100.0)
            .expect("adding a fixing on a quarterly inflation index");

        let expected = Some(Date::new(30, Month::June, 2007));
        assert_eq!(index.settings().last_fixing_date("UK RPI"), expected);
        assert_eq!(index.settings().last_fixing_date("uk rpi"), expected);
    }
}
