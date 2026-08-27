//! The JPY Libor index.
//!
//! Port of `ql/indexes/ibor/jpylibor.hpp`. `JPYLibor` is the Japanese Yen
//! LIBOR fixed by ICE: the [`Libor`] base configuration with family name
//! "JPYLibor", two settlement days, JPY, the [`Japan`] calendar as the
//! financial center, and an [`Actual360`] day counter (`jpylibor.hpp:44-53`).
//! This is the rate fixed in London by ICE, not the Tokyo TIBOR fixing. Like
//! the base, it is pure configuration: [`JpyLibor::new`] returns a plain
//! [`IborIndex`] whose maturity calendar is the joint
//! UK-Exchange-plus-Japan calendar.
//!
//! Deferred visibly: `DailyTenorJPYLibor` (needs the `DailyTenorLibor` base,
//! see the deferral note in [`libor`](crate::indexes::ibor::libor)).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::ibor::libor::Libor;
use crate::indexes::iborindex::IborIndex;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::japan::Japan;
use crate::time::date::Date;
use crate::time::daycounters::actual360::Actual360;
use crate::time::period::Period;

/// The JPY Libor index (`ql/indexes/ibor/jpylibor.hpp`, C++ `JPYLibor`).
///
/// A zero-sized namespace for the JPYLibor constructor.
pub struct JpyLibor;

impl JpyLibor {
    /// Builds a JPY Libor index of the given `tenor` over the `forwarding`
    /// curve.
    ///
    /// Mirrors the C++ `JPYLibor::JPYLibor(tenor, h)` constructor: the
    /// [`Libor`] base with family name "JPYLibor", two settlement days, JPY,
    /// the [`Japan`] financial-center calendar, and an [`Actual360`] day
    /// counter. The base's guards apply: daily tenors are rejected
    /// (`DailyTenorJPYLibor` is not ported yet).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<IborIndex> {
        Libor::new(
            "JPYLibor".into(),
            tenor,
            2,
            Currency::jpy(),
            Japan::new(),
            Actual360::new(),
            forwarding,
            settings,
        )
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for `JPYLibor`: the `jpylibor.hpp:44-53` configuration table,
    //! a maturity pin over the actual joint calendar mirroring the
    //! `usdlibor.rs` oracles, and the `testJpyLibor` bootstrap round-trip
    //! (`piecewiseyieldcurve.cpp:964`).

    use super::*;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::instruments::MakeVanillaSwap;
    use crate::math::interpolations::loglinear::LogLinear;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::shared;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::Discount;
    use crate::termstructures::yields::{PiecewiseYieldCurve, SwapRateHelper};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedkingdom::{Market as UkMarket, UnitedKingdom};
    use crate::time::date::Month;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Rate;

    fn jpy_libor_6m(settings: Shared<Settings<Date>>) -> IborIndex {
        JpyLibor::new(Period::new(6, TimeUnit::Months), Handle::empty(), settings)
            .expect("a 6M JPYLibor tenor is valid")
    }

    /// The `jpylibor.hpp:44-53` configuration: name, fixing days, currency,
    /// day counter, the tenor-derived convention and end-of-month flag, and
    /// the calendar wiring (fixing on UK Exchange, NOT Japan; maturity on the
    /// joint UK-plus-Japan calendar).
    #[test]
    fn jpy_libor_carries_the_ice_configuration() {
        let index = jpy_libor_6m(shared(Settings::<Date>::new()));
        assert_eq!(index.name(), "JPYLibor6M Actual/360");
        assert_eq!(index.fixing_days(), 2);
        assert_eq!(index.currency(), &Currency::jpy());
        assert_eq!(index.day_counter().name(), "Actual/360");
        assert_eq!(index.fixing_calendar().name(), "London stock exchange");
        assert_eq!(
            index.maturity_calendar().name(),
            "JoinHolidays(London stock exchange, Japan)"
        );
        assert_eq!(
            index.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(index.end_of_month());
    }

    /// `Libor::maturityDate` (`libor.cpp:102-113`) advances on the JOINT
    /// calendar, and this date discriminates joint-vs-UK-only:
    ///
    /// v = Monday 23 May 2022 (a business day in both calendars: not a
    /// Japanese May holiday, which end on the 5th-6th, and not a UK May bank
    /// holiday, which in 2022 were Monday 2 May and the Jubilee days 2-3
    /// June; not the last business day of May, so the end-of-month flag is
    /// inert). v + 6M lands on Wednesday 23 November 2022: Labor
    /// Thanksgiving Day, a fixed-date (d == 23, no Monday shift) Japanese
    /// holiday (`japan.rs:128`), a UK Exchange business day (the UK has no
    /// November bank holiday), and not month-end (the 30th is, so the EOM
    /// roll cannot mask the difference). The joint calendar rolls
    /// ModifiedFollowing to Thursday the 24th (a Japan business day: the
    /// d == 24 arm only fires on a Monday); UK-only would keep the 23rd.
    ///
    /// The clone round-trip pins that `clone_with` copies the maturity
    /// calendar: a `SwapRateHelper` prices off a clone (`ratehelpers.rs:834`),
    /// so a clone dropping it would silently pass a spot-date oracle.
    #[test]
    fn maturity_date_advances_on_the_joint_calendar() {
        let index = jpy_libor_6m(shared(Settings::<Date>::new()));
        let v = Date::new(23, Month::May, 2022);
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
        assert_eq!(maturity, Date::new(24, Month::November, 2022));
        assert_eq!(uk_only, Date::new(23, Month::November, 2022));
        assert_ne!(maturity, uk_only);

        let clone = index.clone_with(Handle::empty());
        assert_eq!(clone.maturity_date(v).unwrap(), maturity);
    }

    // (n-in-years, rate-in-percent), transcribed from the shared `swapData`
    // table of piecewiseyieldcurve.cpp.
    const SWAP_DATA: [(i32, Rate); 15] = [
        (1, 4.54),
        (2, 4.63),
        (3, 4.75),
        (4, 4.86),
        (5, 4.99),
        (6, 5.11),
        (7, 5.23),
        (8, 5.33),
        (9, 5.41),
        (10, 5.47),
        (12, 5.60),
        (15, 5.75),
        (20, 5.89),
        (25, 5.95),
        (30, 5.96),
    ];

    /// `testJpyLibor` (`piecewiseyieldcurve.cpp:964`): on 4 October 2007 a
    /// `PiecewiseYieldCurve<Discount, LogLinear>` is bootstrapped from
    /// JPYLibor6M swap helpers on the Japan calendar (spot two Japan business
    /// days after today, skipping the 8 October Health and Sports Day), and
    /// each input swap repriced off the curve is at par within 1e-9. The
    /// fixed-leg conventions are the C++ `CommonVars` defaults: annual,
    /// unadjusted, Thirty360 BondBasis. The `nodes()` call pins that the
    /// solved curve is introspectable without error.
    #[test]
    fn jpy_libor_swap_curve_reprices_its_input_swaps() {
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(4, Month::October, 2007);
        settings.set_evaluation_date(today);
        let calendar = Japan::new();
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let index = jpy_libor_6m(settings.clone());
        let mut instruments: Vec<Shared<dyn RateHelper>> = Vec::new();
        for (n, rate) in SWAP_DATA {
            let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
            instruments.push(SwapRateHelper::new(
                quote,
                Period::new(n, TimeUnit::Years),
                calendar.clone(),
                Frequency::Annual,
                BusinessDayConvention::Unadjusted,
                Thirty360::with_convention(Convention::BondBasis),
                &index,
            ) as Shared<dyn RateHelper>);
        }

        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            settlement,
            instruments,
            Actual360::new(),
            LogLinear,
        )
        .unwrap();
        curve
            .nodes()
            .expect("the bootstrapped curve exposes its nodes");
        let handle: Handle<dyn YieldTermStructure> =
            Handle::new(curve as Shared<dyn YieldTermStructure>);

        let jpylibor6m = shared(
            JpyLibor::new(
                Period::new(6, TimeUnit::Months),
                handle.clone(),
                settings.clone(),
            )
            .expect("a 6M JPYLibor tenor is valid"),
        );
        for (n, rate) in SWAP_DATA {
            let mut swap = MakeVanillaSwap::new(
                Period::new(n, TimeUnit::Years),
                Shared::clone(&jpylibor6m),
                Some(0.0),
                Period::new(0, TimeUnit::Days),
                settings.clone(),
            )
            .with_effective_date(settlement)
            .with_discounting_term_structure(handle.clone())
            .with_fixed_leg_day_count(Thirty360::with_convention(Convention::BondBasis))
            .with_fixed_leg_tenor(Period::try_from(Frequency::Annual).unwrap())
            .with_fixed_leg_convention(BusinessDayConvention::Unadjusted)
            .with_fixed_leg_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_fixed_leg_calendar(calendar.clone())
            .with_floating_leg_calendar(calendar.clone())
            .build()
            .unwrap();

            let estimated = swap.fixed_vs_floating_mut().fair_rate().unwrap();
            let expected = rate / 100.0;
            assert!(
                (estimated - expected).abs() <= 1.0e-9,
                "{n} year(s) swap: estimated {estimated} vs expected {expected}"
            );
        }
    }
}
