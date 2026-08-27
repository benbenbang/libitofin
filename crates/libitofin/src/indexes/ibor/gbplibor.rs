//! The GBP Libor index.
//!
//! Port of `ql/indexes/ibor/gbplibor.hpp`. `GBPLibor` is the Pound Sterling
//! LIBOR fixed by ICE: the [`Libor`] base configuration with family name
//! "GBPLibor", zero settlement days (same-day spot), GBP, the UK Exchange
//! calendar as the financial center, and an [`Actual365Fixed`] day counter
//! (`gbplibor.hpp:41-49`). The financial center equals the fixing calendar,
//! so the base's joint maturity calendar is UK-plus-UK: behaviourally
//! UK-only, but wired faithfully through [`Libor::new`]. Like the base, it
//! is pure configuration: [`GbpLibor::new`] returns a plain [`IborIndex`].
//!
//! Deferred visibly: `DailyTenorGBPLibor` and `GBPLiborON`
//! (`gbplibor.hpp:52-70`) need the `DailyTenorLibor` base, see the deferral
//! note in [`libor`](crate::indexes::ibor::libor).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::ibor::libor::Libor;
use crate::indexes::iborindex::IborIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::unitedkingdom::{Market as UkMarket, UnitedKingdom};
use crate::time::date::Date;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::period::Period;

/// The GBP Libor index (`ql/indexes/ibor/gbplibor.hpp`, C++ `GBPLibor`).
///
/// A zero-sized namespace for the GBPLibor constructor.
pub struct GbpLibor;

impl GbpLibor {
    /// Builds a GBP Libor index of the given `tenor` over the `forwarding`
    /// curve.
    ///
    /// Mirrors the C++ `GBPLibor::GBPLibor(tenor, h)` constructor: the
    /// [`Libor`] base with family name "GBPLibor", zero settlement days,
    /// GBP, the [`UnitedKingdom`] Exchange financial-center calendar, and an
    /// [`Actual365Fixed`] day counter. The base's guards apply: daily tenors
    /// are rejected (`DailyTenorGBPLibor` and `GBPLiborON` are not ported
    /// yet).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<IborIndex> {
        Libor::new(
            "GBPLibor".into(),
            tenor,
            0,
            Currency::gbp(),
            UnitedKingdom::new(UkMarket::Exchange),
            Actual365Fixed::new(),
            forwarding,
            settings,
        )
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for `GBPLibor`: the `gbplibor.hpp:41-49` configuration table,
    //! a spot-lag-0 pin, and a maturity pin over the joint UK-plus-UK
    //! calendar mirroring the `usdlibor.rs` oracles.
    //!
    //! No bootstrap oracle: GBPLibor appears nowhere in
    //! `piecewiseyieldcurve.cpp`, so there is no cached fixture to port.

    use super::*;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::date::Month;
    use crate::time::timeunit::TimeUnit;

    fn gbp_libor_3m(settings: Shared<Settings<Date>>) -> IborIndex {
        GbpLibor::new(Period::new(3, TimeUnit::Months), Handle::empty(), settings)
            .expect("a 3M GBPLibor tenor is valid")
    }

    /// The `gbplibor.hpp:41-49` configuration: name, fixing days, currency,
    /// day counter, the tenor-derived convention and end-of-month flag, and
    /// the calendar wiring (fixing on UK Exchange; maturity on the joint
    /// UK-plus-UK calendar, faithfully doubled as in C++).
    #[test]
    fn gbp_libor_carries_the_ice_configuration() {
        let index = gbp_libor_3m(shared(Settings::<Date>::new()));
        assert_eq!(index.name(), "GBPLibor3M Actual/365 (Fixed)");
        assert_eq!(index.fixing_days(), 0);
        assert_eq!(index.currency(), &Currency::gbp());
        assert_eq!(index.day_counter().name(), "Actual/365 (Fixed)");
        assert_eq!(index.fixing_calendar().name(), "London stock exchange");
        assert_eq!(
            index.maturity_calendar().name(),
            "JoinHolidays(London stock exchange, London stock exchange)"
        );
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(index.end_of_month());
    }

    /// The same-day spot lag (`gbplibor.hpp:44`, `settlementDays == 0`): on
    /// a UK Exchange business day the value date IS the fixing date. A
    /// settlement-days-of-2 copy-paste from USDLibor would advance two UK
    /// business days instead.
    #[test]
    fn value_date_equals_the_fixing_date() {
        let index = gbp_libor_3m(shared(Settings::<Date>::new()));
        let fixing = Date::new(11, Month::August, 2020);
        assert_eq!(index.value_date(fixing).unwrap(), fixing);
    }

    /// `Libor::maturityDate` (`libor.cpp:102-113`) advances on the joint
    /// calendar, which for GBP is UK-plus-UK, so the roll must stay on the
    /// UK calendar - and this date discriminates the financial center:
    ///
    /// v = Tuesday 11 August 2020 (a UK Exchange business day, not the last
    /// business day of August, so the end-of-month flag is inert). v + 3M
    /// lands on Wednesday 11 November 2020: a UK Exchange business day (the
    /// UK has no November bank holiday, `unitedkingdom.rs:38-57`) and not
    /// month-end, but Veterans Day under US LiborImpact - a wrong-financial-
    /// center copy-paste from USDLibor would roll ModifiedFollowing to
    /// Thursday the 12th, while correct GBP keeps the 11th.
    ///
    /// The clone round-trip pins that `clone_with` copies the maturity
    /// calendar: a `SwapRateHelper` prices off a clone (`ratehelpers.rs:834`),
    /// so a clone dropping it would silently pass the other oracles.
    #[test]
    fn maturity_date_stays_on_the_uk_calendar() {
        let index = gbp_libor_3m(shared(Settings::<Date>::new()));
        let v = Date::new(11, Month::August, 2020);
        let maturity = index.maturity_date(v).unwrap();

        let uk = UnitedKingdom::new(UkMarket::Exchange);
        let expected = uk.advance_by_period(
            v,
            index.tenor(),
            index.business_day_convention(),
            index.end_of_month(),
        );

        assert_eq!(maturity, expected);
        assert_eq!(maturity, Date::new(11, Month::November, 2020));

        let clone = index.clone_with(Handle::empty());
        assert_eq!(clone.maturity_date(v).unwrap(), maturity);
    }
}
