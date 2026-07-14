//! Constant caplet/floorlet volatility.
//!
//! Port of `ql/termstructures/volatility/optionlet/constantoptionletvol.{hpp,cpp}`:
//! [`ConstantOptionletVolatility`] implements
//! [`OptionletVolatilityStructure`](super::OptionletVolatilityStructure) with a
//! single volatility, no time or strike dependence. The volatility is either a
//! fixed value (wrapped in an unobservable [`SimpleQuote`], as in C++) or a
//! quote handle whose changes propagate to the structure's observers. The
//! business-day convention, volatility type and displacement are pinned by the
//! constructor; the strike domain spans all of `Real`.
//!
//! The moving constructors take the shared [`Settings`] handle explicitly, per
//! D5.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, make_quote_handle};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::volatility::{VolatilityTermStructure, VolatilityType};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::OptionletVolatilityStructure;

/// Constant caplet volatility, no time-strike dependence.
pub struct ConstantOptionletVolatility {
    base: TermStructureBase,
    business_day_convention: BusinessDayConvention,
    volatility: Handle<dyn Quote>,
    volatility_type: VolatilityType,
    displacement: Real,
}

impl ConstantOptionletVolatility {
    fn wrap(volatility: Volatility) -> Handle<dyn Quote> {
        make_quote_handle(volatility).handle()
    }

    fn assemble(
        base: TermStructureBase,
        business_day_convention: BusinessDayConvention,
        volatility: Handle<dyn Quote>,
        volatility_type: VolatilityType,
        displacement: Real,
        observe: bool,
    ) -> ConstantOptionletVolatility {
        if observe {
            volatility.register_observer(&base.updater());
        }
        ConstantOptionletVolatility {
            base,
            business_day_convention,
            volatility,
            volatility_type,
            displacement,
        }
    }

    /// Fixed reference date, fixed market data.
    pub fn new(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: Volatility,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        displacement: Real,
    ) -> ConstantOptionletVolatility {
        Self::assemble(
            TermStructureBase::with_reference_date(
                reference_date,
                Some(calendar),
                Some(day_counter),
            ),
            business_day_convention,
            Self::wrap(volatility),
            volatility_type,
            displacement,
            false,
        )
    }

    /// Fixed reference date, quote-backed market data; quote changes notify the
    /// structure's observers.
    pub fn with_quote(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: Handle<dyn Quote>,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        displacement: Real,
    ) -> ConstantOptionletVolatility {
        Self::assemble(
            TermStructureBase::with_reference_date(
                reference_date,
                Some(calendar),
                Some(day_counter),
            ),
            business_day_convention,
            volatility,
            volatility_type,
            displacement,
            true,
        )
    }

    /// Reference date moving off the evaluation date, fixed market data.
    #[allow(clippy::too_many_arguments)]
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: Volatility,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        displacement: Real,
        settings: Shared<Settings<Date>>,
    ) -> ConstantOptionletVolatility {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            Self::wrap(volatility),
            volatility_type,
            displacement,
            false,
        )
    }

    /// Reference date moving off the evaluation date, quote-backed market data;
    /// quote changes notify the structure's observers.
    #[allow(clippy::too_many_arguments)]
    pub fn moving_with_quote(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: Handle<dyn Quote>,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        displacement: Real,
        settings: Shared<Settings<Date>>,
    ) -> ConstantOptionletVolatility {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            volatility,
            volatility_type,
            displacement,
            true,
        )
    }
}

impl AsObservable for ConstantOptionletVolatility {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ConstantOptionletVolatility {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl VolatilityTermStructure for ConstantOptionletVolatility {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    fn min_strike(&self) -> Rate {
        Rate::MIN
    }

    fn max_strike(&self) -> Rate {
        Rate::MAX
    }
}

impl OptionletVolatilityStructure for ConstantOptionletVolatility {
    fn volatility_impl(&self, _option_time: Time, _strike: Rate) -> QlResult<Volatility> {
        self.volatility.current_link()?.value()
    }

    fn volatility_type(&self) -> VolatilityType {
        self.volatility_type
    }

    fn displacement(&self) -> Real {
        self.displacement
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

    fn flat_surface(vol: Volatility) -> (Date, ConstantOptionletVolatility) {
        let reference = Date::new(15, Month::June, 2026);
        let surface = ConstantOptionletVolatility::new(
            reference,
            Target::new(),
            BusinessDayConvention::Following,
            vol,
            Actual360::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
        );
        (reference, surface)
    }

    #[test]
    fn volatility_is_constant_across_times_and_strikes() {
        let (reference, surface) = flat_surface(0.2);
        for t in [0.0, 0.25, 1.0, 10.0] {
            for strike in [-0.01, 0.0, 0.03, 1.0e6] {
                assert_eq!(surface.volatility(t, strike, false).unwrap(), 0.2);
            }
        }
        assert_eq!(
            surface
                .volatility_date(reference + 180, 0.03, false)
                .unwrap(),
            0.2
        );
    }

    #[test]
    fn black_variance_is_vol_squared_times_time_in_every_form() {
        let (reference, surface) = flat_surface(0.25);
        let var = surface.black_variance(2.0, 0.03, false).unwrap();
        assert!((var - 0.125).abs() < 1e-15);

        let date = reference + 180;
        let t = surface.time_from_reference(date).unwrap();
        assert_eq!(t, 0.5);
        let by_date = surface.black_variance_date(date, 0.03, false).unwrap();
        let by_time = surface.black_variance(t, 0.03, false).unwrap();
        assert_eq!(by_date, by_time);
        assert!((by_date - 0.25 * 0.25 * 0.5).abs() < 1e-15);
    }

    #[test]
    fn tenor_queries_advance_on_the_calendar() {
        use crate::time::period::Period;
        use crate::time::timeunit::TimeUnit;
        let (_, surface) = flat_surface(0.2);
        let tenor = Period::new(6, TimeUnit::Months);
        let by_tenor = surface.volatility_tenor(tenor, 0.03, false).unwrap();
        assert_eq!(by_tenor, 0.2);
        let var = surface.black_variance_tenor(tenor, 0.03, false).unwrap();
        assert!(var > 0.0);
    }

    #[test]
    fn type_and_displacement_are_reported() {
        let reference = Date::new(15, Month::June, 2026);
        let surface = ConstantOptionletVolatility::new(
            reference,
            Target::new(),
            BusinessDayConvention::Following,
            0.2,
            Actual360::new(),
            VolatilityType::Normal,
            0.01,
        );
        assert_eq!(surface.volatility_type(), VolatilityType::Normal);
        assert_eq!(surface.displacement(), 0.01);
    }

    #[test]
    fn defaults_report_shifted_lognormal_without_displacement() {
        let (_, surface) = flat_surface(0.2);
        assert_eq!(surface.volatility_type(), VolatilityType::ShiftedLognormal);
        assert_eq!(surface.displacement(), 0.0);
    }

    #[test]
    fn every_strike_is_inside_the_domain() {
        let (_, surface) = flat_surface(0.2);
        assert!(surface.volatility(1.0, Real::MAX, false).is_ok());
        assert!(surface.volatility(1.0, Real::MIN, false).is_ok());
        assert_eq!(surface.min_strike(), Real::MIN);
        assert_eq!(surface.max_strike(), Real::MAX);
    }

    #[test]
    fn quote_changes_propagate_and_notify() {
        let reference = Date::new(15, Month::June, 2026);
        let handle = make_quote_handle(0.18);
        let surface = ConstantOptionletVolatility::with_quote(
            reference,
            Target::new(),
            BusinessDayConvention::Following,
            handle.handle(),
            Actual360::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
        );
        assert_eq!(surface.volatility(1.0, 0.03, false).unwrap(), 0.18);

        let flag = Flag::new();
        surface.observable().register_observer(&as_observer(&flag));

        let quote = shared(SimpleQuote::new(0.23));
        handle.link_to(quote.clone() as Shared<dyn Quote>);
        assert!(Flag::is_up(&flag));
        assert_eq!(surface.volatility(1.0, 0.03, false).unwrap(), 0.23);
    }

    #[test]
    fn moving_reference_date_follows_the_evaluation_date() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let surface = ConstantOptionletVolatility::moving(
            2,
            Target::new(),
            BusinessDayConvention::Following,
            0.2,
            Actual360::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
            settings.clone(),
        );
        assert_eq!(
            surface.reference_date().unwrap(),
            Date::new(19, Month::January, 2026)
        );
        assert_eq!(surface.volatility(1.0, 0.03, false).unwrap(), 0.2);

        settings.set_evaluation_date(Date::new(16, Month::January, 2026));
        assert_eq!(
            surface.reference_date().unwrap(),
            Date::new(20, Month::January, 2026)
        );
    }
}
