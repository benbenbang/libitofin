//! Constant Black volatility.
//!
//! Port of `ql/termstructures/volatility/equityfx/blackconstantvol.hpp`:
//! [`BlackConstantVol`] implements the
//! [`BlackVolTermStructure`](super::BlackVolTermStructure) interface for a
//! constant Black volatility, with no time or strike dependence. The
//! volatility is either a fixed value (wrapped in an unobservable
//! [`SimpleQuote`], as in C++) or a quote handle whose changes propagate to
//! the structure's observers.
//!
//! As in C++, the business-day convention is pinned to `Following` and the
//! strike domain spans all of `Real`. The moving constructors take the
//! shared [`Settings`] handle explicitly, per D5.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, make_quote_handle};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::BlackVolTermStructure;

/// Constant Black volatility, no time-strike dependence.
pub struct BlackConstantVol {
    base: TermStructureBase,
    volatility: Handle<dyn Quote>,
}

impl BlackConstantVol {
    fn wrap(volatility: Volatility) -> Handle<dyn Quote> {
        make_quote_handle(volatility).handle()
    }

    fn assemble(
        base: TermStructureBase,
        volatility: Handle<dyn Quote>,
        observe: bool,
    ) -> BlackConstantVol {
        if observe {
            volatility.register_observer(&base.updater());
        }
        BlackConstantVol { base, volatility }
    }

    /// Constant vol structure with a fixed reference date.
    pub fn new(
        reference_date: Date,
        calendar: Option<Calendar>,
        volatility: Volatility,
        day_counter: DayCounter,
    ) -> BlackConstantVol {
        Self::assemble(
            TermStructureBase::with_reference_date(reference_date, calendar, Some(day_counter)),
            Self::wrap(volatility),
            false,
        )
    }

    /// Quote-backed structure with a fixed reference date; quote changes
    /// notify the structure's observers.
    pub fn with_quote(
        reference_date: Date,
        calendar: Option<Calendar>,
        volatility: Handle<dyn Quote>,
        day_counter: DayCounter,
    ) -> BlackConstantVol {
        Self::assemble(
            TermStructureBase::with_reference_date(reference_date, calendar, Some(day_counter)),
            volatility,
            true,
        )
    }

    /// Constant vol structure whose reference date moves off the evaluation
    /// date.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        volatility: Volatility,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> BlackConstantVol {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            Self::wrap(volatility),
            false,
        )
    }

    /// Quote-backed structure whose reference date moves off the evaluation
    /// date; quote changes notify the structure's observers.
    pub fn moving_with_quote(
        settlement_days: Natural,
        calendar: Calendar,
        volatility: Handle<dyn Quote>,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> BlackConstantVol {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            volatility,
            true,
        )
    }
}

impl AsObservable for BlackConstantVol {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for BlackConstantVol {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl VolatilityTermStructure for BlackConstantVol {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::Following
    }

    fn min_strike(&self) -> Rate {
        Rate::MIN
    }

    fn max_strike(&self) -> Rate {
        Rate::MAX
    }
}

impl BlackVolTermStructure for BlackConstantVol {
    fn black_vol_impl(&self, _t: Time, _strike: Real) -> QlResult<Volatility> {
        self.volatility.current_link()?.value()
    }

    fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
        self.variance_from_vol(t, strike)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::{Shared, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn flat_curve(vol: Volatility) -> (Date, BlackConstantVol) {
        let reference = Date::new(15, Month::June, 2026);
        let curve = BlackConstantVol::new(reference, Some(Target::new()), vol, Actual360::new());
        (reference, curve)
    }

    #[test]
    fn vol_is_constant_across_times_and_strikes() {
        let (reference, curve) = flat_curve(0.2);
        for t in [0.0, 0.25, 1.0, 10.0] {
            for strike in [1.0, 100.0, 1.0e6] {
                assert_eq!(curve.black_vol(t, strike, false).unwrap(), 0.2);
            }
        }
        assert_eq!(
            curve.black_vol_date(reference + 180, 100.0, false).unwrap(),
            0.2
        );
    }

    #[test]
    fn variance_is_vol_squared_times_time_in_both_forms() {
        let (reference, curve) = flat_curve(0.25);
        let var = curve.black_variance(2.0, 100.0, false).unwrap();
        assert!((var - 0.125).abs() < 1e-15);

        let date = reference + 180;
        let t = curve.time_from_reference(date).unwrap();
        assert_eq!(t, 0.5);
        let by_date = curve.black_variance_date(date, 100.0, false).unwrap();
        let by_time = curve.black_variance(t, 100.0, false).unwrap();
        assert_eq!(by_date, by_time);
        assert!((by_date - 0.25 * 0.25 * 0.5).abs() < 1e-15);
    }

    #[test]
    fn forward_vol_and_variance_are_flat() {
        let (_, curve) = flat_curve(0.2);
        for (t1, t2) in [(0.5, 1.5), (0.0, 0.0), (1.0, 1.0)] {
            let fwd = curve.black_forward_vol(t1, t2, 100.0, false).unwrap();
            assert!((fwd - 0.2).abs() < 1e-12);
        }
        let var = curve
            .black_forward_variance(1.0, 3.0, 100.0, false)
            .unwrap();
        assert!((var - 0.08).abs() < 1e-15);
    }

    #[test]
    fn every_strike_is_inside_the_domain() {
        let (_, curve) = flat_curve(0.2);
        assert!(curve.black_vol(1.0, Real::MAX, false).is_ok());
        assert!(curve.black_vol(1.0, Real::MIN, false).is_ok());
        assert_eq!(curve.min_strike(), Real::MIN);
        assert_eq!(curve.max_strike(), Real::MAX);
    }

    #[test]
    fn quote_changes_propagate_and_notify() {
        let reference = Date::new(15, Month::June, 2026);
        let handle = make_quote_handle(0.18);
        let curve = BlackConstantVol::with_quote(
            reference,
            Some(Target::new()),
            handle.handle(),
            Actual360::new(),
        );
        assert_eq!(curve.black_vol(1.0, 100.0, false).unwrap(), 0.18);

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));

        let quote = shared(SimpleQuote::new(0.23));
        handle.link_to(quote.clone() as Shared<dyn Quote>);
        assert!(Flag::is_up(&flag));
        assert_eq!(curve.black_vol(1.0, 100.0, false).unwrap(), 0.23);

        Flag::lower(&flag);
        quote.set_value(0.25);
        assert!(Flag::is_up(&flag));
        assert_eq!(curve.black_vol(1.0, 100.0, false).unwrap(), 0.25);
    }

    #[test]
    fn moving_reference_date_follows_the_evaluation_date() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let curve =
            BlackConstantVol::moving(2, Target::new(), 0.2, Actual360::new(), settings.clone());
        assert_eq!(
            curve.reference_date().unwrap(),
            Date::new(19, Month::January, 2026)
        );
        assert_eq!(curve.black_vol(1.0, 100.0, false).unwrap(), 0.2);

        settings.set_evaluation_date(Date::new(16, Month::January, 2026));
        assert_eq!(
            curve.reference_date().unwrap(),
            Date::new(20, Month::January, 2026)
        );
    }

    #[test]
    fn european_option_flat_vol_setup_round_trips() {
        let reference = Date::new(15, Month::June, 2026);
        for vol in [0.15, 0.20, 0.25, 0.30] {
            let curve =
                BlackConstantVol::new(reference, Some(Target::new()), vol, Actual360::new());
            let t = 0.5;
            assert_eq!(curve.black_vol(t, 100.0, false).unwrap(), vol);
            let var = curve.black_variance(t, 100.0, false).unwrap();
            assert!((var - vol * vol * t).abs() < 1e-15);
            let implied = (var / t).sqrt();
            assert!((implied - vol).abs() < 1e-15);
        }
    }
}
