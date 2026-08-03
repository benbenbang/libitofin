//! Inflation indexes.
//!
//! Port of `ql/indexes/inflationindex.{hpp,cpp}`'s `InflationIndex`
//! (`inflationindex.hpp:87`) and [`ZeroInflationIndex`]
//! (`inflationindex.hpp:156`), plus the `inflationPeriod` free function they
//! and every inflation term structure depend on, and the [`Cpi`] observation
//! conventions (`inflationindex.hpp:39`) that read a lagged fixing off a
//! [`ZeroInflationIndex`].
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
use crate::time::timeunit::TimeUnit;
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

/// A zero inflation index (`ZeroInflationIndex`, `inflationindex.hpp:156`).
///
/// It publishes the level of a price index once per [`frequency`] period, and
/// reads back either the stored figure or - once the period is too recent to
/// have been published - a forecast off an inflation curve.
///
/// ## Deferred: the forecast
///
/// The curve is a `Handle<ZeroInflationTermStructure>`
/// (`inflationindex.hpp:183`), a type this batch does not have. The handle
/// field, its `registerWith` and the `forecastFixing` computation
/// (`inflationindex.cpp:224-241`) all arrive with it in batch 2 of #705;
/// `forecast_fixing` here fails with the
/// message an empty [`Handle`](crate::handle::Handle) gives on dereference,
/// which is exactly what C++ raises for an index built with the defaulted
/// (empty) handle. Every forecast path is therefore live and observable now -
/// only the number it would return is missing.
///
/// [`frequency`]: InflationIndex::frequency
pub struct ZeroInflationIndex {
    base: InflationIndexBase,
}

impl ZeroInflationIndex {
    /// Builds a zero inflation index (`inflationindex.cpp:156-168`, less the
    /// curve handle - see the type docs).
    pub fn new(
        family_name: String,
        region: Region,
        revised: bool,
        frequency: Frequency,
        availability_lag: Period,
        currency: Currency,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        ZeroInflationIndex {
            base: InflationIndexBase::new(
                family_name,
                region,
                revised,
                frequency,
                availability_lag,
                currency,
                settings,
            ),
        }
    }

    /// The first day of the inflation period the latest stored figure
    /// describes.
    ///
    /// `ZeroInflationIndex::lastFixingDate` (`inflationindex.cpp:190-195`):
    /// the store holds a daily expansion, so its last date is the *end* of the
    /// published period; C++ attributes the figure to the period's first day
    /// and so does this. An index with no history is an error, C++'s
    /// `QL_REQUIRE(!fixings.empty())`.
    pub fn last_fixing_date(&self) -> QlResult<Date> {
        let last = match self.base.settings().last_fixing_date(&self.base.name()) {
            Some(date) => date,
            None => crate::fail!("no fixings stored for {}", self.base.name()),
        };
        Ok(inflation_period(last, self.frequency())?.0)
    }

    /// Whether `fixing_date` has to be forecast rather than read from history.
    ///
    /// `ZeroInflationIndex::needsForecast` (`inflationindex.cpp:206-233`), a
    /// three-way decision against the latest period that *could* have been
    /// published by today, `inflationPeriod(today - availabilityLag)`. Zero
    /// index fixings are never interpolated, so the date needed is always the
    /// first day of the fixing's own period.
    ///
    /// 1. Before that period: history must have been provided, so no forecast
    ///    - a missing fixing is then an error rather than a forecast.
    /// 2. After it: the figure cannot have been published yet, so forecast.
    ///    **This answers before consulting the store**, so a figure recorded
    ///    beyond the publication horizon is still forecast rather than
    ///    returned (`inflation.cpp:888-890`).
    /// 3. Inside it: forecast only if nothing is on record.
    pub fn needs_forecast(&self, fixing_date: Date) -> QlResult<bool> {
        let today = match self.base.settings().evaluation_date() {
            Some(today) => today,
            None => crate::fail!("no evaluation date set: an index fixing needs a reference date"),
        };
        let frequency = self.frequency();
        let latest_possible = inflation_period(today - self.availability_lag(), frequency)?;
        let latest_needed_date = inflation_period(fixing_date, frequency)?.0;

        if latest_needed_date < latest_possible.0 {
            Ok(false)
        } else if latest_needed_date > latest_possible.1 {
            Ok(true)
        } else {
            Ok(self
                .base
                .settings()
                .fixing(&self.base.name(), latest_needed_date)
                .is_none())
        }
    }

    /// The forecast fixing off the inflation curve - deferred to batch 2 of
    /// #705, see the type docs.
    fn forecast_fixing(&self, _fixing_date: Date) -> QlResult<Rate> {
        crate::fail!("empty Handle cannot be dereferenced")
    }
}

impl Index for ZeroInflationIndex {
    fn name(&self) -> String {
        self.base.name()
    }

    fn fixing_calendar(&self) -> Calendar {
        self.base.fixing_calendar()
    }

    /// Always true: an inflation figure belongs to a period, not to a business
    /// day (`inflationindex.hpp:107`).
    fn is_valid_fixing_date(&self, _fixing_date: Date) -> bool {
        true
    }

    /// The fixing at `fixing_date`, stored or forecast
    /// (`inflationindex.cpp:170-182`).
    ///
    /// `forecast_todays_fixing` is ignored, as the C++ warning at
    /// `inflationindex.hpp:175-177` documents: the choice between history and
    /// forecast is made by [`needs_forecast`](ZeroInflationIndex::needs_forecast)
    /// alone. A date the store should cover but does not is an error, not a
    /// forecast.
    fn fixing(&self, fixing_date: Date, _forecast_todays_fixing: bool) -> QlResult<Rate> {
        if self.needs_forecast(fixing_date)? {
            return self.forecast_fixing(fixing_date);
        }
        let (first, _) = inflation_period(fixing_date, self.frequency())?;
        match self.past_fixing(fixing_date)? {
            Some(fixing) => Ok(fixing),
            None => crate::fail!("Missing {} fixing for {}", self.base.name(), first),
        }
    }

    /// The stored figure of the period `fixing_date` falls in
    /// (`inflationindex.cpp:184-188`), which is filed on the period's first
    /// day.
    fn past_fixing(&self, fixing_date: Date) -> QlResult<Option<Rate>> {
        let (first, _) = inflation_period(fixing_date, self.frequency())?;
        Ok(self.base.settings().fixing(&self.base.name(), first))
    }

    /// Delegated to [`InflationIndexBase::add_fixing`], which spreads the
    /// figure over its whole inflation period. Inheriting the single-entry
    /// trait default here would silently break every read outside the
    /// published day.
    fn add_fixing(&self, fixing_date: Date, value: Rate) -> QlResult<()> {
        self.base.add_fixing(fixing_date, value)
    }

    fn settings(&self) -> &Settings<Date> {
        self.base.settings()
    }

    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl InflationIndex for ZeroInflationIndex {
    fn inflation_base(&self) -> &InflationIndexBase {
        &self.base
    }
}

/// How an observation interpolates between the index fixings bracketing it
/// (`CPI::InterpolationType`, `inflationindex.hpp:45-49`).
///
/// The deprecated `AsIndex` variant is **not ported**: it was a migration aid
/// for the coupons, deprecated in QuantLib 1.43, and its arm in
/// `laggedFixing` falls straight through to `Flat`
/// (`inflationindex.cpp:36-42`) - so it carries no behaviour of its own.
/// `testCpiAsIndexInterpolation` (`inflation.cpp:1410`) is therefore not
/// ported either; it re-checks the [`Flat`](CpiInterpolationType::Flat)
/// numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpiInterpolationType {
    /// Flat from the previous fixing.
    Flat,
    /// Linearly between the bracketing fixings.
    Linear,
}

/// The `CPI` namespace of `inflationindex.hpp:39-83`.
///
/// `CPI::laggedYoYRate`, the year-on-year sibling of
/// [`lagged_fixing`](Cpi::lagged_fixing) (`inflationindex.hpp:78-82`), is
/// **not ported**: it takes a `YoYInflationIndex`, a type this batch of #705
/// does not have. It arrives with that index.
pub enum Cpi {}

impl Cpi {
    /// The `index` fixing observed at `date` under `observation_lag` and
    /// `interpolation_type` (`CPI::laggedFixing`,
    /// `inflationindex.cpp:28-64`).
    ///
    /// `date` is the unlagged date - usually the payment date of an
    /// inflation-linked coupon. Subtracting the lag lands in the *fixing*
    /// period: a May date with a three-month lag observes February's figure,
    /// and March's too when interpolating.
    ///
    /// [`Flat`](CpiInterpolationType::Flat) reads the fixing of that lagged
    /// period. [`Linear`](CpiInterpolationType::Linear) advances from it to the
    /// next period's fixing by how far `date` has run into its own period,
    /// except when `date` is that period's first day: there the weight is zero,
    /// and C++ returns early rather than ask for the second fixing, which may
    /// need a forecast curve that is not set.
    ///
    /// # Errors
    ///
    /// Propagates whatever [`ZeroInflationIndex::fixing`](Index::fixing)
    /// raises: a period too old to be unpublished but absent from the history
    /// is a missing-fixing error, not a forecast, and it surfaces here rather
    /// than being swallowed.
    pub fn lagged_fixing(
        index: &ZeroInflationIndex,
        date: Date,
        observation_lag: Period,
        interpolation_type: CpiInterpolationType,
    ) -> QlResult<Rate> {
        let frequency = index.frequency();
        let fixing_period = inflation_period(date - observation_lag, frequency)?;
        let i0 = index.fixing(fixing_period.0, false)?;

        match interpolation_type {
            CpiInterpolationType::Flat => Ok(i0),
            CpiInterpolationType::Linear => {
                let interpolation_period = inflation_period(date, frequency)?;
                if date == interpolation_period.0 {
                    return Ok(i0);
                }

                let one_day = Period::new(1, TimeUnit::Days);
                let i1 = index.fixing(fixing_period.1 + one_day, false)?;

                let elapsed = (date - interpolation_period.0) as Rate;
                let length = ((interpolation_period.1 + one_day) - interpolation_period.0) as Rate;
                Ok(i0 + (i1 - i0) * elapsed / length)
            }
        }
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

    fn a_zero_index(frequency: Frequency, settings: Shared<Settings<Date>>) -> ZeroInflationIndex {
        ZeroInflationIndex::new(
            "RPI".into(),
            Region::uk(),
            false,
            frequency,
            Period::new(1, TimeUnit::Months),
            Currency::gbp(),
            settings,
        )
    }

    /// The fixture of `testZeroIndexFutureFixing` (`inflation.cpp:851-860`):
    /// it is 10 April 2024 and the index publishes monthly with a one-month
    /// availability lag, so the latest period that could have been published
    /// is March 2024 - `inflationPeriod(10 March 2024) = [1 Mar, 31 Mar]`. The
    /// history stops at February.
    fn an_april_2024_zero_index() -> ZeroInflationIndex {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(10, April, 2024));
        let index = a_zero_index(Frequency::Monthly, settings);
        for (date, value) in [
            (Date::new(1, December, 2023), 100.0),
            (Date::new(1, January, 2024), 100.1),
            (Date::new(1, February, 2024), 100.2),
        ] {
            index
                .add_fixing(date, value)
                .expect("adding a published figure");
        }
        index
    }

    /// The D11 write-side pin on the concrete type: [`ZeroInflationIndex`]'s
    /// own `impl Index` must route `add_fixing` to the base's daily expansion.
    /// Inheriting the trait's single-entry default (`index.rs:110`) instead
    /// stores 15 December alone, and the two inner dates - read raw, not
    /// through `inflation_period` - then fail.
    #[test]
    fn a_zero_fixing_covers_its_whole_inflation_period() {
        let index = a_zero_index(Frequency::Quarterly, shared(Settings::<Date>::new()));
        index
            .add_fixing(Date::new(15, December, 2007), 100.0)
            .expect("adding a fixing on a quarterly zero inflation index");

        assert!(index.has_historical_fixing(Date::new(30, November, 2007)));
        assert!(index.has_historical_fixing(Date::new(31, December, 2007)));

        assert!(!index.has_historical_fixing(Date::new(30, September, 2007)));
        assert!(!index.has_historical_fixing(Date::new(1, January, 2008)));
    }

    /// A figure is read at the first day of its period, whatever day inside
    /// the period is asked for (`inflationindex.cpp:184-188`).
    ///
    /// The plural `add_fixings` stays on the [`Index`] default and stores a
    /// single raw entry - the C++ asymmetry, where only `addFixing` is the
    /// virtual `InflationIndex` override - so 1 October is the *only* date on
    /// record here. Reading the asked-for date raw would answer `None`.
    #[test]
    fn a_past_fixing_is_read_at_the_start_of_its_period() {
        let index = a_zero_index(Frequency::Quarterly, shared(Settings::<Date>::new()));
        index
            .add_fixings([(Date::new(1, October, 2007), 100.0)])
            .expect("recording a single raw entry through the Index default");

        assert_eq!(
            index
                .past_fixing(Date::new(20, December, 2007))
                .expect("every date is a valid fixing date"),
            Some(100.0)
        );
    }

    /// Branch 1 of `needsForecast`: the period is well before the publication
    /// horizon, so the stored figure is returned rather than forecast.
    #[test]
    fn a_fixing_before_the_horizon_is_read_from_history() {
        let index = an_april_2024_zero_index();
        let february = Date::new(1, February, 2024);

        assert!(!index.needs_forecast(february).expect("a monthly index"));
        assert_eq!(
            index
                .fixing(february, false)
                .expect("February 2024 is published"),
            100.2
        );
    }

    /// Branch 1 again, on the path that discriminates it from a forecast: a
    /// period this old must have been published, so an absent figure is the
    /// `QL_REQUIRE` of `inflationindex.cpp:174-177`, not a forecast.
    #[test]
    fn a_missing_fixing_before_the_horizon_is_an_error() {
        let index = an_april_2024_zero_index();
        let november = Date::new(1, November, 2023);

        assert!(!index.needs_forecast(november).expect("a monthly index"));
        let error = index
            .fixing(november, false)
            .expect_err("November 2023 was never published");
        assert!(error.to_string().contains("Missing"), "err was: {error}");
    }

    /// Branch 3: March 2024 is the period that might just have been published,
    /// so the store decides - forecast while it is absent, read once it lands.
    #[test]
    fn a_fixing_inside_the_horizon_forecasts_only_while_absent() {
        let index = an_april_2024_zero_index();
        let march = Date::new(1, March, 2024);

        assert!(index.needs_forecast(march).expect("a monthly index"));
        let error = index
            .fixing(march, false)
            .expect_err("March 2024 has no figure yet and no curve to forecast off");
        assert!(
            error.to_string().contains("empty Handle"),
            "err was: {error}"
        );

        index
            .add_fixing(march, 100.3)
            .expect("March 2024 gets published");
        assert!(!index.needs_forecast(march).expect("a monthly index"));
        assert_eq!(
            index.fixing(march, false).expect("March 2024 is published"),
            100.3
        );
    }

    /// Branch 2, the one that must answer before consulting the store
    /// (`inflation.cpp:884-890`): April 2024 cannot have been published on 10
    /// April, so it is forecast - and stays forecast even after a figure is
    /// recorded for it. Probing the store first would return 100.4 here.
    #[test]
    fn a_fixing_beyond_the_horizon_forecasts_even_when_stored() {
        let index = an_april_2024_zero_index();
        let april = Date::new(1, April, 2024);

        assert!(index.needs_forecast(april).expect("a monthly index"));

        index
            .add_fixing(april, 100.4)
            .expect("recording a figure ahead of its publication");

        assert!(index.needs_forecast(april).expect("a monthly index"));
        let error = index
            .fixing(april, false)
            .expect_err("April 2024 is beyond the publication horizon");
        assert!(
            error.to_string().contains("empty Handle"),
            "err was: {error}"
        );
    }

    /// The figure is attributed to the first day of its period, where the
    /// store's own last date is the period's last day
    /// (`inflationindex.cpp:190-195`).
    #[test]
    fn the_last_fixing_date_is_the_start_of_the_published_period() {
        let index = a_zero_index(Frequency::Quarterly, shared(Settings::<Date>::new()));
        index
            .add_fixing(Date::new(15, December, 2007), 100.0)
            .expect("adding a fixing on a quarterly zero inflation index");

        assert_eq!(
            index.last_fixing_date().expect("the index has a history"),
            Date::new(1, October, 2007)
        );
    }

    #[test]
    fn the_last_fixing_date_of_an_empty_index_is_an_error() {
        let index = a_zero_index(Frequency::Quarterly, shared(Settings::<Date>::new()));
        let error = index
            .last_fixing_date()
            .expect_err("an index with no history has no last fixing date");
        assert!(
            error.to_string().contains("no fixings stored"),
            "err was: {error}"
        );
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
