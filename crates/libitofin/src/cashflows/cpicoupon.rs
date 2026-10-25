//! Explicit-base CPI coupon, plain pricer and indexed principal cash flow.
//!
//! Adapted from QuantLib `ql/cashflows/cpicoupon` and `cpicouponpricer`,
//! revision `9863b578af0caa4cecabf697196533e84a8308b6`.
//! Copyright (C) 2009, 2011 Chris Kenyon; Copyright (C) 2022 Quaternion Risk Management Ltd.
//! See `THIRD_PARTY_NOTICES.md` and `QUANTLIB_LICENSE.txt`.

use super::{Coupon, CouponBase};
use crate::cashflow::CashFlow;
use crate::errors::QlResult;
use crate::event::Event;
use crate::handle::Handle;
use crate::indexes::Index;
use crate::indexes::inflationindex::{
    Cpi, CpiInterpolationType, InflationIndex, ZeroInflationIndex, inflation_period,
};
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::shared::{Shared, SharedMut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::{
    date::{Date, Month},
    daycounter::DayCounter,
    period::Period,
    timeunit::TimeUnit,
};
use crate::{require, types::Real};

pub(crate) fn finite_cpi(value: Real) -> QlResult<Real> {
    require!(value.is_finite(), "non-finite CPI result");
    Ok(value)
}

pub(crate) fn check_base_cpi(base: Real) -> QlResult<()> {
    require!(base.is_finite() && base.abs() > 1e-16, "invalid base CPI");
    Ok(())
}

pub(crate) fn checked_cpi_shift(date: Date, lag: Period) -> QlResult<Date> {
    checked_cpi_offset(date, lag, false)
}
pub(crate) fn checked_cpi_offset(date: Date, lag: Period, forward: bool) -> QlResult<Date> {
    require!(
        (367..=109574).contains(&date.serial_number()),
        "invalid CPI date"
    );
    require!(lag.length() >= 0, "negative CPI observation lag");
    let n = i64::from(lag.length()) * if forward { -1 } else { 1 };
    let serial = match lag.units() {
        TimeUnit::Days => i64::from(date.serial_number()) - n,
        TimeUnit::Weeks => i64::from(date.serial_number()) - 7 * n,
        TimeUnit::Months | TimeUnit::Years => {
            let months = if lag.units() == TimeUnit::Years {
                n * 12
            } else {
                n
            };
            let target =
                i64::from(date.year()) * 12 + i64::from(date.month().ordinal()) - 1 - months;
            let year = target.div_euclid(12);
            require!(
                (1901..=2199).contains(&year),
                "CPI observation outside date range"
            );
            let year = i32::try_from(year)
                .map_err(|_| crate::errors::QlError::new("CPI year overflow", file!(), line!()))?;
            let month =
                Month::from_ordinal(i32::try_from(target.rem_euclid(12) + 1).map_err(|_| {
                    crate::errors::QlError::new("CPI month overflow", file!(), line!())
                })?);
            let first = Date::new(1, month, year);
            let day = date
                .day_of_month()
                .min(Date::end_of_month(first).day_of_month());
            return Ok(Date::new(day, month, year));
        }
        _ => crate::fail!("unsupported CPI lag unit"),
    };
    require!(
        (367..=109574).contains(&serial),
        "CPI observation outside date range"
    );
    Ok(Date::from_serial(i32::try_from(serial).map_err(|_| {
        crate::errors::QlError::new("CPI serial overflow", file!(), line!())
    })?))
}

pub(crate) fn observed_cpi(
    index: &ZeroInflationIndex,
    date: Date,
    lag: Period,
    interpolation: CpiInterpolationType,
) -> QlResult<Real> {
    let fixing = checked_cpi_shift(date, lag)?;
    if let Some(today) = Index::settings(index).evaluation_date() {
        checked_cpi_shift(today, index.availability_lag())?;
    }
    let period = inflation_period(fixing, index.frequency())?;
    let observation_period = inflation_period(date, index.frequency())?;
    if interpolation == CpiInterpolationType::Linear && date != observation_period.0 {
        require!(
            period.1.serial_number() < 109574 && observation_period.1.serial_number() < 109574,
            "CPI interpolation outside date range"
        );
    }
    finite_cpi(Cpi::lagged_fixing(index, date, lag, interpolation)?)
}

/// Plain CPI swaplet pricer without convexity or optionlet adjustments.
#[derive(Clone)]
pub struct CpiCouponPricer {
    nominal_curve: Handle<dyn YieldTermStructure>,
}

impl Default for CpiCouponPricer {
    fn default() -> Self {
        Self::new(Handle::empty())
    }
}
impl CpiCouponPricer {
    /// Construct an explicit-base CPI object from the supplied terms.
    pub fn new(nominal_curve: Handle<dyn YieldTermStructure>) -> Self {
        Self { nominal_curve }
    }
    /// Return the CPI-scaled fixed rate observed at `date`.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn accrued_rate(&self, coupon: &CpiCoupon, date: Date) -> QlResult<Real> {
        finite_cpi(coupon.fixed_rate * coupon.index_ratio(date)?)
    }
    /// Return the rate observed at the coupon accrual end.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn swaplet_rate(&self, coupon: &CpiCoupon) -> QlResult<Real> {
        self.accrued_rate(coupon, coupon.accrual_end_date())
    }
    /// Discounted swaplet value per unit nominal.
    pub fn swaplet_price(&self, coupon: &CpiCoupon) -> QlResult<Real> {
        let curve = self.nominal_curve.current_link()?;
        let payment = coupon.base.payment_date();
        let discount = if payment > curve.reference_date()? {
            curve.discount_date(payment, false)?
        } else {
            1.0
        };
        finite_cpi(self.swaplet_rate(coupon)? * coupon.accrual_period() * discount)
    }
}

/// CPI-scaled fixed-rate coupon with an explicit nonzero base CPI.
pub struct CpiCoupon {
    base: CouponBase,
    index: Shared<ZeroInflationIndex>,
    base_cpi: Real,
    base_date: Option<Date>,
    lag: Period,
    interpolation: CpiInterpolationType,
    day_counter: DayCounter,
    fixed_rate: Real,
    pricer: std::cell::RefCell<Option<Shared<CpiCouponPricer>>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl CpiCoupon {
    #[allow(clippy::too_many_arguments)]
    /// Construct an explicit-base CPI object from the supplied terms.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn new(
        base_cpi: Real,
        payment: Date,
        nominal: Real,
        start: Date,
        end: Date,
        index: Shared<ZeroInflationIndex>,
        lag: Period,
        interpolation: CpiInterpolationType,
        day_counter: DayCounter,
        fixed_rate: Real,
        reference_start: Option<Date>,
        reference_end: Option<Date>,
        ex_coupon: Option<Date>,
    ) -> QlResult<Self> {
        check_base_cpi(base_cpi)?;
        finite_cpi(nominal)?;
        finite_cpi(fixed_rate)?;
        require!(
            start < end && payment.serial_number() >= 367,
            "invalid CPI coupon dates"
        );
        for date in [reference_start, reference_end, ex_coupon]
            .into_iter()
            .flatten()
        {
            require!(
                (367..=109574).contains(&date.serial_number()),
                "invalid CPI reference or ex-coupon date"
            );
        }
        require!(
            reference_start.unwrap_or(start) < reference_end.unwrap_or(end),
            "invalid CPI reference period"
        );
        checked_cpi_shift(start, lag)?;
        checked_cpi_shift(end, lag)?;
        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        Index::observable(&*index).register_observer(&observer);
        Index::settings(&*index).register_eval_date_observer(&observer);
        Ok(Self {
            base: CouponBase::new(
                payment,
                nominal,
                start,
                end,
                reference_start,
                reference_end,
                ex_coupon,
            ),
            index,
            base_cpi,
            base_date: None,
            lag,
            interpolation,
            day_counter,
            fixed_rate,
            pricer: std::cell::RefCell::new(None),
            observable,
            forwarder,
        })
    }
    /// Construct a coupon whose base CPI is read lazily from `base_date`.
    ///
    /// Native coupon observation uses `base_date + lag` followed by the lagged
    /// fixing, including month-end and Linear interpolation conventions.
    /// # Errors
    /// Rejects invalid terms and checked date arithmetic, without reading fixings.
    #[allow(clippy::too_many_arguments)]
    pub fn from_base_date(
        base_date: Date,
        payment: Date,
        nominal: Real,
        start: Date,
        end: Date,
        index: Shared<ZeroInflationIndex>,
        lag: Period,
        interpolation: CpiInterpolationType,
        day_counter: DayCounter,
        fixed_rate: Real,
        reference_start: Option<Date>,
        reference_end: Option<Date>,
        ex_coupon: Option<Date>,
    ) -> QlResult<Self> {
        checked_cpi_offset(base_date, lag, true)?;
        let mut coupon = Self::new(
            f64::MAX,
            payment,
            nominal,
            start,
            end,
            index,
            lag,
            interpolation,
            day_counter,
            fixed_rate,
            reference_start,
            reference_end,
            ex_coupon,
        )?;
        coupon.base_date = Some(base_date);
        Ok(coupon)
    }
    /// Return the optional live base observation date.
    pub fn base_date(&self) -> Option<Date> {
        self.base_date
    }
    /// Return the effective CPI denominator, reading a date-based value live.
    ///
    /// # Errors
    /// Propagates missing fixings, curve errors and invalid observed CPI values.
    pub fn base_fixing(&self) -> QlResult<Real> {
        let value = match self.base_date {
            Some(date) => observed_cpi(
                &self.index,
                checked_cpi_offset(date, self.lag, true)?,
                self.lag,
                self.interpolation,
            )?,
            None => self.base_cpi,
        };
        check_base_cpi(value)?;
        Ok(value)
    }
    /// Return the explicitly supplied CPI denominator.
    ///
    /// Date-based objects return the native `f64::MAX` null sentinel, not an
    /// observed CPI level. Use `base_fixing` for their fallible live denominator.
    pub fn base_cpi(&self) -> Real {
        self.base_cpi
    }
    /// Return the contractual unscaled fixed rate.
    pub fn fixed_rate(&self) -> Real {
        self.fixed_rate
    }
    /// Return the retained live CPI index.
    pub fn cpi_index(&self) -> &Shared<ZeroInflationIndex> {
        &self.index
    }
    /// Return the observation lag.
    pub fn observation_lag(&self) -> Period {
        self.lag
    }
    /// Return the CPI observation interpolation rule.
    pub fn observation_interpolation(&self) -> CpiInterpolationType {
        self.interpolation
    }
    /// Return reference-period end minus observation lag.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn fixing_date(&self) -> QlResult<Date> {
        checked_cpi_shift(self.reference_period_end(), self.lag)
    }
    /// Return the lagged CPI level at the contract observation date.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn index_fixing(&self) -> QlResult<Real> {
        observed_cpi(
            &self.index,
            self.accrual_end_date(),
            self.lag,
            self.interpolation,
        )
    }
    /// Return the lagged CPI level at `date` divided by its effective base CPI.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn index_ratio(&self, date: Date) -> QlResult<Real> {
        let base = self.base_fixing()?;
        finite_cpi(observed_cpi(&self.index, date, self.lag, self.interpolation)? / base)
    }
    /// Attach and retain the plain swaplet pricer, notifying observers.
    pub fn set_pricer(&self, pricer: Shared<CpiCouponPricer>) {
        let observer = self.forwarder.clone() as SharedMut<dyn Observer>;
        if let Some(old) = self.pricer.borrow().as_ref() {
            old.nominal_curve.unregister_observer(&observer);
        }
        pricer.nominal_curve.register_observer(&observer);
        *self.pricer.borrow_mut() = Some(pricer);
        self.observable.notify_observers();
    }
    fn pricer(&self) -> QlResult<Shared<CpiCouponPricer>> {
        self.pricer
            .borrow()
            .clone()
            .ok_or_else(|| crate::errors::QlError::new("CPI pricer not set", file!(), line!()))
    }
}
impl AsObservable for CpiCoupon {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}
impl Coupon for CpiCoupon {
    fn coupon_base(&self) -> &CouponBase {
        &self.base
    }
    fn amount(&self) -> QlResult<Real> {
        finite_cpi(self.nominal() * self.rate()? * self.accrual_period())
    }
    fn rate(&self) -> QlResult<Real> {
        self.pricer()?.swaplet_rate(self)
    }
    fn day_counter(&self) -> DayCounter {
        self.day_counter.clone()
    }
    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        if date <= self.accrual_start_date() || date > self.base.payment_date() {
            return Ok(0.0);
        }
        finite_cpi(
            self.nominal() * self.pricer()?.accrued_rate(self, date)? * self.accrued_period(date),
        )
    }
}

/// Explicit-base CPI indexed principal, optionally paying growth only.
pub struct CpiCashFlow {
    nominal: Real,
    base_cpi: Real,
    index: Shared<ZeroInflationIndex>,
    observation_date: Date,
    lag: Period,
    interpolation: CpiInterpolationType,
    payment: Date,
    growth_only: bool,
    observable: Shared<Observable>,
    _forwarder: SharedMut<ResetThenNotify>,
}
impl CpiCashFlow {
    #[allow(clippy::too_many_arguments)]
    /// Construct an explicit-base CPI object from the supplied terms.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn new(
        nominal: Real,
        index: Shared<ZeroInflationIndex>,
        base_cpi: Real,
        observation_date: Date,
        lag: Period,
        interpolation: CpiInterpolationType,
        payment: Date,
        growth_only: bool,
    ) -> QlResult<Self> {
        finite_cpi(nominal)?;
        check_base_cpi(base_cpi)?;
        checked_cpi_shift(observation_date, lag)?;
        require!(payment.serial_number() >= 367, "invalid CPI payment date");
        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        Index::observable(&*index).register_observer(&observer);
        Index::settings(&*index).register_eval_date_observer(&observer);
        Ok(Self {
            nominal,
            base_cpi,
            index,
            observation_date,
            lag,
            interpolation,
            payment,
            growth_only,
            observable,
            _forwarder: forwarder,
        })
    }
    /// Return the lagged CPI level at the contract observation date.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn index_fixing(&self) -> QlResult<Real> {
        observed_cpi(
            &self.index,
            self.observation_date,
            self.lag,
            self.interpolation,
        )
    }
    /// Return the explicit CPI denominator.
    pub fn base_fixing(&self) -> Real {
        self.base_cpi
    }
    /// Return the unadjusted contract observation date.
    pub fn observation_date(&self) -> Date {
        self.observation_date
    }
    /// Return the observation lag.
    pub fn observation_lag(&self) -> Period {
        self.lag
    }
    /// Return the CPI observation interpolation rule.
    pub fn interpolation(&self) -> CpiInterpolationType {
        self.interpolation
    }
    /// Return the retained live CPI index.
    pub fn cpi_index(&self) -> &Shared<ZeroInflationIndex> {
        &self.index
    }
    /// Return whether uninflated principal is subtracted.
    pub fn growth_only(&self) -> bool {
        self.growth_only
    }
}
impl AsObservable for CpiCashFlow {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}
impl Event for CpiCashFlow {
    fn date(&self) -> Date {
        self.payment
    }
    fn has_occurred(
        &self,
        settings: &crate::settings::Settings<Date>,
        reference: Option<Date>,
        include: Option<bool>,
    ) -> QlResult<bool> {
        crate::cashflow::cash_flow_has_occurred(self.payment, settings, reference, include)
    }
}
impl CashFlow for CpiCashFlow {
    fn amount(&self) -> QlResult<Real> {
        finite_cpi(
            self.nominal
                * (self.index_fixing()? / self.base_cpi - if self.growth_only { 1.0 } else { 0.0 }),
        )
    }
    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }
    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}

#[cfg(test)]
#[path = "cpicoupon_tests.rs"]
mod tests;
