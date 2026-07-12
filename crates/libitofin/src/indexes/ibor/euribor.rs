//! The Euribor index.
//!
//! Port of `ql/indexes/ibor/euribor.{hpp,cpp}`. [`Euribor`] is the first named
//! concrete [`IborIndex`]: the rate fixed by the ECB, fixing "Euribor" on the
//! [`Target`] calendar in [`EUR`](Currency::eur) with two settlement days and an
//! [`Actual360`] day counter. It adds no behaviour over [`IborIndex`] - it is
//! pure configuration, so [`Euribor::new`] returns a plain [`IborIndex`].
//!
//! The one wrinkle QuantLib bakes in is that the business-day convention and the
//! end-of-month flag depend on the tenor's unit: `Days`/`Weeks` roll `Following`
//! and off month-end, `Months`/`Years` roll `ModifiedFollowing` and on month-end
//! (`euriborConvention` / `euriborEOM` in `euribor.cpp`).
//!
//! Deferred (separate tickets, per #301): `Euribor365` (the Actual/365 variant),
//! the daily-tenor `DailyTenor` constructors, and the rest of the Libor family.

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::target::Target;
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::{fail, require};

/// The Euribor index (`ql/indexes/ibor/euribor.hpp`).
///
/// A zero-sized namespace for the Euribor constructors. The rate is the one
/// fixed by the ECB; use a London-BBA index for the EurLibor fixing instead.
pub struct Euribor;

impl Euribor {
    /// Builds a Euribor index of the given `tenor` over the `forwarding` curve.
    ///
    /// Mirrors the C++ `Euribor::Euribor(tenor, h)` constructor: family name
    /// "Euribor", two settlement days, [`EUR`](Currency::eur), the [`Target`]
    /// calendar, the tenor-dependent convention and end-of-month flag, and an
    /// [`Actual360`] day counter. Daily tenors are rejected with the C++ message
    /// (they need the dedicated `DailyTenor` constructor, not ported yet).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<IborIndex> {
        let index = IborIndex::new(
            "Euribor".into(),
            tenor,
            2,
            Currency::eur(),
            Target::new(),
            euribor_convention(tenor)?,
            euribor_eom(tenor)?,
            Actual360::new(),
            forwarding,
            settings,
        );
        require!(
            index.tenor().units() != TimeUnit::Days,
            "for daily tenors ({}) dedicated DailyTenor constructor must be used",
            index.tenor()
        );
        Ok(index)
    }

    /// The 1-week Euribor index (`Euribor1W`).
    pub fn one_week(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::named(Period::new(1, TimeUnit::Weeks), forwarding, settings)
    }

    /// The 1-month Euribor index (`Euribor1M`).
    pub fn one_month(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::named(Period::new(1, TimeUnit::Months), forwarding, settings)
    }

    /// The 3-month Euribor index (`Euribor3M`).
    pub fn three_months(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::named(Period::new(3, TimeUnit::Months), forwarding, settings)
    }

    /// The 6-month Euribor index (`Euribor6M`).
    pub fn six_months(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::named(Period::new(6, TimeUnit::Months), forwarding, settings)
    }

    /// The 1-year Euribor index (`Euribor1Y`).
    pub fn one_year(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::named(Period::new(1, TimeUnit::Years), forwarding, settings)
    }

    /// Shared body for the named week/month/year helpers, whose fixed tenors can
    /// never trip the daily-tenor guard or the invalid-units branch.
    fn named(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        Self::new(tenor, forwarding, settings)
            .expect("a week, month, or year Euribor tenor is always valid")
    }
}

/// The tenor-dependent business-day convention (`euriborConvention`).
fn euribor_convention(tenor: Period) -> QlResult<BusinessDayConvention> {
    match tenor.units() {
        TimeUnit::Days | TimeUnit::Weeks => Ok(BusinessDayConvention::Following),
        TimeUnit::Months | TimeUnit::Years => Ok(BusinessDayConvention::ModifiedFollowing),
        _ => fail!("invalid time units"),
    }
}

/// The tenor-dependent end-of-month flag (`euriborEOM`).
fn euribor_eom(tenor: Period) -> QlResult<bool> {
    match tenor.units() {
        TimeUnit::Days | TimeUnit::Weeks => Ok(false),
        TimeUnit::Months | TimeUnit::Years => Ok(true),
        _ => fail!("invalid time units"),
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for `Euribor`, matching `euribor.cpp` construction and the
    //! `Euribor6M` usages in `indexes.cpp` / the floating-coupon suite.

    use super::*;
    use crate::indexes::index::Index;
    use crate::interestrate::Compounding;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::Month;
    use crate::time::frequency::Frequency;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn flat_curve(reference: Date, rate: f64) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// `euribor.cpp` construction: a `Euribor6M` composes the name, carries two
    /// fixing days, EUR, TARGET, and Actual/360, and (a `Months` tenor) rolls
    /// `ModifiedFollowing` on month-end.
    #[test]
    fn euribor6m_matches_the_quantlib_construction_table() {
        let settings = shared(Settings::<Date>::new());
        let index = Euribor::six_months(Handle::empty(), settings);

        assert_eq!(index.name(), "Euribor6M Actual/360");
        assert_eq!(index.fixing_days(), 2);
        assert_eq!(*index.currency(), Currency::eur());
        assert_eq!(index.fixing_calendar().name(), "TARGET");
        assert_eq!(index.day_counter().name(), "Actual/360");
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(index.end_of_month());
    }

    /// The `euriborConvention` / `euriborEOM` switch (`euribor.cpp`): a `Weeks`
    /// tenor rolls `Following` and off month-end - the other side of the
    /// Days/Weeks vs Months/Years boundary from the `Months` case above.
    #[test]
    fn one_week_rolls_following_off_month_end() {
        let settings = shared(Settings::<Date>::new());
        let index = Euribor::one_week(Handle::empty(), settings);

        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert!(!index.end_of_month());
    }

    /// The `euribor.cpp` guard: a daily tenor is an error carrying the C++
    /// message, not a panic (the `DailyTenor` constructor is not ported).
    #[test]
    fn a_daily_tenor_is_rejected_with_the_quantlib_message() {
        let settings = shared(Settings::<Date>::new());
        let Err(err) = Euribor::new(Period::new(1, TimeUnit::Days), Handle::empty(), settings)
        else {
            panic!("a daily tenor must be rejected");
        };
        assert!(
            err.to_string()
                .contains("for daily tenors (1D) dedicated DailyTenor constructor must be used")
        );
    }

    /// Euribor is pure configuration: a `Euribor6M` forecasts the same fixing as
    /// a hand-built `IborIndex` with the identical family, tenor, calendar,
    /// convention, and day counter over the same flat curve.
    #[test]
    fn euribor6m_forecasts_like_an_equivalent_hand_built_index() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let rate = 0.03;

        let euribor = Euribor::six_months(flat_curve(today, rate), settings.clone());
        let hand_built = IborIndex::new(
            "Euribor".into(),
            Period::new(6, TimeUnit::Months),
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            true,
            Actual360::new(),
            flat_curve(today, rate),
            settings,
        );

        let fixing_date = Date::new(15, Month::July, 2026);
        let a = euribor.forecast_fixing(fixing_date).unwrap();
        let b = hand_built.forecast_fixing(fixing_date).unwrap();
        assert_eq!(a, b);
    }
}
