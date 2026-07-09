//! Constant local volatility.
//!
//! Port of `ql/termstructures/volatility/equityfx/localconstantvol.hpp`:
//! [`LocalConstantVol`] implements the
//! [`LocalVolTermStructure`](super::LocalVolTermStructure) interface for a
//! constant local volatility, no time-strike dependence - basically a proxy
//! for a constant Black volatility. The volatility is either a fixed value
//! (wrapped in an unobservable [`SimpleQuote`], as in C++) or a quote handle
//! whose changes propagate to the structure's observers.
//!
//! As in C++, no calendar is set, the business-day convention is pinned to
//! `Following` and the strike domain spans all of `Real`. The moving
//! constructors take the shared [`Settings`] handle explicitly, per D5.

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

use super::LocalVolTermStructure;

/// Constant local volatility, no time-strike dependence.
pub struct LocalConstantVol {
    base: TermStructureBase,
    volatility: Handle<dyn Quote>,
}

impl LocalConstantVol {
    fn wrap(volatility: Volatility) -> Handle<dyn Quote> {
        make_quote_handle(volatility).handle()
    }

    fn assemble(
        base: TermStructureBase,
        volatility: Handle<dyn Quote>,
        observe: bool,
    ) -> LocalConstantVol {
        if observe {
            volatility.register_observer(&base.updater());
        }
        LocalConstantVol { base, volatility }
    }

    /// Constant local vol structure with a fixed reference date.
    pub fn new(
        reference_date: Date,
        volatility: Volatility,
        day_counter: DayCounter,
    ) -> LocalConstantVol {
        Self::assemble(
            TermStructureBase::with_reference_date(reference_date, None, Some(day_counter)),
            Self::wrap(volatility),
            false,
        )
    }

    /// Quote-backed structure with a fixed reference date; quote changes
    /// notify the structure's observers.
    pub fn with_quote(
        reference_date: Date,
        volatility: Handle<dyn Quote>,
        day_counter: DayCounter,
    ) -> LocalConstantVol {
        Self::assemble(
            TermStructureBase::with_reference_date(reference_date, None, Some(day_counter)),
            volatility,
            true,
        )
    }

    /// Constant local vol structure whose reference date moves off the
    /// evaluation date.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        volatility: Volatility,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> LocalConstantVol {
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
    ) -> LocalConstantVol {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            volatility,
            true,
        )
    }
}

impl AsObservable for LocalConstantVol {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for LocalConstantVol {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl VolatilityTermStructure for LocalConstantVol {
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

impl LocalVolTermStructure for LocalConstantVol {
    fn local_vol_impl(&self, _t: Time, _strike: Real) -> QlResult<Volatility> {
        self.volatility.current_link()?.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::{Shared, shared};
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn flat_curve(vol: Volatility) -> (Date, LocalConstantVol) {
        let reference = Date::new(15, Month::June, 2026);
        let curve = LocalConstantVol::new(reference, vol, Actual360::new());
        (reference, curve)
    }

    #[test]
    fn local_vol_is_constant_across_times_and_levels() {
        let (reference, curve) = flat_curve(0.2);
        for t in [0.0, 0.25, 1.0, 10.0] {
            for level in [1.0, 100.0, 1.0e6] {
                assert_eq!(curve.local_vol(t, level, false).unwrap(), 0.2);
            }
        }
        assert_eq!(
            curve.local_vol_date(reference + 180, 100.0, false).unwrap(),
            0.2
        );
    }

    #[test]
    fn every_level_is_inside_the_domain() {
        let (_, curve) = flat_curve(0.2);
        assert!(curve.local_vol(1.0, Real::MAX, false).is_ok());
        assert!(curve.local_vol(1.0, Real::MIN, false).is_ok());
        assert_eq!(curve.min_strike(), Real::MIN);
        assert_eq!(curve.max_strike(), Real::MAX);
        assert_eq!(curve.max_date(), Date::max_date());
    }

    #[test]
    fn range_checks_gate_time_and_nan() {
        let (_, curve) = flat_curve(0.2);
        assert!(curve.local_vol(-0.5, 100.0, false).is_err());
        assert!(curve.local_vol(Time::NAN, 100.0, false).is_err());
    }

    #[test]
    fn quote_changes_propagate_and_notify() {
        let reference = Date::new(15, Month::June, 2026);
        let handle = make_quote_handle(0.18);
        let curve = LocalConstantVol::with_quote(reference, handle.handle(), Actual360::new());
        assert_eq!(curve.local_vol(1.0, 100.0, false).unwrap(), 0.18);

        let flag = Flag::new();
        curve.observable().register_observer(&as_observer(&flag));

        let quote = shared(SimpleQuote::new(0.23));
        handle.link_to(quote.clone() as Shared<dyn Quote>);
        assert!(Flag::is_up(&flag));
        assert_eq!(curve.local_vol(1.0, 100.0, false).unwrap(), 0.23);

        Flag::lower(&flag);
        quote.set_value(0.25);
        assert!(Flag::is_up(&flag));
        assert_eq!(curve.local_vol(1.0, 100.0, false).unwrap(), 0.25);
    }
}
