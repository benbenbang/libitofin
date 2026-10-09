//! The USD Libor index.
//!
//! Port of `ql/indexes/ibor/usdlibor.hpp`. `USDLibor` is the US Dollar LIBOR
//! fixed by ICE: the [`Libor`] base configuration with family name
//! "USDLibor", two settlement days, USD, the US LiborImpact calendar as the
//! financial center, and an [`Actual360`] day counter (`usdlibor.hpp:44-50`).
//! Like the base, it is pure configuration: [`UsdLibor::new`] returns a plain
//! [`IborIndex`] whose maturity calendar is the joint UK-Exchange-plus-US
//! calendar.
//!
//! Deferred visibly: `DailyTenorUSDLibor` (needs the `DailyTenorLibor` base,
//! see the deferral note in [`libor`](crate::indexes::ibor::libor)).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::ibor::libor::Libor;
use crate::indexes::iborindex::IborIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::unitedstates::{Market as UsMarket, UnitedStates};
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// The USD Libor index (`ql/indexes/ibor/usdlibor.hpp`, C++ `USDLibor`).
///
/// A zero-sized namespace for the USDLibor constructor.
pub struct UsdLibor;

impl UsdLibor {
    /// Builds the standard six-month USD Libor index using [`Self::new`].
    ///
    /// The forwarding handle and settings retain their shared live state.
    pub fn six_months(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::new(Period::new(6, TimeUnit::Months), forwarding, settings)
            .expect("a six-month USD Libor tenor is valid")
    }

    /// Builds a USD Libor index of the given `tenor` over the `forwarding`
    /// curve.
    ///
    /// Mirrors the C++ `USDLibor::USDLibor(tenor, h)` constructor: the
    /// [`Libor`] base with family name "USDLibor", two settlement days, USD,
    /// the [`UnitedStates`] LiborImpact financial-center calendar, and an
    /// [`Actual360`] day counter. The base's guards apply: daily tenors are
    /// rejected (`DailyTenorUSDLibor` is not ported yet).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<IborIndex> {
        Libor::new(
            "USDLibor".into(),
            tenor,
            2,
            Currency::usd(),
            UnitedStates::new(UsMarket::LiborImpact),
            Actual360::new(),
            forwarding,
            settings,
        )
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for `USDLibor`: the `testSwapRateHelperSpotDate` case
    //! (`piecewiseyieldcurve.cpp:1114`) plus maturity- and value-roll pins
    //! over the actual joint calendar.
    //!
    //! The last-relevant-date regression is covered by `tests/custom_pillars.rs`.

    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::interestrate::Compounding;
    use crate::shared::shared;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yields::SwapRateHelper;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedkingdom::{Market as UkMarket, UnitedKingdom};
    use crate::time::date::Month;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;

    fn usd_libor_3m(settings: Shared<Settings<Date>>) -> IborIndex {
        UsdLibor::new(Period::new(3, TimeUnit::Months), Handle::empty(), settings)
            .expect("a 3M USDLibor tenor is valid")
    }

    /// The `usdlibor.hpp:44-50` configuration: name, fixing days, currency,
    /// and the tenor-derived convention and end-of-month flag.
    #[test]
    fn usd_libor_carries_the_ice_configuration() {
        let index = usd_libor_3m(shared(Settings::<Date>::new()));
        assert_eq!(index.name(), "USDLibor3M Actual/360");
        assert_eq!(index.fixing_days(), 2);
        assert_eq!(index.currency(), &Currency::usd());
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(index.end_of_month());
    }

    /// `testSwapRateHelperSpotDate` (`piecewiseyieldcurve.cpp:1114`): with
    /// the evaluation date on Friday 11 October 2019, the helper's swap
    /// starts 15 October 2019. Advancing two days on the US calendar would
    /// give the 16th (Monday 14 October is Columbus Day), but the LIBOR spot
    /// advances on the UK calendar, landing on the 15th - a US business day,
    /// so the joint adjust is a no-op here.
    ///
    /// The C++ assertion reads `helper->swap()->startDate()`; this port's
    /// `earliest_date` is that same date (`initialize_dates` sets it to the
    /// minimum of the two schedule start dates, both the swap's spot start).
    #[test]
    fn swap_rate_helper_spot_date_advances_on_the_uk_calendar() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(11, Month::October, 2019));
        let index = usd_libor_3m(settings);

        let helper = SwapRateHelper::from_rate(
            0.02,
            Period::new(5, TimeUnit::Years),
            UnitedStates::new(UsMarket::GovernmentBond),
            Frequency::Semiannual,
            BusinessDayConvention::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            &index,
        );

        assert_eq!(helper.earliest_date(), Date::new(15, Month::October, 2019));
    }

    /// `Libor::valueDate` (`libor.cpp:86-100`): the UK-calendar advance from
    /// Thursday 7 November 2019 lands on Monday 11 November 2019, a UK
    /// business day but Veterans Day under US LiborImpact, so the joint
    /// adjust rolls the value date to the 12th. The single-calendar trait
    /// default would keep the 11th.
    #[test]
    fn value_date_is_adjusted_on_the_joint_calendar() {
        let index = usd_libor_3m(shared(Settings::<Date>::new()));
        let value = index
            .value_date(Date::new(7, Month::November, 2019))
            .unwrap();
        assert_eq!(value, Date::new(12, Month::November, 2019));
    }

    /// `Libor::maturityDate` (`libor.cpp:102-113`) advances on the JOINT
    /// calendar, and this date discriminates joint-vs-UK-only:
    ///
    /// v = Tuesday 11 August 2020 (a business day in both calendars, not the
    /// last business day of August, so the end-of-month flag is inert).
    /// v + 3M lands on Wednesday 11 November 2020: Veterans Day, a fixed-date
    /// (d == 11, no Monday shift) US holiday observed by LiborImpact (whose
    /// only carve-out from US settlement is the post-2015 Independence-Day
    /// observed shift, `unitedstates.rs:233`), a UK Exchange business day
    /// (the UK has no November bank holiday), and not month-end (so the EOM
    /// roll cannot mask the difference). The joint calendar rolls
    /// ModifiedFollowing to Thursday the 12th; UK-only would keep the 11th.
    ///
    /// The clone round-trip pins that `clone_with` copies the maturity
    /// calendar: a `SwapRateHelper` prices off a clone (`ratehelpers.rs:834`),
    /// so a clone dropping it would silently pass the spot-date oracle.
    #[test]
    fn maturity_date_advances_on_the_joint_calendar() {
        let index = usd_libor_3m(shared(Settings::<Date>::new()));
        let v = Date::new(11, Month::August, 2020);
        let maturity = index.maturity_date(v).unwrap();

        let uk = UnitedKingdom::new(UkMarket::Exchange);
        let joint = index.maturity_calendar();
        let expected = joint.advance_by_period(
            v,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        );
        let uk_only = uk.advance_by_period(
            v,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        );

        assert_eq!(maturity, expected);
        assert_eq!(maturity, Date::new(12, Month::November, 2020));
        assert_eq!(uk_only, Date::new(11, Month::November, 2020));
        assert_ne!(maturity, uk_only);

        let clone = index.clone_with(Handle::empty());
        assert_eq!(clone.maturity_date(v).unwrap(), maturity);
    }

    /// The single-calendar regression pin for the #963 roll-rule fold: a
    /// `Libor` keeps `valueCalendar == fixingCalendar == UK Exchange`
    /// (`libor.cpp:98`), so now that `IborIndex` overrides `fixing_date` its
    /// answer must still be the plain UK roll-back.
    ///
    /// Off Friday 13 November 2020 that is Wednesday the 11th - Veterans Day,
    /// a US LiborImpact holiday and a UK business day, so a roll-back on the
    /// joint maturity calendar would give Tuesday the 10th instead.
    #[test]
    fn fixing_date_rolls_back_on_the_uk_calendar_alone() {
        let index = usd_libor_3m(shared(Settings::<Date>::new()));
        assert_eq!(
            index.fixing_date(Date::new(13, Month::November, 2020)),
            Date::new(11, Month::November, 2020)
        );
    }

    #[test]
    fn six_months_matches_generic_dates_configuration_and_forecasts() {
        let today = Date::new(1, Month::May, 2020);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let forwarding = Handle::new(shared(FlatForward::with_rate(
            today,
            0.05,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let index = UsdLibor::six_months(forwarding.clone(), settings.clone());
        let generic =
            UsdLibor::new(Period::new(6, TimeUnit::Months), forwarding, settings).unwrap();
        assert_eq!(index.name(), "USDLibor6M Actual/360");
        assert_eq!(index.name(), generic.name());
        assert_eq!(index.tenor(), generic.tenor());
        assert_eq!(index.currency(), generic.currency());
        assert_eq!(index.fixing_days(), generic.fixing_days());
        assert_eq!(index.day_counter(), generic.day_counter());
        assert_eq!(
            index.business_day_convention(),
            generic.business_day_convention()
        );
        assert_eq!(index.end_of_month(), generic.end_of_month());
        let fixing = Date::new(7, Month::May, 2020);
        let value = index.value_date(fixing).unwrap();
        let maturity = index.maturity_date(value).unwrap();
        assert_eq!(value, Date::new(12, Month::May, 2020));
        assert_eq!(maturity, Date::new(12, Month::November, 2020));
        assert_eq!(value, generic.value_date(fixing).unwrap());
        assert_eq!(maturity, generic.maturity_date(value).unwrap());
        assert_eq!(index.fixing_date(value), generic.fixing_date(value));
        assert_eq!(index.fixing_date(value), Date::new(7, Month::May, 2020));
        let accrual = Actual360::new().year_fraction(value, maturity);
        let expected = ((0.05 * accrual).exp() - 1.0) / accrual;
        assert!((index.forecast_fixing(fixing).unwrap() - expected).abs() < 1e-12);
        assert_eq!(
            index.fixing(fixing, false).unwrap(),
            generic.fixing(fixing, false).unwrap()
        );
    }

    #[test]
    fn six_months_keeps_live_forwarding_and_settings() {
        let today = Date::new(15, Month::June, 2026);
        let fixing = Date::new(15, Month::July, 2026);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let forwarding = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let index = UsdLibor::six_months(forwarding.handle(), settings.clone());
        assert!(index.forecast_fixing(fixing).is_err());
        let curve = |rate| {
            shared(FlatForward::with_rate(
                today,
                rate,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>
        };
        forwarding.link_to(curve(0.02));
        let before = index.fixing(fixing, false).unwrap();
        forwarding.link_to(curve(0.07));
        let after = index.fixing(fixing, false).unwrap();
        assert!(after > before);
        let start = index.value_date(fixing).unwrap();
        let end = index.maturity_date(start).unwrap();
        let accrual = index.day_counter().year_fraction(start, end);
        assert!((after - ((0.07 * accrual).exp() - 1.0) / accrual).abs() < 1e-12);
        settings.set_evaluation_date(fixing);
        settings.set_enforces_todays_historic_fixings(true);
        assert!(index.fixing(fixing, false).is_err());
        assert_eq!(index.fixing(fixing, true).unwrap(), after);
        settings.set_enforces_todays_historic_fixings(false);
        assert_eq!(index.fixing(fixing, false).unwrap(), after);
        settings.set_evaluation_date(Date::new(16, Month::July, 2026));
        assert!(index.fixing(fixing, false).is_err());
        settings.set_evaluation_date(today);
        assert_eq!(index.fixing(fixing, false).unwrap(), after);
    }
}
