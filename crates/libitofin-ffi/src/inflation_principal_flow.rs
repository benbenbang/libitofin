//! Intrinsic checked inflation flows retained through generic conversions.
use libitofin::cashflow::CashFlow;
use libitofin::cashflows::{Coupon, CpiCashFlow, ZeroInflationCashFlow};
use libitofin::errors::QlResult;
use libitofin::event::Event;
use libitofin::indexes::inflationindex::InflationIndex;
use libitofin::patterns::observable::{AsObservable, Observable};
use libitofin::settings::Settings;
use libitofin::time::date::Date;

fn checked_shift(date: Date, lag: libitofin::time::period::Period) -> QlResult<Date> {
    use libitofin::time::timeunit::TimeUnit;
    libitofin::require!(
        (367..=109_574).contains(&date.serial_number()),
        "invalid inflation date"
    );
    libitofin::require!(lag.length() >= 0, "negative inflation observation lag");
    let n = i64::from(lag.length());
    let serial = match lag.units() {
        TimeUnit::Days => i64::from(date.serial_number()) - n,
        TimeUnit::Weeks => i64::from(date.serial_number()) - 7 * n,
        TimeUnit::Months | TimeUnit::Years => {
            let months = if lag.units() == TimeUnit::Years {
                12 * n
            } else {
                n
            };
            let total =
                i64::from(date.year()) * 12 + i64::from(date.month().ordinal()) - 1 - months;
            let year = total.div_euclid(12);
            libitofin::require!(
                (1901..=2199).contains(&year),
                "inflation observation outside date range"
            );
            let year = i32::try_from(year).map_err(|_| {
                libitofin::errors::QlError::new("inflation year overflow", file!(), line!())
            })?;
            let month = libitofin::time::date::Month::from_ordinal(
                i32::try_from(total.rem_euclid(12) + 1).map_err(|_| {
                    libitofin::errors::QlError::new("inflation month overflow", file!(), line!())
                })?,
            );
            let first = Date::new(1, month, year);
            return Ok(Date::new(
                date.day_of_month()
                    .min(Date::end_of_month(first).day_of_month()),
                month,
                year,
            ));
        }
        _ => libitofin::fail!("unsupported inflation observation lag unit"),
    };
    libitofin::require!(
        (367..=109_574).contains(&serial),
        "inflation observation outside date range"
    );
    Ok(Date::from_serial(i32::try_from(serial).map_err(|_| {
        libitofin::errors::QlError::new("inflation serial overflow", file!(), line!())
    })?))
}
fn check_observation(
    index: &libitofin::indexes::ZeroInflationIndex,
    date: Date,
    lag: libitofin::time::period::Period,
    interpolation: libitofin::indexes::inflationindex::CpiInterpolationType,
) -> QlResult<()> {
    use libitofin::indexes::{
        Index,
        inflationindex::{CpiInterpolationType, inflation_period},
    };
    let fixing = checked_shift(date, lag)?;
    if let Some(today) = Index::settings(index).evaluation_date() {
        checked_shift(today, index.availability_lag())?;
    }
    if interpolation == CpiInterpolationType::Linear {
        let (first, last) = inflation_period(date, index.frequency())?;
        if date != first {
            libitofin::require!(
                last.serial_number() < 109_574
                    && inflation_period(fixing, index.frequency())?
                        .1
                        .serial_number()
                        < 109_574,
                "inflation interpolation outside date range"
            );
        }
    }
    Ok(())
}

pub(super) struct FiniteCpiFlow(pub(super) CpiCashFlow);
impl FiniteCpiFlow {
    pub(super) fn base_fixing(&self) -> f64 {
        self.0.base_fixing()
    }
    pub(super) fn index_fixing(&self) -> QlResult<f64> {
        check_observation(
            self.0.cpi_index(),
            self.0.observation_date(),
            self.0.observation_lag(),
            self.0.interpolation(),
        )?;
        self.0.index_fixing()
    }
}
impl AsObservable for FiniteCpiFlow {
    fn observable(&self) -> &Observable {
        self.0.observable()
    }
}
impl Event for FiniteCpiFlow {
    fn date(&self) -> Date {
        self.0.date()
    }
    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        date: Option<Date>,
        include: Option<bool>,
    ) -> QlResult<bool> {
        self.0.has_occurred(settings, date, include)
    }
}
impl CashFlow for FiniteCpiFlow {
    fn amount(&self) -> QlResult<f64> {
        self.index_fixing()?;
        self.0.amount()
    }
    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }
    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}
pub(super) struct FiniteZeroFlow(pub(super) ZeroInflationCashFlow);
impl FiniteZeroFlow {
    pub(super) fn base_fixing(&self) -> QlResult<f64> {
        check_observation(
            self.0.zero_inflation_index(),
            self.0.start_date(),
            self.0.observation_lag(),
            self.0.observation_interpolation(),
        )?;
        let value = self.0.base_fixing()?;
        libitofin::require!(value.is_finite(), "non-finite inflation principal fixing");
        Ok(value)
    }
    pub(super) fn index_fixing(&self) -> QlResult<f64> {
        check_observation(
            self.0.zero_inflation_index(),
            self.0.end_date(),
            self.0.observation_lag(),
            self.0.observation_interpolation(),
        )?;
        let value = self.0.index_fixing()?;
        libitofin::require!(value.is_finite(), "non-finite inflation principal fixing");
        Ok(value)
    }
}
impl AsObservable for FiniteZeroFlow {
    fn observable(&self) -> &Observable {
        self.0.observable()
    }
}
impl Event for FiniteZeroFlow {
    fn date(&self) -> Date {
        self.0.date()
    }
    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        date: Option<Date>,
        include: Option<bool>,
    ) -> QlResult<bool> {
        self.0.has_occurred(settings, date, include)
    }
}
impl CashFlow for FiniteZeroFlow {
    fn amount(&self) -> QlResult<f64> {
        self.base_fixing()?;
        self.index_fixing()?;
        let amount = self.0.amount()?;
        libitofin::require!(amount.is_finite(), "non-finite inflation principal amount");
        Ok(amount)
    }
    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }
    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}
