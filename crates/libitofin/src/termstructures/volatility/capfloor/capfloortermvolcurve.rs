//! ATM cap/floor term-volatility curve using QuantLib's natural cubic spline.

use super::CapFloorTermVolatilityStructure;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::cubic::{CubicDerivativeApprox, CubicInterpolation};
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::{
    businessdayconvention::BusinessDayConvention, calendar::Calendar, date::Date,
    daycounter::DayCounter, period::Period,
};
use crate::types::{Natural, Rate, Time, Volatility};

/// Live ATM quotes interpolated over cap/floor expiry time.
pub struct CapFloorTermVolCurve {
    base: TermStructureBase,
    convention: BusinessDayConvention,
    tenors: Vec<Period>,
    quotes: Vec<Handle<dyn Quote>>,
}

impl CapFloorTermVolCurve {
    /// Quote-backed curve with a fixed reference date.
    pub fn with_reference_date(
        date: Date,
        calendar: Calendar,
        convention: BusinessDayConvention,
        tenors: Vec<Period>,
        quotes: Vec<Handle<dyn Quote>>,
        day_counter: DayCounter,
    ) -> QlResult<Self> {
        Self::assemble(
            TermStructureBase::with_reference_date(date, Some(calendar), Some(day_counter)),
            convention,
            tenors,
            quotes,
        )
    }

    /// Quote-backed curve whose reference date follows the evaluation date.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        convention: BusinessDayConvention,
        tenors: Vec<Period>,
        quotes: Vec<Handle<dyn Quote>>,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            convention,
            tenors,
            quotes,
        )
    }

    fn assemble(
        base: TermStructureBase,
        convention: BusinessDayConvention,
        tenors: Vec<Period>,
        quotes: Vec<Handle<dyn Quote>>,
    ) -> QlResult<Self> {
        crate::require!(
            tenors.len() >= 2 && tenors.len() == quotes.len(),
            "at least two matching option tenors and volatilities are required"
        );
        for tenor in &tenors {
            use crate::time::timeunit::TimeUnit;
            let max = match tenor.units() {
                TimeUnit::Days => 366 * 300,
                TimeUnit::Weeks => 53 * 300,
                TimeUnit::Months => 12 * 300,
                TimeUnit::Years => 300,
                _ => crate::fail!("cap/floor tenors must use days, weeks, months or years"),
            };
            crate::require!(
                tenor.length() > 0 && tenor.length() <= max,
                "cap/floor tenor outside supported date range"
            );
        }
        crate::require!(
            tenors.windows(2).all(|w| w[0] < w[1]),
            "option tenors must be strictly increasing"
        );
        for quote in &quotes {
            quote.register_observer(&base.updater());
        }
        let result = Self {
            base,
            convention,
            tenors,
            quotes,
        };
        result.interpolation()?;
        Ok(result)
    }

    /// Ordered cap/floor option tenors.
    pub fn option_tenors(&self) -> &[Period] {
        &self.tenors
    }

    /// Option times recalculated from the current reference date.
    pub fn option_times(&self) -> QlResult<Vec<Time>> {
        self.tenors
            .iter()
            .map(|&p| self.time_from_reference(self.option_date_from_tenor(p)?))
            .collect()
    }

    fn interpolation(&self) -> QlResult<CubicInterpolation> {
        let values = self
            .quotes
            .iter()
            .map(|q| {
                let value = q.current_link()?.value()?;
                crate::require!(
                    value.is_finite() && value >= 0.0,
                    "cap/floor volatility must be finite and nonnegative"
                );
                Ok(value)
            })
            .collect::<QlResult<Vec<_>>>()?;
        Ok(
            CubicInterpolation::new(self.option_times()?, values, CubicDerivativeApprox::Spline)?
                .with_extrapolation(true),
        )
    }
}

impl AsObservable for CapFloorTermVolCurve {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl TermStructure for CapFloorTermVolCurve {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn reference_date(&self) -> QlResult<Date> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.base.reference_date()))
            .unwrap_or_else(|_| {
                Err(crate::errors::QlError::new(
                    "cap/floor reference date outside supported range",
                    file!(),
                    line!(),
                ))
            })
    }
    fn max_time(&self) -> QlResult<Time> {
        self.time_from_reference(
            self.option_date_from_tenor(*self.tenors.last().expect("validated tenors"))?,
        )
    }
    fn max_date(&self) -> Date {
        self.option_date_from_tenor(*self.tenors.last().expect("validated tenors"))
            .unwrap_or_else(|_| Date::null())
    }
}
impl VolatilityTermStructure for CapFloorTermVolCurve {
    fn option_date_from_tenor(&self, tenor: Period) -> QlResult<Date> {
        let reference = self.reference_date()?;
        let calendar = self
            .calendar()
            .expect("curve construction requires calendar");
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            calendar.advance_by_period(reference, tenor, self.convention, false)
        }))
        .map_err(|_| {
            crate::errors::QlError::new(
                "cap/floor option date outside supported range",
                file!(),
                line!(),
            )
        })
    }

    fn business_day_convention(&self) -> BusinessDayConvention {
        self.convention
    }
    fn min_strike(&self) -> Rate {
        -f64::MAX
    }
    fn max_strike(&self) -> Rate {
        f64::MAX
    }
}
impl CapFloorTermVolatilityStructure for CapFloorTermVolCurve {
    fn volatility_impl(&self, length: Time, _strike: Rate) -> QlResult<Volatility> {
        self.interpolation()?.value(length)
    }
}
