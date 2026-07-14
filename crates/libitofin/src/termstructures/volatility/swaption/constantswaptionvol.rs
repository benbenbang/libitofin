//! Constant swaption volatility.
//!
//! Port of `ql/termstructures/volatility/swaption/swaptionconstantvol.{hpp,cpp}`:
//! [`ConstantSwaptionVolatility`] implements
//! [`SwaptionVolatilityStructure`](super::SwaptionVolatilityStructure) with a
//! single volatility, no option-time, swap-length or strike dependence. The
//! volatility is either a fixed value (wrapped in an unobservable
//! [`SimpleQuote`](crate::quotes::SimpleQuote), as in C++) or a quote handle
//! whose changes propagate to the structure's observers. The business-day
//! convention, volatility type and shift are pinned by the constructor; the
//! strike domain spans all of `Real` and the swap-tenor domain spans 100 years.
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
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::SwaptionVolatilityStructure;

/// Constant swaption volatility, no time-strike dependence.
pub struct ConstantSwaptionVolatility {
    base: TermStructureBase,
    business_day_convention: BusinessDayConvention,
    volatility: Handle<dyn Quote>,
    max_swap_tenor: Period,
    volatility_type: VolatilityType,
    shift: Real,
}

impl ConstantSwaptionVolatility {
    fn wrap(volatility: Volatility) -> Handle<dyn Quote> {
        make_quote_handle(volatility).handle()
    }

    fn assemble(
        base: TermStructureBase,
        business_day_convention: BusinessDayConvention,
        volatility: Handle<dyn Quote>,
        volatility_type: VolatilityType,
        shift: Real,
        observe: bool,
    ) -> ConstantSwaptionVolatility {
        if observe {
            volatility.register_observer(&base.updater());
        }
        ConstantSwaptionVolatility {
            base,
            business_day_convention,
            volatility,
            max_swap_tenor: Period::new(100, TimeUnit::Years),
            volatility_type,
            shift,
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
        shift: Real,
    ) -> ConstantSwaptionVolatility {
        Self::assemble(
            TermStructureBase::with_reference_date(
                reference_date,
                Some(calendar),
                Some(day_counter),
            ),
            business_day_convention,
            Self::wrap(volatility),
            volatility_type,
            shift,
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
        shift: Real,
    ) -> ConstantSwaptionVolatility {
        Self::assemble(
            TermStructureBase::with_reference_date(
                reference_date,
                Some(calendar),
                Some(day_counter),
            ),
            business_day_convention,
            volatility,
            volatility_type,
            shift,
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
        shift: Real,
        settings: Shared<Settings<Date>>,
    ) -> ConstantSwaptionVolatility {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            Self::wrap(volatility),
            volatility_type,
            shift,
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
        shift: Real,
        settings: Shared<Settings<Date>>,
    ) -> ConstantSwaptionVolatility {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            volatility,
            volatility_type,
            shift,
            true,
        )
    }
}

impl AsObservable for ConstantSwaptionVolatility {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ConstantSwaptionVolatility {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl VolatilityTermStructure for ConstantSwaptionVolatility {
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

impl SwaptionVolatilityStructure for ConstantSwaptionVolatility {
    fn volatility_impl(
        &self,
        _option_time: Time,
        _swap_length: Time,
        _strike: Rate,
    ) -> QlResult<Volatility> {
        self.volatility.current_link()?.value()
    }

    fn max_swap_tenor(&self) -> Period {
        self.max_swap_tenor
    }

    fn volatility_type(&self) -> VolatilityType {
        self.volatility_type
    }

    fn shift_impl(&self, _option_time: Time, _swap_length: Time) -> QlResult<Real> {
        super::require_lognormal_for_shift(self.volatility_type)?;
        Ok(self.shift)
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

    fn flat_surface(vol: Volatility) -> (Date, ConstantSwaptionVolatility) {
        let reference = Date::new(15, Month::June, 2026);
        let surface = ConstantSwaptionVolatility::new(
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
    fn volatility_is_constant_across_option_and_swap_axes() {
        let (reference, surface) = flat_surface(0.2);
        for option_time in [0.0, 0.25, 1.0, 10.0] {
            for swap_length in [0.5, 1.0, 30.0] {
                for strike in [-0.01, 0.0, 0.03, 1.0e6] {
                    assert_eq!(
                        surface
                            .volatility_time(option_time, swap_length, strike, false)
                            .unwrap(),
                        0.2
                    );
                }
            }
        }
        assert_eq!(
            surface
                .volatility(reference + 180, 5.0, 0.03, false)
                .unwrap(),
            0.2
        );
    }

    #[test]
    fn black_variance_is_vol_squared_times_option_time() {
        let (reference, surface) = flat_surface(0.25);
        let var = surface.black_variance_time(2.0, 5.0, 0.03, false).unwrap();
        assert!((var - 0.125).abs() < 1e-15);

        let date = reference + 180;
        let t = surface.time_from_reference(date).unwrap();
        assert_eq!(t, 0.5);
        let by_date = surface.black_variance(date, 5.0, 0.03, false).unwrap();
        let by_time = surface.black_variance_time(t, 5.0, 0.03, false).unwrap();
        assert_eq!(by_date, by_time);
        assert!((by_date - 0.25 * 0.25 * 0.5).abs() < 1e-15);
    }

    #[test]
    fn max_swap_tenor_and_length_span_a_century() {
        let (_, surface) = flat_surface(0.2);
        assert_eq!(surface.max_swap_tenor(), Period::new(100, TimeUnit::Years));
        assert_eq!(surface.max_swap_length().unwrap(), 100.0);
    }

    #[test]
    fn shifted_lognormal_reports_its_shift() {
        let reference = Date::new(15, Month::June, 2026);
        let surface = ConstantSwaptionVolatility::new(
            reference,
            Target::new(),
            BusinessDayConvention::Following,
            0.2,
            Actual360::new(),
            VolatilityType::ShiftedLognormal,
            0.01,
        );
        assert_eq!(surface.volatility_type(), VolatilityType::ShiftedLognormal);
        assert_eq!(surface.shift(reference + 90, 5.0, false).unwrap(), 0.01);
    }

    #[test]
    fn normal_surface_rejects_a_shift_query() {
        let reference = Date::new(15, Month::June, 2026);
        let surface = ConstantSwaptionVolatility::new(
            reference,
            Target::new(),
            BusinessDayConvention::Following,
            0.2,
            Actual360::new(),
            VolatilityType::Normal,
            0.0,
        );
        assert_eq!(surface.volatility_type(), VolatilityType::Normal);
        assert!(surface.shift(reference + 90, 5.0, false).is_err());
    }

    #[test]
    fn defaults_report_shifted_lognormal_without_shift() {
        let (reference, surface) = flat_surface(0.2);
        assert_eq!(surface.volatility_type(), VolatilityType::ShiftedLognormal);
        assert_eq!(surface.shift(reference + 90, 5.0, false).unwrap(), 0.0);
    }

    #[test]
    fn engine_facing_constructor_uses_null_calendar_settlement_zero() {
        use crate::time::calendars::nullcalendar::NullCalendar;
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let surface = ConstantSwaptionVolatility::moving(
            0,
            NullCalendar::new(),
            BusinessDayConvention::Following,
            0.2,
            Actual360::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
            settings.clone(),
        );
        assert_eq!(
            surface.reference_date().unwrap(),
            Date::new(15, Month::January, 2026)
        );
        let variance = surface
            .black_variance(Date::new(15, Month::January, 2027), 5.0, 0.03, false)
            .unwrap();
        assert!(variance > 0.0);
    }

    #[test]
    fn quote_changes_propagate_and_notify() {
        let reference = Date::new(15, Month::June, 2026);
        let handle = make_quote_handle(0.18);
        let surface = ConstantSwaptionVolatility::with_quote(
            reference,
            Target::new(),
            BusinessDayConvention::Following,
            handle.handle(),
            Actual360::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
        );
        assert_eq!(
            surface.volatility_time(1.0, 5.0, 0.03, false).unwrap(),
            0.18
        );

        let flag = Flag::new();
        surface.observable().register_observer(&as_observer(&flag));

        let quote = shared(SimpleQuote::new(0.23));
        handle.link_to(quote.clone() as Shared<dyn Quote>);
        assert!(Flag::is_up(&flag));
        assert_eq!(
            surface.volatility_time(1.0, 5.0, 0.03, false).unwrap(),
            0.23
        );
    }

    #[test]
    fn moving_reference_date_follows_the_evaluation_date() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let surface = ConstantSwaptionVolatility::moving(
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
        assert_eq!(surface.volatility_time(1.0, 5.0, 0.03, false).unwrap(), 0.2);

        settings.set_evaluation_date(Date::new(16, Month::January, 2026));
        assert_eq!(
            surface.reference_date().unwrap(),
            Date::new(20, Month::January, 2026)
        );
    }
}
