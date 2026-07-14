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
