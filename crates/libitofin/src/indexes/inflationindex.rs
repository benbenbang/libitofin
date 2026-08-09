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
//! - **No `interpolated_` member on [`YoYInflationIndex`]**. C++ declares one
//!   (`inflationindex.hpp:253`), deprecated in 1.43, that nothing ever assigns:
//!   it is `false` at declaration and no constructor, subclass or setter moves
//!   it. Its four readers are the branches of `pastFixing`, `needsForecast` and
//!   `forecastFixing` ported here (`inflationindex.cpp:304,335,353,376`) plus
//!   the `AsIndex` arm of `detail::CPI::effectiveInterpolationType`
//!   (`:417`), and `AsIndex` is itself unported (see
//!   [`CpiInterpolationType`]). The field is therefore carried neither as
//!   state nor as an `interpolated()` inspector, and only the live
//!   non-interpolated branch of each of those three methods is ported.
//! - **`inflationYearFraction` is homed here**, beside [`inflation_period`],
//!   where C++ declares it with the term structures
//!   (`inflationtermstructure.hpp:247`). It is the same period-quantizing
//!   date arithmetic as its neighbour, and the zero index's forecast is its
//!   only caller so far.

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::index::Index;
use crate::indexes::region::Region;
use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::inflation::inflationtermstructure::{
    YoYInflationTermStructure, ZeroInflationTermStructure,
};
use crate::time::calendar::Calendar;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::{Date, Month};
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Rate, Time};

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

/// The inflation time between `d1` and `d2` under `frequency`.
///
/// Port of the `inflationYearFraction` free function
/// (`inflationtermstructure.cpp:290-310`, declared at
/// `inflationtermstructure.hpp:247`); it lands here rather than in the term
/// structures, see the module divergences.
///
/// An interpolated index reads a figure that moves within its period, so the
/// plain day count between the two dates is the time between them. A
/// non-interpolated one holds one figure for the whole period, so the time
/// that matters is the one between the two *period starts* -
/// [`inflation_period`] is applied to **both** ends, not just to `d2`. The
/// caller usually passes a `d1` that is already a period start, which makes
/// the quantization look redundant; it is not, and a `d1` inside its period
/// takes the same time as its period's first day.
pub fn inflation_year_fraction(
    frequency: Frequency,
    index_is_interpolated: bool,
    day_counter: &DayCounter,
    d1: Date,
    d2: Date,
) -> QlResult<Time> {
    if index_is_interpolated {
        return Ok(day_counter.year_fraction(d1, d2));
    }
    let (first_of_d1, _) = inflation_period(d1, frequency)?;
    let (first_of_d2, _) = inflation_period(d2, frequency)?;
    Ok(day_counter.year_fraction(first_of_d1, first_of_d2))
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
/// have been published - a forecast off the inflation curve it holds
/// (`zeroInflation_`, `inflationindex.hpp:183`).
///
/// [`new`](ZeroInflationIndex::new) leaves that curve [`Handle`] empty, as the
/// C++ constructor's defaulted argument does, and
/// [`with_term_structure`](ZeroInflationIndex::with_term_structure) links one
/// in: the split keeps the concrete indexes (`UkRpi`, `UkHicp`, `EuHicp`) free
/// of a curve they do not choose. An index on an empty handle answers every
/// forecast with the dereference error, which is exactly what C++ raises for
/// the defaulted handle.
///
/// [`clone_linked_to`](ZeroInflationIndex::clone_linked_to) is the relink-a-copy
/// case the same split leaves open, for a caller that already holds an index and
/// needs the same one reading a different curve.
///
/// [`frequency`]: InflationIndex::frequency
pub struct ZeroInflationIndex {
    base: InflationIndexBase,
    term_structure: Handle<dyn ZeroInflationTermStructure>,
}

impl ZeroInflationIndex {
    /// Builds a zero inflation index on an empty curve handle
    /// (`inflationindex.cpp:158-168` with the defaulted `zeroInflation`).
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
            term_structure: Handle::empty(),
        }
    }

    /// Links the index to the inflation curve it forecasts off.
    ///
    /// The curve half of the C++ constructor: it stores the handle and
    /// registers the index with it (`registerWith(zeroInflation_)`,
    /// `inflationindex.cpp:167`), so relinking the handle - or a change in the
    /// curve it points at - re-broadcasts through
    /// [`observable`](Index::observable).
    pub fn with_term_structure(
        self,
        term_structure: Handle<dyn ZeroInflationTermStructure>,
    ) -> ZeroInflationIndex {
        term_structure.register_observer(&self.base.observer());
        ZeroInflationIndex {
            base: self.base,
            term_structure,
        }
    }

    /// The same index reading a different inflation curve
    /// (`ZeroInflationIndex::clone`, `inflationindex.cpp:245-249`).
    ///
    /// C++ rebuilds from the six identity fields and the new handle, and so does
    /// this: the copy is a peer of the original, not a view of it, and the two
    /// forecast independently off their own curves. A bootstrap helper builds one
    /// against its own relinkable handle so it can point the index at the curve
    /// being solved without touching the index the caller passed in.
    ///
    /// The copy sees the original's fixings. C++ gets that from the global
    /// `IndexManager`, keyed by index name; here it follows from the two sharing
    /// the [`Settings`] the fixing store lives on (D11) under a name recomposed
    /// from the same family and region.
    ///
    /// The copy observes its new handle, exactly as
    /// [`with_term_structure`](ZeroInflationIndex::with_term_structure) leaves it.
    /// A helper that links that handle to the curve it is bootstrapping therefore
    /// unregisters the copy again (`inflationhelpers.cpp:106-110`).
    pub fn clone_linked_to(
        &self,
        term_structure: Handle<dyn ZeroInflationTermStructure>,
    ) -> ZeroInflationIndex {
        ZeroInflationIndex::new(
            self.base.family_name.clone(),
            self.base.region.clone(),
            self.base.revised,
            self.base.frequency,
            self.base.availability_lag,
            self.base.currency.clone(),
            Shared::clone(self.base.settings()),
        )
        .with_term_structure(term_structure)
    }

    /// The inflation curve the index forecasts off
    /// (`zeroInflationTermStructure`, `inflationindex.hpp:178`), empty until
    /// [`with_term_structure`](ZeroInflationIndex::with_term_structure) links
    /// one.
    pub fn term_structure(&self) -> &Handle<dyn ZeroInflationTermStructure> {
        &self.term_structure
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

    /// The forecast fixing off the inflation curve
    /// (`inflationindex.cpp:223-241`).
    ///
    /// The curve prices *relative to the fixing at its base date*, so the
    /// forecast is that base figure compounded by the zero-coupon inflation
    /// rate of the fixing's own period over the inflation time separating the
    /// two: `baseFixing * (1 + Z1)^t1`. The base figure must therefore be on
    /// record, which is why a base date past the publication horizon is an
    /// error rather than a nested forecast.
    ///
    /// Both the rate and the time are taken at the *first day* of the fixing's
    /// inflation period, a zero index's figure being constant across it.
    ///
    /// C++ guards the power against a rate at or below -100 %, which
    /// bootstrapping can reach transiently while extrapolating, by returning
    /// zero; the guard is kept after `t1` is computed, as it is there, so a
    /// curve without a day counter fails the same way in both ports.
    ///
    /// # Errors
    ///
    /// An index with no curve linked fails here, with the message an empty
    /// [`Handle`] gives on dereference.
    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        let curve = self.term_structure.current_link()?;
        let base_date = curve.base_date();
        crate::require!(
            !self.needs_forecast(base_date)?,
            "{} index fixing at base date {base_date} is not available",
            self.base.name()
        );
        let base_fixing = self.fixing(base_date, false)?;

        let (first_date_in_period, _) = inflation_period(fixing_date, self.frequency())?;
        let z1 = curve.zero_rate_date(first_date_in_period, false)?;
        let t1 = inflation_year_fraction(
            self.frequency(),
            false,
            &curve.require_day_counter()?,
            base_date,
            first_date_in_period,
        )?;
        if z1 <= -1.0 {
            return Ok(0.0);
        }
        Ok(base_fixing * (1.0 + z1).powf(t1))
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

/// A year-on-year inflation index (`YoYInflationIndex`,
/// `inflationindex.hpp:192`).
///
/// It answers the inflation *rate* over the year ending in the period asked
/// for, where a [`ZeroInflationIndex`] answers the price *level*. It comes in
/// two shapes, and the shape it was built in decides where every past figure
/// comes from:
///
/// - a **ratio** index ([`from_underlying`](YoYInflationIndex::from_underlying))
///   stores nothing of its own and divides two fixings of the zero index it
///   wraps, a year apart (`inflationindex.cpp:331-341`);
/// - a **quoted** index ([`new`](YoYInflationIndex::new)) is published as a rate
///   in its own right - on Bloomberg, say - and reads its own fixing history
///   (`:343-369`).
///
/// Either way a period too recent to have been published is forecast off the
/// year-on-year curve the index holds, which
/// [`new`](YoYInflationIndex::new) and
/// [`from_underlying`](YoYInflationIndex::from_underlying) leave empty and
/// [`with_term_structure`](YoYInflationIndex::with_term_structure) links, on the
/// split [`ZeroInflationIndex`] uses and for the same reason.
pub struct YoYInflationIndex {
    base: InflationIndexBase,
    term_structure: Handle<dyn YoYInflationTermStructure>,
    ratio: bool,
    underlying: Option<Shared<ZeroInflationIndex>>,
}

impl YoYInflationIndex {
    /// Builds a **quoted** year-on-year index on an empty curve handle
    /// (`inflationindex.cpp:264-274` with the defaulted `yoyInflation`).
    ///
    /// The metadata is given explicitly, and the index keeps its own history:
    /// the published year-on-year rates have to be recorded on it through
    /// [`add_fixing`](Index::add_fixing).
    pub fn new(
        family_name: String,
        region: Region,
        revised: bool,
        frequency: Frequency,
        availability_lag: Period,
        currency: Currency,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        YoYInflationIndex {
            base: InflationIndexBase::new(
                family_name,
                region,
                revised,
                frequency,
                availability_lag,
                currency,
                settings,
            ),
            term_structure: Handle::empty(),
            ratio: false,
            underlying: None,
        }
    }

    /// Builds a **ratio** year-on-year index over `underlying`
    /// (`inflationindex.cpp:253-262` with the defaulted `yoyInflation`).
    ///
    /// The metadata is inherited wholesale from the zero index, bar the family
    /// name, which is prefixed `YYR_` - so a `"UK RPI"` underlying yields
    /// `"UK YYR_RPI"`. The index stores no fixings of its own; it registers
    /// with the underlying (`registerWith(underlyingIndex_)`, `:260`) so a
    /// figure published there re-broadcasts through
    /// [`observable`](Index::observable).
    pub fn from_underlying(underlying: Shared<ZeroInflationIndex>) -> Self {
        let base = InflationIndexBase::new(
            format!("YYR_{}", underlying.family_name()),
            underlying.region().clone(),
            underlying.revised(),
            underlying.frequency(),
            underlying.availability_lag(),
            underlying.currency().clone(),
            Shared::clone(underlying.inflation_base().settings()),
        );
        underlying.observable().register_observer(&base.observer());
        YoYInflationIndex {
            base,
            term_structure: Handle::empty(),
            ratio: true,
            underlying: Some(underlying),
        }
    }

    /// Links the index to the year-on-year curve it forecasts off, registering
    /// with it (`registerWith(yoyInflation_)`, `inflationindex.cpp:261,273`).
    pub fn with_term_structure(
        self,
        term_structure: Handle<dyn YoYInflationTermStructure>,
    ) -> YoYInflationIndex {
        term_structure.register_observer(&self.base.observer());
        YoYInflationIndex {
            term_structure,
            ..self
        }
    }

    /// Whether this index is the ratio of two [`ZeroInflationIndex`] fixings
    /// rather than a quoted rate (`inflationindex.hpp:241`).
    pub fn ratio(&self) -> bool {
        self.ratio
    }

    /// The zero index a ratio index divides, `None` on a quoted one
    /// (`inflationindex.hpp:242`).
    pub fn underlying_index(&self) -> Option<&Shared<ZeroInflationIndex>> {
        self.underlying.as_ref()
    }

    /// The year-on-year curve the index forecasts off
    /// (`inflationindex.hpp:243`), empty until
    /// [`with_term_structure`](YoYInflationIndex::with_term_structure) links
    /// one.
    pub fn yoy_inflation_term_structure(&self) -> &Handle<dyn YoYInflationTermStructure> {
        &self.term_structure
    }

    /// The first day of the inflation period the latest figure on record
    /// describes (`inflationindex.cpp:287-296`).
    ///
    /// A ratio index owns no history, so it answers with the underlying's; a
    /// quoted one reads its own store, whose last date is the *end* of the
    /// published period, and attributes the figure to that period's first day.
    pub fn last_fixing_date(&self) -> QlResult<Date> {
        if let Some(underlying) = &self.underlying {
            return underlying.last_fixing_date();
        }
        let last = match self.base.settings().last_fixing_date(&self.base.name()) {
            Some(date) => date,
            None => crate::fail!("no fixings stored for {}", self.base.name()),
        };
        Ok(inflation_period(last, self.frequency())?.0)
    }

    /// Whether `fixing_date` has to be forecast rather than read from history
    /// (`inflationindex.cpp:298-329`).
    ///
    /// The figure needed is the one at the first day of the fixing's own
    /// period, the index not interpolating (see the module divergences). A
    /// ratio index defers the question to the underlying, which is where the
    /// figures actually live; a quoted one runs the same three-way decision
    /// against the publication horizon as
    /// [`ZeroInflationIndex::needs_forecast`], including its second branch
    /// answering before the store is consulted.
    pub fn needs_forecast(&self, fixing_date: Date) -> QlResult<bool> {
        let frequency = self.frequency();
        let latest_needed_date = inflation_period(fixing_date, frequency)?.0;

        if let Some(underlying) = &self.underlying {
            return underlying.needs_forecast(latest_needed_date);
        }

        let today = match self.base.settings().evaluation_date() {
            Some(today) => today,
            None => crate::fail!("no evaluation date set: an index fixing needs a reference date"),
        };
        let latest_possible = inflation_period(today - self.availability_lag(), frequency)?;

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

    /// The forecast rate off the year-on-year curve
    /// (`inflationindex.cpp:372-387`).
    ///
    /// The date handed to the curve is the *first day* of the fixing's own
    /// period, derived before the curve is reached exactly as C++ derives it:
    /// [`yoy_rate_date`](YoYInflationTermStructure::yoy_rate_date) folds any
    /// seasonality in at the date it receives rather than at the period start,
    /// so passing the raw fixing date would agree for monthly factor sets and
    /// diverge for finer ones.
    ///
    /// # Errors
    ///
    /// An index with no curve linked fails here, with the message an empty
    /// [`Handle`] gives on dereference.
    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        let (first_date_in_period, _) = inflation_period(fixing_date, self.frequency())?;
        self.term_structure
            .current_link()?
            .yoy_rate_date(first_date_in_period, false)
    }

    /// The same index reading a different year-on-year curve
    /// (`YoYInflationIndex::clone`, `inflationindex.cpp:389-398`).
    ///
    /// C++ rebuilds through whichever constructor built the original, and so
    /// does this: a ratio copy wraps the same underlying, a quoted one
    /// recomposes from the six identity fields and so keys - and sees - the
    /// same fixing history.
    pub fn clone_linked_to(
        &self,
        term_structure: Handle<dyn YoYInflationTermStructure>,
    ) -> YoYInflationIndex {
        let copy = match &self.underlying {
            Some(underlying) => YoYInflationIndex::from_underlying(Shared::clone(underlying)),
            None => YoYInflationIndex::new(
                self.base.family_name.clone(),
                self.base.region.clone(),
                self.base.revised,
                self.base.frequency,
                self.base.availability_lag,
                self.base.currency.clone(),
                Shared::clone(self.base.settings()),
            ),
        };
        copy.with_term_structure(term_structure)
    }
}

impl Index for YoYInflationIndex {
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

    /// The rate at `fixing_date`, past or forecast
    /// (`inflationindex.cpp:277-285`).
    ///
    /// `forecast_todays_fixing` is ignored, as the C++ warning at
    /// `inflationindex.hpp:222-224` documents.
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

    /// The past rate of the period `fixing_date` falls in
    /// (`inflationindex.cpp:331-370`).
    ///
    /// A ratio index divides the underlying's figure for that period by its
    /// figure a year earlier, both read flat off their own periods - the zero
    /// lag being what makes [`Cpi::lagged_fixing`] land on the period the
    /// fixing date is already in. A quoted index reads the figure filed on its
    /// own period's first day, and answers `None` when there is none, which
    /// [`fixing`](Index::fixing) turns into C++'s missing-fixing error.
    fn past_fixing(&self, fixing_date: Date) -> QlResult<Option<Rate>> {
        let underlying = match &self.underlying {
            Some(underlying) => underlying,
            None => {
                let (first, _) = inflation_period(fixing_date, self.frequency())?;
                return Ok(self.base.settings().fixing(&self.base.name(), first));
            }
        };

        let no_lag = Period::new(0, TimeUnit::Months);
        let interpolation = CpiInterpolationType::Flat;
        let past = Cpi::lagged_fixing(underlying, fixing_date, no_lag, interpolation)?;
        let previous = Cpi::lagged_fixing(
            underlying,
            fixing_date - Period::new(1, TimeUnit::Years),
            no_lag,
            interpolation,
        )?;
        Ok(Some(past / previous - 1.0))
    }

    /// Delegated to [`InflationIndexBase::add_fixing`], which spreads the
    /// figure over its whole inflation period.
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

impl InflationIndex for YoYInflationIndex {
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
    use crate::handle::RelinkableHandle;
    use crate::math::interpolations::linear::Linear;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::TermStructure;
    use crate::termstructures::inflation::inflationtermstructure::InflationTermStructure;
    use crate::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;
    use crate::time::date::Month::{
        April, December, February, January, March, November, October, September,
    };
    use crate::time::daycounters::actual360::Actual360;
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

    /// The non-interpolated branch quantizes **both** ends
    /// (`inflationtermstructure.cpp:302-306`). Between two mid-month dates it
    /// therefore counts 1 February to 1 April, where the interpolated branch
    /// counts 14 February to 20 April; passing `d1` through unquantized would
    /// land on a third number again.
    #[test]
    fn a_non_interpolated_year_fraction_counts_between_period_starts() {
        let day_counter = Actual360::new();
        let (d1, d2) = (Date::new(14, February, 2007), Date::new(20, April, 2007));

        let interpolated =
            inflation_year_fraction(Frequency::Monthly, true, &day_counter, d1, d2).unwrap();
        let flat =
            inflation_year_fraction(Frequency::Monthly, false, &day_counter, d1, d2).unwrap();

        assert_eq!(interpolated, day_counter.year_fraction(d1, d2));
        assert_eq!(interpolated, 65.0 / 360.0);
        assert_eq!(flat, 59.0 / 360.0);
        assert_ne!(
            flat,
            day_counter.year_fraction(d1, Date::new(1, April, 2007))
        );
    }

    /// The quantization follows the frequency, not the calendar month, and a
    /// frequency the periods are not defined for is an error.
    #[test]
    fn a_year_fraction_quantizes_to_the_frequency() {
        let day_counter = Actual360::new();
        let (d1, d2) = (Date::new(14, February, 2007), Date::new(20, April, 2007));

        let quarterly =
            inflation_year_fraction(Frequency::Quarterly, false, &day_counter, d1, d2).unwrap();
        assert_eq!(quarterly, 90.0 / 360.0);

        assert!(inflation_year_fraction(Frequency::Weekly, false, &day_counter, d1, d2).is_err());
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

    /// The forecast fixture, shaped like `testInterpolatedZeroTermStructure`
    /// (`inflation.cpp:398-427`) but built directly rather than bootstrapped:
    /// it is 15 January 2022, the curve's base date is 1 November 2021 and the
    /// index publishes monthly one month in arrears, so every period from
    /// January 2022 on is beyond the publication horizon and must be forecast.
    const BASE_FIXING: Rate = 100.0;

    fn today() -> Date {
        Date::new(15, January, 2022)
    }

    fn curve_base_date() -> Date {
        Date::new(1, November, 2021)
    }

    fn a_curve(base_date: Date, rates: Vec<Rate>) -> Shared<ZeroInflationCurve> {
        shared(
            ZeroInflationCurve::new(
                today(),
                vec![
                    base_date,
                    Date::new(1, January, 2023),
                    Date::new(1, January, 2025),
                ],
                rates,
                Frequency::Monthly,
                Actual360::new(),
                Linear,
                None,
            )
            .expect("a well-formed zero inflation curve"),
        )
    }

    fn an_index_on(curve: &Shared<ZeroInflationCurve>) -> ZeroInflationIndex {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        a_zero_index(Frequency::Monthly, settings)
            .with_term_structure(Handle::new(
                Shared::clone(curve) as Shared<dyn ZeroInflationTermStructure>
            ))
    }

    /// The oracle: a forecast is the base-date figure compounded by the
    /// curve's zero rate over the inflation time between the two
    /// (`inflationindex.cpp:223-241`).
    ///
    /// The pin is computed **by hand** here - the zero rate by interpolating
    /// the two bracketing nodes, the exponent by day-counting between the two
    /// period starts - so it does not share
    /// [`ZeroInflationTermStructure::zero_rate_date`]'s or
    /// [`inflation_year_fraction`]'s arithmetic with the code under test.
    ///
    /// The pinned date is mid-period and genuinely on the forecast path: it
    /// needs a forecast, and its curve time is positive, so the figure comes
    /// off the curve rather than out of the history.
    #[test]
    fn a_forecast_compounds_the_base_fixing_by_the_curve_zero_rate() {
        let curve = a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]);
        let index = an_index_on(&curve);
        index
            .add_fixing(curve_base_date(), BASE_FIXING)
            .expect("seeding the base-date period");

        let mid_march = Date::new(15, March, 2022);
        let period_start = Date::new(1, March, 2022);
        assert!(index.needs_forecast(mid_march).expect("a monthly index"));
        let t = curve.time_from_reference(period_start).unwrap();
        assert_eq!(t, 45.0 / 360.0);
        assert!(t > 0.0);

        let (t_lo, t_hi) = (curve.times()[0], curve.times()[1]);
        let (r_lo, r_hi) = (curve.rates()[0], curve.rates()[1]);
        let z1 = r_lo + (t - t_lo) / (t_hi - t_lo) * (r_hi - r_lo);
        let t1 = (period_start - curve_base_date()) as Time / 360.0;
        assert_eq!(t1, 120.0 / 360.0);
        let expected = BASE_FIXING * (1.0 + z1).powf(t1);

        let forecast = index
            .fixing(mid_march, false)
            .expect("March 2022 is forecast");
        assert!(
            (forecast - expected).abs() < 1.0e-10,
            "forecast was {forecast}"
        );

        let unquantized = curve.time_from_reference(mid_march).unwrap();
        let z_unquantized = r_lo + (unquantized - t_lo) / (t_hi - t_lo) * (r_hi - r_lo);
        assert!((z1 - z_unquantized).abs() > 1.0e-4);
    }

    /// A zero index's figure is constant across its period, so every day of
    /// March forecasts March's number.
    #[test]
    fn every_date_in_a_period_forecasts_the_same_figure() {
        let curve = a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]);
        let index = an_index_on(&curve);
        index
            .add_fixing(curve_base_date(), BASE_FIXING)
            .expect("seeding the base-date period");

        let first = index.fixing(Date::new(1, March, 2022), false).unwrap();
        assert_eq!(
            index.fixing(Date::new(15, March, 2022), false).unwrap(),
            first
        );
        assert_eq!(
            index.fixing(Date::new(31, March, 2022), false).unwrap(),
            first
        );
        assert_ne!(
            index.fixing(Date::new(1, April, 2022), false).unwrap(),
            first
        );
    }

    /// The base figure has to be on record: a curve whose base date is itself
    /// past the publication horizon would need a forecast to produce one, and
    /// C++ stops rather than recurse (`inflationindex.cpp:225-227`).
    #[test]
    fn a_forecast_needs_the_fixing_at_the_curve_base_date() {
        let curve = a_curve(Date::new(1, January, 2022), vec![0.02, 0.05, 0.06]);
        let index = an_index_on(&curve);
        assert!(index.needs_forecast(curve.base_date()).unwrap());

        let error = index
            .fixing(Date::new(15, March, 2022), false)
            .expect_err("the base-date figure cannot have been published");
        assert!(
            error.to_string().contains("index fixing at base date"),
            "err was: {error}"
        );
    }

    /// The `pow` guard (`inflationindex.cpp:236-239`): a zero rate at or below
    /// -100 %, which only extrapolation during a bootstrap reaches, forecasts
    /// zero instead of raising a negative base to a fractional power. The base
    /// node is the one the curve leaves unconstrained, so the segment running
    /// off it can dip below -1.
    #[test]
    fn a_zero_rate_at_or_below_minus_one_forecasts_zero() {
        let curve = a_curve(curve_base_date(), vec![-1.5, -0.9, -0.8]);
        let index = an_index_on(&curve);
        index
            .add_fixing(curve_base_date(), BASE_FIXING)
            .expect("seeding the base-date period");

        let march = Date::new(15, March, 2022);
        assert!(curve.zero_rate_date(march, false).unwrap() <= -1.0);
        assert_eq!(index.fixing(march, false).unwrap(), 0.0);
    }

    /// `registerWith(zeroInflation_)` (`inflationindex.cpp:167`), asserted
    /// structurally: the index computes every fixing on demand and caches
    /// nothing, so a relink changes no value and only the notification
    /// discriminates a registered index from an unregistered one.
    #[test]
    fn the_index_re_broadcasts_a_curve_relink() {
        let curve = a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]);
        let handle: RelinkableHandle<dyn ZeroInflationTermStructure> = RelinkableHandle::empty();
        let settings = shared(Settings::<Date>::new());
        let index = a_zero_index(Frequency::Monthly, settings).with_term_structure(handle.handle());

        let flag = shared_mut(Flag::default());
        index
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        handle.link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);
        assert!(flag.borrow().up);
    }

    /// The copy carries the six identity fields across
    /// (`inflationindex.cpp:245-249`), so it keys the same fixing store and
    /// answers the same publication questions.
    #[test]
    fn a_clone_keeps_the_indexs_identity() {
        let curve = a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]);
        let index = an_index_on(&curve);
        let copy = index.clone_linked_to(Handle::empty());

        assert_eq!(copy.name(), index.name());
        assert_eq!(copy.family_name(), index.family_name());
        assert_eq!(copy.region(), index.region());
        assert_eq!(copy.revised(), index.revised());
        assert_eq!(copy.frequency(), index.frequency());
        assert_eq!(copy.availability_lag(), index.availability_lag());
        assert_eq!(copy.currency().code(), index.currency().code());
    }

    /// The two share one fixing history, which is what makes a helper's copy
    /// usable: it prices off figures the caller published on the original. C++
    /// routes both through the global `IndexManager`; here both hold the same
    /// [`Settings`], so the store and the evaluation date are shared too.
    ///
    /// The write is made *after* the copy is taken, so a port that snapshotted
    /// the history at clone time would fail here.
    #[test]
    fn a_clone_shares_the_originals_fixings() {
        let curve = a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]);
        let index = an_index_on(&curve);
        let copy = index.clone_linked_to(Handle::empty());

        index
            .add_fixing(curve_base_date(), BASE_FIXING)
            .expect("seeding the base-date period");

        assert_eq!(
            copy.past_fixing(Date::new(20, November, 2021))
                .expect("every date is a valid fixing date"),
            Some(BASE_FIXING)
        );
        assert_eq!(
            copy.last_fixing_date().expect("the shared history"),
            curve_base_date()
        );

        copy.add_fixing(Date::new(1, December, 2021), 101.0)
            .expect("publishing through the copy");
        assert_eq!(
            index
                .past_fixing(Date::new(15, December, 2021))
                .expect("every date is a valid fixing date"),
            Some(101.0)
        );
    }

    /// The point of the copy: it forecasts off the curve it was handed, not off
    /// the original's. Both compound the same shared base figure over the same
    /// inflation time, so only the curve's rate separates the two answers - here
    /// 5 % against 2 % at the March 2022 node.
    #[test]
    fn a_clone_forecasts_off_its_own_curve() {
        let index = an_index_on(&a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]));
        index
            .add_fixing(curve_base_date(), BASE_FIXING)
            .expect("seeding the base-date period");

        let slower = a_curve(curve_base_date(), vec![0.01, 0.02, 0.03]);
        let copy = index.clone_linked_to(Handle::new(
            slower as Shared<dyn ZeroInflationTermStructure>,
        ));

        let march = Date::new(15, March, 2022);
        let original = index.fixing(march, false).expect("March 2022 is forecast");
        let cloned = copy.fixing(march, false).expect("March 2022 is forecast");

        assert!(original > BASE_FIXING);
        assert!(cloned > BASE_FIXING);
        assert!(
            original - cloned > 0.5,
            "the two curves gave {original} and {cloned}"
        );
    }

    /// The copy registers with its own handle, as the constructor does
    /// (`inflationindex.cpp:167`), and the original keeps observing only its
    /// own: relinking one notifies one index and not the other.
    #[test]
    fn a_clone_observes_only_its_own_handle() {
        let curve = a_curve(curve_base_date(), vec![0.02, 0.05, 0.06]);
        let handle: RelinkableHandle<dyn ZeroInflationTermStructure> = RelinkableHandle::empty();
        let settings = shared(Settings::<Date>::new());
        let index = a_zero_index(Frequency::Monthly, settings);
        let copy = index.clone_linked_to(handle.handle());

        let on_original = shared_mut(Flag::default());
        index
            .observable()
            .register_observer(&(on_original.clone() as SharedMut<dyn Observer>));
        let on_copy = shared_mut(Flag::default());
        copy.observable()
            .register_observer(&(on_copy.clone() as SharedMut<dyn Observer>));

        handle.link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);

        assert!(on_copy.borrow().up, "the copy observes the handle it took");
        assert!(!on_original.borrow().up, "the original is a separate index");
    }
}
