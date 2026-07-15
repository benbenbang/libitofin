//! The Inter-Bank-Offered-Rate index.
//!
//! Port of `ql/indexes/iborindex.{hpp,cpp}`. [`IborIndex`] is the concrete
//! [`InterestRateIndex`] behind the floating leg: an
//! [`InterestRateIndexBase`] plus a business-day convention, an end-of-month
//! flag, and a [`Handle`] to the forwarding [`YieldTermStructure`] it reads
//! fixings off.
//!
//! It supplies the two members the base leaves abstract:
//! [`maturity_date`](InterestRateIndex::maturity_date) advances the value date
//! by the tenor under the index convention, and
//! [`forecast_fixing`](InterestRateIndex::forecast_fixing) derives the simple
//! forward rate from the curve's discount factors between the value and
//! maturity dates. The index registers its forwarding observer with the curve
//! handle, so a relinked or changed curve notifies the index's observers.
//!
//! [`OvernightIndex`] lives here too, as it does in `iborindex.hpp`: an
//! [`IborIndex`] pinned to the overnight configuration (one-day tenor,
//! `Following`, no end-of-month).

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::{InterestRateIndex, InterestRateIndexBase};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Time};
use crate::{currency::Currency, require};

/// A concrete Inter-Bank-Offered-Rate index (e.g. Libor, Euribor).
///
/// Wraps an [`InterestRateIndexBase`] with the forwarding curve and the
/// convention the maturity calculation needs. Built with a possibly empty curve
/// handle, exactly as the C++ default `Handle<YieldTermStructure> h = {}`
/// allows; a fixing forecast on an empty handle is an error, not a panic (D4).
pub struct IborIndex {
    base: InterestRateIndexBase,
    convention: BusinessDayConvention,
    end_of_month: bool,
    term_structure: Handle<dyn YieldTermStructure>,
}

impl IborIndex {
    /// Builds an index over `forwarding`, registering with the curve handle.
    ///
    /// Mirrors the C++ constructor: it composes the base (which normalizes the
    /// tenor and wires evaluation-date and fixing-history observation), stores
    /// the convention and end-of-month flag, and registers the index's
    /// forwarding observer with the curve handle so a relink notifies observers.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family_name: String,
        tenor: Period,
        settlement_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        day_counter: DayCounter,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        let base = InterestRateIndexBase::new(
            family_name,
            tenor,
            settlement_days,
            currency,
            fixing_calendar,
            day_counter,
            settings,
        );
        forwarding.register_observer(&base.observer());
        IborIndex {
            base,
            convention,
            end_of_month,
            term_structure: forwarding,
        }
    }

    /// The convention applied when rolling the value date to maturity.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.convention
    }

    /// Whether the maturity roll keeps to month ends.
    pub fn end_of_month(&self) -> bool {
        self.end_of_month
    }

    /// The curve used to forecast fixings (`forwardingTermStructure`).
    pub fn forwarding_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.term_structure
    }

    /// Rebuilds this index onto a different forwarding curve, copying every
    /// other configuration field verbatim (`clone`, `iborindex.cpp:85-93`).
    ///
    /// A `DepositRateHelper` clones its index onto its own relinkable handle so
    /// it prices off the curve being bootstrapped rather than the curve the
    /// index was handed (`ratehelpers.cpp:206`). The clone keeps the same family
    /// name and tenor, hence the same D11 fixing-store key, and reuses the same
    /// [`Settings`], so it shares the original's fixing history (keyed on name)
    /// and evaluation date.
    pub fn clone_with(&self, forwarding: Handle<dyn YieldTermStructure>) -> IborIndex {
        IborIndex::new(
            self.family_name().to_string(),
            self.tenor(),
            self.fixing_days(),
            self.currency().clone(),
            self.fixing_calendar(),
            self.convention,
            self.end_of_month,
            self.day_counter().clone(),
            forwarding,
            self.base.settings().clone(),
        )
    }

    /// The simple forward rate over `[d1, d2]` with year fraction `t`, read off
    /// the forwarding curve (the C++ `forecastFixing(d1, d2, t)`).
    ///
    /// C++ keeps this overload private and declares `IborCoupon` a `friend`
    /// (`iborindex.hpp:81`), so only the coupon may pass cached dates and no
    /// arbitrary caller can ask a 6-month index for a 1-year fixing. The port
    /// mirrors that trust boundary with crate visibility: reachable by
    /// [`IborCoupon`](crate::cashflows::iborcoupon::IborCoupon)'s par/indexed
    /// forecast, closed to the public API.
    pub(crate) fn forecast_fixing_between(&self, d1: Date, d2: Date, t: Time) -> QlResult<Rate> {
        require!(
            !self.term_structure.is_empty(),
            "null term structure set to this instance of {}",
            self.name()
        );
        let curve = self.term_structure.current_link()?;
        let disc1 = curve.discount_date(d1, false)?;
        let disc2 = curve.discount_date(d2, false)?;
        Ok((disc1 / disc2 - 1.0) / t)
    }
}

impl InterestRateIndex for IborIndex {
    fn base(&self) -> &InterestRateIndexBase {
        &self.base
    }

    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        Ok(self.fixing_calendar().advance_by_period(
            value_date,
            self.tenor(),
            self.convention,
            self.end_of_month,
        ))
    }

    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        let d1 = self.value_date(fixing_date)?;
        let d2 = self.maturity_date(d1)?;
        let t = self.day_counter().year_fraction(d1, d2);
        let positive_time = t > 0.0;
        require!(
            positive_time,
            "cannot calculate forward rate between {d1:?} and {d2:?}: non positive time ({t}) using {} daycounter",
            self.day_counter().name()
        );
        self.forecast_fixing_between(d1, d2, t)
    }
}

/// An overnight index (e.g. SOFR, ESTR): an [`IborIndex`] fixed on a one-day
/// tenor.
///
/// Port of `OvernightIndex` (`ql/indexes/iborindex.hpp:88`), which subclasses
/// [`IborIndex`] and forwards a fixed overnight configuration: a `1*Days`
/// tenor, a [`Following`](BusinessDayConvention::Following) roll, and no
/// end-of-month adjustment. The port keeps that "is-an-`IborIndex`" relation as
/// a newtype embedding the configured [`IborIndex`], re-exposed through
/// [`InterestRateIndex`] (and hence the whole [`Index`] surface) so a
/// downstream overnight coupon can hold a concrete `OvernightIndex` rather than
/// an indistinguishable [`IborIndex`].
///
/// The inner [`IborIndex`] is held behind a [`Shared`] so its identity can be
/// re-exposed through [`ibor_index`](Self::ibor_index). C++ upcasts a single
/// `shared_ptr<OvernightIndex>` into a `shared_ptr<IborIndex>` wherever an
/// overnight index is used as its base (an OIS handing itself to
/// `FixedVsFloatingSwap`, say); the owned newtype cannot project an [`Rc`], so
/// wrapping the inner index restores that single-identity relation.
///
/// The C++ `clone()` override (`iborindex.cpp:85`) re-curves the index onto a
/// different forwarding handle. [`IborIndex`] ports it as
/// [`clone_with`](IborIndex::clone_with) for rate-helper bootstrapping;
/// `OvernightIndex` does not override it, as no overnight helper needs the
/// overnight-typed clone yet.
pub struct OvernightIndex(Shared<IborIndex>);

impl OvernightIndex {
    /// Builds an overnight index over `forwarding`.
    ///
    /// Mirrors the C++ constructor (`iborindex.cpp:76`): an [`IborIndex`] with a
    /// `1*Days` tenor, a [`Following`](BusinessDayConvention::Following) roll,
    /// and `end_of_month = false`, leaving family name, settlement days,
    /// currency, fixing calendar, and day counter to the caller.
    pub fn new(
        family_name: String,
        settlement_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        day_counter: DayCounter,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> OvernightIndex {
        OvernightIndex(shared(IborIndex::new(
            family_name,
            Period::new(1, TimeUnit::Days),
            settlement_days,
            currency,
            fixing_calendar,
            BusinessDayConvention::Following,
            false,
            day_counter,
            forwarding,
            settings,
        )))
    }

    /// The inner [`IborIndex`] this overnight index is a `1*Days` configuration
    /// of, sharing its identity (the C++ upcast of the same `shared_ptr`).
    pub fn ibor_index(&self) -> Shared<IborIndex> {
        self.0.clone()
    }

    /// Re-curves the overnight index onto a different forwarding handle,
    /// preserving its configuration (the C++ `clone(h)` override that
    /// `OISRateHelper::initialize` calls, `oisratehelper.cpp:114`). The result
    /// stays overnight-typed, so it can be handed back to [`MakeOis`], mirroring
    /// C++'s `dynamic_pointer_cast<OvernightIndex>(clone(...))`.
    ///
    /// [`MakeOis`]: crate::instruments::MakeOis
    pub fn clone_with(&self, forwarding: Handle<dyn YieldTermStructure>) -> Shared<OvernightIndex> {
        shared(OvernightIndex(shared(self.0.clone_with(forwarding))))
    }

    /// The convention applied when rolling the value date to maturity (always
    /// [`Following`](BusinessDayConvention::Following)).
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.0.business_day_convention()
    }

    /// Whether the maturity roll keeps to month ends (always `false`).
    pub fn end_of_month(&self) -> bool {
        self.0.end_of_month()
    }

    /// The curve used to forecast fixings (`forwardingTermStructure`).
    pub fn forwarding_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        self.0.forwarding_term_structure()
    }
}

impl InterestRateIndex for OvernightIndex {
    fn base(&self) -> &InterestRateIndexBase {
        self.0.base()
    }

    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        self.0.maturity_date(value_date)
    }

    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        self.0.forecast_fixing(fixing_date)
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for the plain single-calendar `IborIndex`.
    //!
    //! `indexes.cpp testCustomIborIndex` (:152) exercises `CustomIborIndex`, a
    //! three-calendar variant (separate fixing/value/maturity calendars) that is
    //! not ported here; its hard-coded bespoke-holiday date assertions (e.g.
    //! `valueDate(7 Jan 2025) == 9 Jan 2025`) apply when that subclass lands.
    //! Plain `IborIndex` has one calendar, so these tests cover only the
    //! single-calendar subset.

    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::interestrate::Compounding;
    use crate::patterns::observable::Observer;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendars::target::Target;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    /// A `foo` index on TARGET, Actual/360, two settlement days - the shape
    /// `indexes.cpp`'s `IborIndex("foo", ...)` cases use.
    fn ibor(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        IborIndex::new(
            "foo".into(),
            tenor,
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::Following,
            false,
            Actual360::new(),
            forwarding,
            settings,
        )
    }

    fn flat_curve(reference: Date, rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// `testTenorNormalization` (indexes.cpp:124): a 12-month index and a
    /// 1-year index normalize to the same name.
    #[test]
    fn twelve_months_and_one_year_yield_the_same_name() {
        let settings = shared(Settings::<Date>::new());
        let i12m = ibor(
            Period::new(12, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        );
        let i1y = ibor(Period::new(1, TimeUnit::Years), Handle::empty(), settings);
        assert_eq!(i12m.name(), i1y.name());
    }

    /// `testTenorNormalization` (indexes.cpp:124): the 6-day index matures
    /// before the 7-day index off the same date.
    #[test]
    fn six_day_index_matures_before_seven_day_index() {
        let settings = shared(Settings::<Date>::new());
        let i6d = ibor(
            Period::new(6, TimeUnit::Days),
            Handle::empty(),
            settings.clone(),
        );
        let i7d = ibor(Period::new(7, TimeUnit::Days), Handle::empty(), settings);
        let test_date = Date::new(28, Month::April, 2023);
        assert!(i6d.maturity_date(test_date).unwrap() < i7d.maturity_date(test_date).unwrap());
    }

    /// `testFixingHasHistoricalFixing` (indexes.cpp:83) through the index: a
    /// fixing added on one index is seen by another instance of the same name
    /// (the store keys on name), not by a differently-tenored index, and clears.
    #[test]
    fn historical_fixing_is_shared_by_name_and_cleared() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let ibor6m = ibor(
            Period::new(6, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        );
        let ibor3m = ibor(
            Period::new(3, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        );
        let ibor6m_a = ibor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);

        let mut day = today;
        while !ibor6m.is_valid_fixing_date(day) {
            day -= 1;
        }
        ibor6m.add_fixing(day, 0.01).unwrap();

        assert!(!ibor3m.has_historical_fixing(day));
        assert!(ibor6m.has_historical_fixing(day));
        assert!(ibor6m_a.has_historical_fixing(day));

        ibor6m.clear_fixings();
        assert!(!ibor6m.has_historical_fixing(day));
        assert!(!ibor6m_a.has_historical_fixing(day));
    }

    /// `forecastFixing` (iborindex.cpp:44) reads the simple forward
    /// `(disc1/disc2 - 1)/t` off the curve between the value and maturity dates.
    ///
    /// The simple-forward formula is `iborindex.hpp`'s private `forecastFixing`
    /// overload (:112-120); no C++ suite test pins it directly for a plain
    /// `IborIndex` (`testCdiIndex` :206 asserts a Brazil CDI *compounded*
    /// forecast, a different formula on a different index). So this checks two
    /// independent things: the implementation matches the discount-factor
    /// formula (a plumbing pin), and it matches the closed form of a simple
    /// forward on a continuously-compounded flat curve, `(exp(r*t) - 1)/t`,
    /// computed here without touching the curve.
    #[test]
    fn forecast_fixing_reads_the_simple_forward_off_the_curve() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let rate = 0.05;
        let curve = flat_curve(today, rate);
        let index = ibor(Period::new(6, TimeUnit::Months), curve.clone(), settings);

        let fixing_date = Date::new(15, Month::July, 2026);
        let d1 = index.value_date(fixing_date).unwrap();
        let d2 = index.maturity_date(d1).unwrap();
        let t = Actual360::new().year_fraction(d1, d2);
        let forecast = index.forecast_fixing(fixing_date).unwrap();

        let link = curve.current_link().unwrap();
        let disc1 = link.discount_date(d1, false).unwrap();
        let disc2 = link.discount_date(d2, false).unwrap();
        assert!((forecast - (disc1 / disc2 - 1.0) / t).abs() < 1e-12);

        let analytic = ((rate * t).exp() - 1.0) / t;
        assert!((forecast - analytic).abs() < 1e-12);
        assert!(forecast > 0.0);
    }

    /// The `iborindex.hpp` private-overload guard: forecasting off an empty
    /// forwarding handle is an error carrying the C++ message, not a panic.
    #[test]
    fn forecast_on_an_empty_curve_is_an_error() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = ibor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let err = index
            .forecast_fixing(Date::new(15, Month::July, 2026))
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("null term structure set to this instance of")
        );
    }

    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    /// `registerWith(termStructure_)` (iborindex.cpp:39): relinking the
    /// forwarding curve notifies the index's observers.
    #[test]
    fn relinking_the_forwarding_curve_notifies_the_index() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let index = ibor(Period::new(6, TimeUnit::Months), handle.handle(), settings);

        let flag = shared_mut(Flag { up: false });
        index
            .base()
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        handle.link_to(shared(FlatForward::with_rate(
            today,
            0.05,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);

        assert!(flag.borrow().up);
    }

    /// `clone` (`iborindex.cpp:85`) re-curves onto a new handle while copying
    /// every other field verbatim: the clone forecasts off its own curve (the
    /// original untouched), and - keyed on the same name - shares the
    /// original's fixing history.
    #[test]
    fn clone_with_recurves_and_shares_fixing_history() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let original = ibor(
            Period::new(6, TimeUnit::Months),
            flat_curve(today, 0.05),
            settings,
        );

        let cloned_handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let clone = original.clone_with(cloned_handle.handle());

        assert_eq!(clone.name(), original.name());
        assert_eq!(clone.tenor(), original.tenor());
        assert_eq!(clone.fixing_days(), original.fixing_days());
        assert_eq!(
            clone.business_day_convention(),
            original.business_day_convention()
        );
        assert_eq!(clone.end_of_month(), original.end_of_month());

        let fixing_date = Date::new(15, Month::July, 2026);
        assert!(
            clone.forecast_fixing(fixing_date).is_err(),
            "clone starts on its own empty handle"
        );
        cloned_handle.link_to(shared(FlatForward::with_rate(
            today,
            0.02,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);

        let d1 = clone.value_date(fixing_date).unwrap();
        let d2 = clone.maturity_date(d1).unwrap();
        let t = Actual360::new().year_fraction(d1, d2);
        let clone_forecast = clone.forecast_fixing(fixing_date).unwrap();
        assert!((clone_forecast - ((0.02 * t).exp() - 1.0) / t).abs() < 1e-12);
        let original_forecast = original.forecast_fixing(fixing_date).unwrap();
        assert!(
            (original_forecast - ((0.05 * t).exp() - 1.0) / t).abs() < 1e-12,
            "the original still forecasts off its own curve"
        );

        let mut day = today;
        while !original.is_valid_fixing_date(day) {
            day -= 1;
        }
        original.add_fixing(day, 0.01).unwrap();
        assert!(
            clone.has_historical_fixing(day),
            "a clone shares the original's fixing history by name"
        );
    }

    /// `OvernightIndex` (`iborindex.cpp:76`) pins the overnight configuration:
    /// a one-day tenor named `ON` at zero fixing days, a `Following` roll, and
    /// no end-of-month adjustment.
    #[test]
    fn overnight_index_carries_the_overnight_configuration() {
        let settings = shared(Settings::<Date>::new());
        let index = OvernightIndex::new(
            "SOFR".into(),
            0,
            Currency::usd(),
            UnitedStates::new(Market::Sofr),
            Actual360::new(),
            Handle::empty(),
            settings,
        );

        assert_eq!(index.tenor(), Period::new(1, TimeUnit::Days));
        assert_eq!(index.fixing_days(), 0);
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert!(!index.end_of_month());
        assert_eq!(index.name(), "SOFRON Actual/360");
        assert_eq!(index.fixing_calendar().name(), "SOFR fixing calendar");
    }

    /// The overnight index forecasts through the inherited [`IborIndex`]
    /// machinery: over a flat continuously-compounded curve its one-day forward
    /// matches the closed form `(exp(r*t) - 1)/t`.
    #[test]
    fn overnight_index_forecasts_through_the_ibor_machinery() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let rate = 0.05;
        let index = OvernightIndex::new(
            "ESTR".into(),
            0,
            Currency::eur(),
            Target::new(),
            Actual360::new(),
            flat_curve(today, rate),
            settings,
        );

        let fixing_date = Date::new(15, Month::July, 2026);
        let d1 = index.value_date(fixing_date).unwrap();
        let d2 = index.maturity_date(d1).unwrap();
        let t = Actual360::new().year_fraction(d1, d2);
        let forecast = index.forecast_fixing(fixing_date).unwrap();
        let analytic = ((rate * t).exp() - 1.0) / t;
        assert!((forecast - analytic).abs() < 1e-12);
    }
}
