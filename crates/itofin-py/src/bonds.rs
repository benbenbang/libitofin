//! Fixed and zero-coupon bonds sharing a retained, lazy bond base.

use crate::cashflows::{CashFlowGuard, PyLeg};
use crate::curve::PyYieldTermStructure;
use crate::helpers::PyBondPriceType;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyCompounding, PyDate, PyDayCounter, PyFrequency,
    PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::cashflows::CashFlows;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    Bond, BondPrice, CallableFixedRateBond, CallableZeroCouponBond, FixedRateBond, ZeroCouponBond,
};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::{BondFunctions, DiscountingBondEngine};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::date::Date;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{
    gen_methods_from_python, gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods,
};
use pyo3_stub_gen::inventory::submit;
use std::cell::{Cell, Ref};

pub(crate) fn finite(value: f64) -> PyResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ItofinError::new_err("bond amount must be finite"))
    }
}
pub(crate) fn analytical_date(date: Option<&PyDate>) -> PyResult<Option<Date>> {
    date.map(|date| {
        let value = date.inner();
        if value == Date::null() {
            Err(ItofinError::new_err("null analytical date"))
        } else {
            Ok(value)
        }
    })
    .transpose()
}
pub(crate) fn analytical_solver(accuracy: f64, max_evaluations: usize, guess: f64) -> PyResult<()> {
    finite(accuracy)?;
    finite(guess)?;
    if accuracy <= 0.0 || max_evaluations == 0 {
        return Err(ItofinError::new_err(
            "accuracy and maximum evaluations must be positive",
        ));
    }
    Ok(())
}
fn analytical_price(price: f64, price_type: PyBondPriceType) -> BondPrice {
    match price_type {
        PyBondPriceType::Clean => BondPrice::Clean(price),
        PyBondPriceType::Dirty => BondPrice::Dirty(price),
    }
}
pub(crate) fn positive(value: f64) -> PyResult<()> {
    finite(value)?;
    if value <= 0.0 {
        return Err(ItofinError::new_err(
            "face amount and redemption must be positive",
        ));
    }
    Ok(())
}
pub(crate) fn step(date: Date, days: i64) -> PyResult<Date> {
    let serial = i64::from(date.serial_number()) + days;
    if !(i64::from(Date::min_date().serial_number())..=i64::from(Date::max_date().serial_number()))
        .contains(&serial)
    {
        return Err(ItofinError::new_err("bond date outside supported range"));
    }
    Ok(Date::from_serial(serial as i32))
}
fn business(calendar: &PyCalendar, date: Date) -> PyResult<bool> {
    calendar.checked(&PyDate::from_inner(date), "price a bond")?;
    Ok(calendar.inner().is_business_day(date))
}
pub(crate) fn adjust(calendar: &PyCalendar, date: Date, convention: Bdc) -> PyResult<Date> {
    if date == Date::null() {
        return Err(ItofinError::new_err("null bond date"));
    }
    if convention == Bdc::Unadjusted {
        return Ok(date);
    }
    let direction = match convention {
        Bdc::Preceding | Bdc::ModifiedPreceding => -1,
        _ => 1,
    };
    let mut result = date;
    if convention == Bdc::Nearest {
        let mut previous = date;
        while !business(calendar, result)? && !business(calendar, previous)? {
            result = step(result, 1)?;
            previous = step(previous, -1)?;
        }
        return if business(calendar, result)? {
            Ok(result)
        } else {
            Ok(previous)
        };
    }
    while !business(calendar, result)? {
        result = step(result, direction)?;
    }
    if ((convention == Bdc::ModifiedFollowing
        || convention == Bdc::HalfMonthModifiedFollowing
        || convention == Bdc::ModifiedPreceding)
        && result.month() != date.month())
        || (convention == Bdc::HalfMonthModifiedFollowing
            && date.day_of_month() <= 15
            && result.day_of_month() > 15)
    {
        return adjust(
            calendar,
            date,
            if direction > 0 {
                Bdc::Preceding
            } else {
                Bdc::Following
            },
        );
    }
    Ok(result)
}
pub(crate) fn advance(
    calendar: &PyCalendar,
    date: Date,
    n: i64,
    unit: TimeUnit,
    convention: Bdc,
    eom: bool,
) -> PyResult<Date> {
    if n == 0 {
        return adjust(calendar, date, convention);
    }
    if unit == TimeUnit::Days {
        let mut result = date;
        for _ in 0..n.unsigned_abs() {
            result = step(result, n.signum())?;
            while !business(calendar, result)? {
                result = step(result, n.signum())?;
            }
        }
        return Ok(result);
    }
    let shifted = match unit {
        TimeUnit::Weeks => step(
            date,
            n.checked_mul(7)
                .ok_or_else(|| ItofinError::new_err("bond period overflow"))?,
        )?,
        TimeUnit::Months | TimeUnit::Years => {
            let months = if unit == TimeUnit::Years {
                n.checked_mul(12)
            } else {
                Some(n)
            }
            .ok_or_else(|| ItofinError::new_err("bond period overflow"))?;
            let year = (i64::from(date.year()) * 12 + i64::from(date.month().ordinal()) - 1
                + months)
                .div_euclid(12);
            if !(1901..=2199).contains(&year) {
                return Err(ItofinError::new_err("bond period outside supported years"));
            }
            date + Period::new(
                i32::try_from(n).map_err(|_| ItofinError::new_err("bond period overflow"))?,
                unit,
            )
        }
        _ => return Err(ItofinError::new_err("unsupported bond period unit")),
    };
    if eom && (unit == TimeUnit::Months || unit == TimeUnit::Years) {
        if convention == Bdc::Unadjusted {
            if Date::is_end_of_month(date) {
                return Ok(Date::end_of_month(shifted));
            }
        } else if date >= adjust(calendar, Date::end_of_month(date), Bdc::Preceding)? {
            return adjust(calendar, Date::end_of_month(shifted), Bdc::Preceding);
        }
    }
    adjust(calendar, shifted, convention)
}

pub(crate) enum BondBacking {
    Plain(SharedMut<Bond>),
    CallableFixed(SharedMut<CallableFixedRateBond>),
    CallableZero(SharedMut<CallableZeroCouponBond>),
}
impl BondBacking {
    fn borrow(&self) -> Ref<'_, Bond> {
        match self {
            Self::Plain(bond) => bond.borrow(),
            Self::CallableFixed(bond) => Ref::map(bond.borrow(), |bond| bond.bond()),
            Self::CallableZero(bond) => Ref::map(bond.borrow(), |bond| bond.bond()),
        }
    }
    fn instrument(&self) -> Ref<'_, dyn Instrument> {
        match self {
            Self::Plain(bond) => Ref::map(bond.borrow(), |bond| bond as &dyn Instrument),
            Self::CallableFixed(bond) => Ref::map(bond.borrow(), |bond| bond as &dyn Instrument),
            Self::CallableZero(bond) => Ref::map(bond.borrow(), |bond| bond as &dyn Instrument),
        }
    }
    fn with_mut<T>(&self, operation: impl FnOnce(&mut dyn Instrument) -> T) -> T {
        match self {
            Self::Plain(bond) => operation(&mut *bond.borrow_mut()),
            Self::CallableFixed(bond) => operation(&mut *bond.borrow_mut()),
            Self::CallableZero(bond) => operation(&mut *bond.borrow_mut()),
        }
    }
    fn settlement_value(&self) -> PyResult<f64> {
        Ok(match self {
            Self::Plain(bond) => bond.borrow_mut().settlement_value(),
            Self::CallableFixed(bond) => bond.borrow_mut().settlement_value(),
            Self::CallableZero(bond) => bond.borrow_mut().settlement_value(),
        }
        .map_err(PyQlError::from)?)
    }
    fn callable_price(&self, clean: bool) -> PyResult<f64> {
        let value = match self {
            Self::Plain(_) => return Err(ItofinError::new_err("callable bond required")),
            Self::CallableFixed(bond) => {
                let mut bond = bond.borrow_mut();
                if clean {
                    bond.clean_price()
                } else {
                    bond.dirty_price()
                }
            }
            Self::CallableZero(bond) => {
                let mut bond = bond.borrow_mut();
                if clean {
                    bond.clean_price()
                } else {
                    bond.dirty_price()
                }
            }
        };
        finite(value.map_err(PyQlError::from)?)
    }
}

/// Flat-yield duration conventions for ordinary bond cash flows.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "Duration",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.instruments"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyDuration {
    /// Discounted cash-flow-weighted average payment time.
    Simple = 0,
    /// Modified duration scaled by 1 + yield/frequency; requires compounded yield.
    Macaulay = 1,
    /// Negative first yield derivative divided by bond value.
    Modified = 2,
}
impl PyDuration {
    fn inner(self) -> libitofin::cashflows::Duration {
        use libitofin::cashflows::Duration;
        match self {
            Self::Simple => Duration::Simple,
            Self::Macaulay => Duration::Macaulay,
            Self::Modified => Duration::Modified,
        }
    }
}

/// Shared bond interface, constructed only through concrete bond classes.
#[gen_stub_pyclass]
#[pyclass(name = "Bond", subclass, unsendable, module = "itofin.instruments")]
pub struct PyBond {
    indexed: bool,
    inner: BondBacking,
    calendar: PyCalendar,
    flags: (bool, Option<bool>, Option<bool>),
    guard: Option<CashFlowGuard>,
    engine_inputs: Option<(Handle<dyn YieldTermStructure>, Option<bool>)>,
    frozen: Cell<bool>,
}
impl PyBond {
    fn inspection_payment(
        &self,
        settlement: Option<&PyDate>,
        next: bool,
        with_amount: bool,
    ) -> PyResult<(Option<Date>, Option<f64>)> {
        let settlement = analytical_date(settlement)?
            .map_or_else(|| self.settlement_date(None).map(|date| date.inner()), Ok)?;
        let bond = self.inner.borrow();
        self.cashflows().inspection_payment(
            bond.settings(),
            Some(false),
            Some(settlement),
            next,
            with_amount,
        )
    }
    fn require_unindexed_coupon_analytics(&self) -> PyResult<()> {
        if self.indexed {
            Err(ItofinError::new_err(
                "CPI raw-coupon BPS and ATM analytics are unsupported",
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn forward_plain(&self) -> PyResult<SharedMut<Bond>> {
        if self.guard.is_some() || self.indexed {
            return Err(ItofinError::new_err("fixed or zero plain bond required"));
        }
        match &self.inner {
            BondBacking::Plain(bond) => Ok(bond.clone()),
            _ => Err(ItofinError::new_err("fixed or zero plain bond required")),
        }
    }
    pub(crate) fn prepare_forward(&mut self, force: bool) -> PyResult<()> {
        self.prepare(force)
    }
    pub(crate) fn from_inner(inner: Bond, calendar: PyCalendar) -> Self {
        Self::from_backing(BondBacking::Plain(shared_mut(inner)), calendar)
    }
    pub(crate) fn from_indexed(inner: Bond, calendar: PyCalendar) -> Self {
        let mut base = Self::from_inner(inner, calendar);
        base.indexed = true;
        base
    }
    pub(crate) fn from_backing(inner: BondBacking, calendar: PyCalendar) -> Self {
        let bond = inner.borrow();
        let flags = (
            bond.settings().include_reference_date_events(),
            bond.settings().include_todays_cash_flows(),
            None,
        );
        drop(bond);
        Self {
            indexed: false,
            inner,
            calendar,
            flags,
            guard: None,
            engine_inputs: None,
            frozen: Cell::new(false),
        }
    }
    pub(crate) fn from_floating(inner: Bond, calendar: PyCalendar, guard: CashFlowGuard) -> Self {
        let mut base = Self::from_inner(inner, calendar);
        base.flags.2 = Some(base.inner.borrow().settings().using_at_par_coupons());
        base.guard = Some(guard);
        base
    }
    pub(crate) fn callable_probe_settlement(
        &self,
        settlement: Option<&PyDate>,
    ) -> PyResult<Option<Date>> {
        let actual = analytical_date(settlement)?;
        if actual.is_none() {
            self.settlement_date(None)?;
        }
        Ok(actual)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn callable_implied_volatility(
        &self,
        target_price: f64,
        discount_curve: &PyYieldTermStructure,
        price_type: PyBondPriceType,
        accuracy: f64,
        max_evaluations: usize,
        min_vol: f64,
        max_vol: f64,
    ) -> PyResult<f64> {
        self.settlement_date(None)?;
        let target = analytical_price(target_price, price_type);
        let curve = discount_curve.handle();
        match &self.inner {
            BondBacking::CallableFixed(bond) => bond.borrow().implied_volatility(
                target,
                curve,
                accuracy,
                max_evaluations,
                min_vol,
                max_vol,
            ),
            BondBacking::CallableZero(bond) => bond.borrow().implied_volatility(
                target,
                curve,
                accuracy,
                max_evaluations,
                min_vol,
                max_vol,
            ),
            BondBacking::Plain(_) => {
                return Err(ItofinError::new_err(
                    "implied volatility requires a callable bond",
                ));
            }
        }
        .map_err(|error| PyQlError::from(error).into())
    }
    fn coupon_sensitivity_settlement(&self, settlement: Option<&PyDate>) -> PyResult<Date> {
        if !matches!(self.inner, BondBacking::Plain(_)) {
            return Err(ItofinError::new_err("ordinary noncallable bond required"));
        }
        analytical_date(settlement)?
            .map_or_else(|| self.settlement_date(None).map(|date| date.inner()), Ok)
    }
    fn risk_settlement(&self, settlement: Option<&PyDate>) -> PyResult<Date> {
        if !matches!(self.inner, BondBacking::Plain(_)) {
            return Err(ItofinError::new_err("ordinary noncallable bond required"));
        }
        self.analytical_settlement(settlement, false)
    }
    fn analytical_settlement(&self, settlement: Option<&PyDate>, clean: bool) -> PyResult<Date> {
        let settlement = analytical_date(settlement)?
            .map_or_else(|| self.settlement_date(None).map(|date| date.inner()), Ok)?;
        if let Some(guard) = &self.guard {
            let bond = self.inner.borrow();
            for (i, flow) in bond.cashflows().iter().enumerate() {
                if !flow
                    .has_occurred(bond.settings(), Some(settlement), Some(false))
                    .map_err(PyQlError::from)?
                    && !flow
                        .trading_ex_coupon(bond.settings(), Some(settlement))
                        .map_err(PyQlError::from)?
                {
                    guard(Some(i))?;
                }
            }
        }
        if clean {
            self.guard_accrued(settlement)?;
        }
        Ok(settlement)
    }
    fn guard_accrued(&self, date: Date) -> PyResult<()> {
        if let Some(guard) = &self.guard {
            let bond = self.inner.borrow();
            if bond.notional(Some(date)).map_err(PyQlError::from)? != 0.0
                && let Some(index) = CashFlows::next_cash_flow(
                    bond.cashflows(),
                    bond.settings(),
                    Some(false),
                    Some(date),
                )
                .map_err(PyQlError::from)?
            {
                let payment = bond.cashflows()[index].date();
                for (i, flow) in bond
                    .cashflows()
                    .iter()
                    .enumerate()
                    .skip(index)
                    .take_while(|(_, flow)| flow.date() == payment)
                {
                    if let Some(coupon) = flow.as_coupon()
                        && date > coupon.accrual_start_date()
                        && date <= payment
                    {
                        guard(Some(i))?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn refresh_forward(&mut self) {
        let flags = {
            let bond = self.inner.borrow();
            (
                bond.settings().include_reference_date_events(),
                bond.settings().include_todays_cash_flows(),
                self.guard
                    .as_ref()
                    .map(|_| bond.settings().using_at_par_coupons()),
            )
        };
        if flags != self.flags {
            match &self.inner {
                BondBacking::CallableZero(_) => {
                    self.inner.instrument().base();
                }
                BondBacking::Plain(_) | BondBacking::CallableFixed(_) => {
                    self.inner
                        .instrument()
                        .base()
                        .observer()
                        .borrow_mut()
                        .update();
                }
            }
            self.flags = flags;
        }
    }
    pub(crate) fn sensitivity_instrument(&mut self) -> PyResult<SharedMut<dyn Instrument>> {
        self.prepare(false)?;
        Ok(match &self.inner {
            BondBacking::Plain(bond) => SharedMut::clone(bond) as SharedMut<dyn Instrument>,
            BondBacking::CallableFixed(bond) => SharedMut::clone(bond) as SharedMut<dyn Instrument>,
            BondBacking::CallableZero(bond) => SharedMut::clone(bond) as SharedMut<dyn Instrument>,
        })
    }
    fn prepare(&mut self, force: bool) -> PyResult<()> {
        let settlement = self.settlement_date(None)?.inner();
        self.refresh_forward();
        if let (Some(guard), Some((curve, include))) = (&self.guard, &self.engine_inputs) {
            let bond = self.inner.borrow();
            if !(force || !self.frozen.get() && !bond.base().is_calculated())
                || bond.is_expired().map_err(PyQlError::from)?
            {
                return Ok(());
            }
            let reference = curve
                .current_link()
                .map_err(PyQlError::from)?
                .reference_date()
                .map_err(PyQlError::from)?;
            let include =
                include.unwrap_or_else(|| bond.settings().include_reference_date_events());
            for (i, flow) in bond.cashflows().iter().enumerate() {
                let needed = |date, include| -> PyResult<bool> {
                    Ok(!flow
                        .has_occurred(bond.settings(), Some(date), Some(include))
                        .map_err(PyQlError::from)?
                        && !flow
                            .trading_ex_coupon(bond.settings(), Some(date))
                            .map_err(PyQlError::from)?)
                };
                if needed(reference, include)? || needed(settlement, false)? {
                    guard(Some(i))?;
                }
            }
        }
        Ok(())
    }
}
submit! {
    gen_methods_from_python! {
        r#"
        import typing
        class PyBond:
            def __new__(cls, _unconstructible: typing.NoReturn) -> Bond: ...
        "#
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBond {
    #[new]
    #[gen_stub(skip)]
    fn new(_unconstructible: &Bound<'_, PyAny>) -> PyResult<Self> {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Bond is constructed through a concrete bond class",
        ))
    }
    /// Attach an engine sharing this bond's settings. Dependencies remain live.
    fn set_engine(&mut self, engine: &PyDiscountingBondEngine) -> PyResult<()> {
        if !matches!(self.inner, BondBacking::Plain(_)) {
            return Err(ItofinError::new_err(
                "discounting engine requires a noncallable bond",
            ));
        }
        if !Shared::ptr_eq(&self.inner.borrow().settings_handle(), &engine.settings) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        self.engine_inputs = Some((engine.curve.clone(), engine.include));
        self.inner.with_mut(|bond| {
            bond.base_mut().set_pricing_engine(
                SharedMut::clone(&engine.inner) as SharedMut<dyn PricingEngine>,
            );
        });
        Ok(())
    }
    /// Clean analytical quote per 100 using an explicit discount curve.
    #[pyo3(signature=(discount_curve, *, settlement=None))]
    fn clean_price_from_curve(
        &self,
        discount_curve: &PyYieldTermStructure,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        let settlement = self.analytical_settlement(settlement, true)?;
        let curve = discount_curve
            .handle()
            .current_link()
            .map_err(PyQlError::from)?;
        finite(
            BondFunctions::clean_price(&self.inner.borrow(), &*curve, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Clean analytical quote per 100 from a flat yield with supplied conventions.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, settlement=None))]
    fn clean_price_from_yield(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        finite(yield_rate)?;
        let settlement = self.analytical_settlement(settlement, true)?;
        finite(
            BondFunctions::clean_price_from_yield(
                &self.inner.borrow(),
                yield_rate,
                day_counter.inner(),
                compounding.inner(),
                frequency.inner(),
                Some(settlement),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Clean analytical quote per 100 using a parallel zero-rate spread.
    #[pyo3(signature=(discount_curve, z_spread, compounding, frequency, *, settlement=None))]
    fn clean_price_at_z_spread(
        &self,
        discount_curve: &PyYieldTermStructure,
        z_spread: f64,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        finite(z_spread)?;
        let settlement = self.analytical_settlement(settlement, true)?;
        finite(
            BondFunctions::clean_price_at_z_spread(
                &self.inner.borrow(),
                discount_curve.handle(),
                z_spread,
                compounding.inner(),
                frequency.inner(),
                Some(settlement),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Dirty analytical quote per 100 using an explicit discount curve.
    #[pyo3(signature=(discount_curve, *, settlement=None))]
    fn dirty_price_from_curve(
        &self,
        discount_curve: &PyYieldTermStructure,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        let settlement = self.analytical_settlement(settlement, false)?;
        let curve = discount_curve
            .handle()
            .current_link()
            .map_err(PyQlError::from)?;
        finite(
            BondFunctions::dirty_price(&self.inner.borrow(), &*curve, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Dirty analytical quote per 100 from a flat yield with supplied conventions.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, settlement=None))]
    fn dirty_price_from_yield(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        finite(yield_rate)?;
        let settlement = self.analytical_settlement(settlement, false)?;
        finite(
            BondFunctions::dirty_price_from_yield(
                &self.inner.borrow(),
                yield_rate,
                day_counter.inner(),
                compounding.inner(),
                frequency.inner(),
                Some(settlement),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Dirty analytical quote per 100 using a parallel zero-rate spread.
    #[pyo3(signature=(discount_curve, z_spread, compounding, frequency, *, settlement=None))]
    fn dirty_price_at_z_spread(
        &self,
        discount_curve: &PyYieldTermStructure,
        z_spread: f64,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        finite(z_spread)?;
        let settlement = self.analytical_settlement(settlement, false)?;
        finite(
            BondFunctions::dirty_price_at_z_spread(
                &self.inner.borrow(),
                discount_curve.handle(),
                z_spread,
                compounding.inner(),
                frequency.inner(),
                Some(settlement),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Coupon-rate sensitivity per 100, not signed yield BPV. Coupon amounts are not read.
    #[pyo3(signature=(discount_curve, *, settlement=None))]
    fn bps_from_curve(
        &self,
        discount_curve: &PyYieldTermStructure,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        self.require_unindexed_coupon_analytics()?;
        let settlement = self.coupon_sensitivity_settlement(settlement)?;
        let curve = discount_curve
            .handle()
            .current_link()
            .map_err(PyQlError::from)?;
        finite(
            BondFunctions::bps(&self.inner.borrow(), &*curve, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Coupon-rate BPS per 100 under flat-yield discounting, not signed yield BPV.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, settlement=None))]
    fn bps_from_yield(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        self.require_unindexed_coupon_analytics()?;
        let rate = libitofin::interestrate::InterestRate::new(
            finite(yield_rate)?,
            day_counter.inner(),
            compounding.inner(),
            frequency.inner(),
        )
        .map_err(PyQlError::from)?;
        let settlement = self.coupon_sensitivity_settlement(settlement)?;
        finite(
            BondFunctions::bps_at_yield(&self.inner.borrow(), &rate, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// The coupon rate repricing the bond's own curve NPV, not a par-price target.
    #[pyo3(signature=(discount_curve, *, settlement=None))]
    fn atm_rate_from_curve(
        &self,
        discount_curve: &PyYieldTermStructure,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        self.require_unindexed_coupon_analytics()?;
        let settlement = self.risk_settlement(settlement)?;
        let curve = discount_curve
            .handle()
            .current_link()
            .map_err(PyQlError::from)?;
        finite(
            BondFunctions::atm_rate(&self.inner.borrow(), &*curve, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Coupon rate repricing an explicit per-100 clean or dirty quote at settlement.
    /// Uses current notional and live coupon amounts; the own-NPV ATM API is unchanged.
    #[pyo3(signature=(discount_curve, price, *, price_type=PyBondPriceType::Clean, settlement=None))]
    fn atm_rate_from_price(
        &self,
        discount_curve: &PyYieldTermStructure,
        price: f64,
        price_type: PyBondPriceType,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        self.require_unindexed_coupon_analytics()?;
        if !matches!(self.inner, BondBacking::Plain(_)) {
            return Err(ItofinError::new_err("ordinary noncallable bond required"));
        }
        let price = analytical_price(finite(price)?, price_type);
        let settlement =
            self.analytical_settlement(settlement, matches!(price, BondPrice::Clean(_)))?;
        let curve = discount_curve
            .handle()
            .current_link()
            .map_err(PyQlError::from)?;
        finite(
            BondFunctions::atm_rate_from_price(
                &self.inner.borrow(),
                &*curve,
                price,
                Some(settlement),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Flat-yield duration, independent of pricing engines. Macaulay requires compounded yield.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, duration_type=PyDuration::Modified, settlement=None))]
    fn duration(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        duration_type: PyDuration,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        let rate = libitofin::interestrate::InterestRate::new(
            finite(yield_rate)?,
            day_counter.inner(),
            compounding.inner(),
            frequency.inner(),
        )
        .map_err(PyQlError::from)?;
        let settlement = self.risk_settlement(settlement)?;
        finite(
            BondFunctions::duration(
                &self.inner.borrow(),
                &rate,
                duration_type.inner(),
                Some(settlement),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Flat-yield convexity of ordinary bond cash flows.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, settlement=None))]
    fn convexity(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        let rate = libitofin::interestrate::InterestRate::new(
            finite(yield_rate)?,
            day_counter.inner(),
            compounding.inner(),
            frequency.inner(),
        )
        .map_err(PyQlError::from)?;
        let settlement = self.risk_settlement(settlement)?;
        finite(
            BondFunctions::convexity(&self.inner.borrow(), &rate, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Signed currency value for a one-basis-point yield rise, not a per-100 quote.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, settlement=None))]
    fn basis_point_value(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        let rate = libitofin::interestrate::InterestRate::new(
            finite(yield_rate)?,
            day_counter.inner(),
            compounding.inner(),
            frequency.inner(),
        )
        .map_err(PyQlError::from)?;
        let settlement = self.risk_settlement(settlement)?;
        finite(
            BondFunctions::basis_point_value(&self.inner.borrow(), &rate, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Yield change for a one-cent currency value increase, not a per-100 quote.
    #[pyo3(signature=(yield_rate, day_counter, compounding, frequency, *, settlement=None))]
    fn yield_value_basis_point(
        &self,
        yield_rate: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        let rate = libitofin::interestrate::InterestRate::new(
            finite(yield_rate)?,
            day_counter.inner(),
            compounding.inner(),
            frequency.inner(),
        )
        .map_err(PyQlError::from)?;
        let settlement = self.risk_settlement(settlement)?;
        finite(
            BondFunctions::yield_value_basis_point(&self.inner.borrow(), &rate, Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Solve the yield repricing a clean or dirty per-100 quote.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(price, day_counter, compounding, frequency, *, price_type=PyBondPriceType::Clean, settlement=None, accuracy=1e-10, max_evaluations=100, guess=0.05))]
    fn yield_rate(
        &self,
        price: f64,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        price_type: PyBondPriceType,
        settlement: Option<&PyDate>,
        accuracy: f64,
        max_evaluations: usize,
        guess: f64,
    ) -> PyResult<f64> {
        finite(price)?;
        analytical_solver(accuracy, max_evaluations, guess)?;
        let settlement =
            self.analytical_settlement(settlement, matches!(price_type, PyBondPriceType::Clean))?;
        finite(
            BondFunctions::yield_rate(
                &self.inner.borrow(),
                analytical_price(price, price_type),
                day_counter.inner(),
                compounding.inner(),
                frequency.inner(),
                Some(settlement),
                Some(accuracy),
                Some(max_evaluations),
                Some(guess),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Solve the parallel zero-rate spread repricing a clean or dirty per-100 quote.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(price, discount_curve, compounding, frequency, *, price_type=PyBondPriceType::Clean, settlement=None, accuracy=1e-10, max_evaluations=100, guess=0.0))]
    fn z_spread(
        &self,
        price: f64,
        discount_curve: &PyYieldTermStructure,
        compounding: PyCompounding,
        frequency: &PyFrequency,
        price_type: PyBondPriceType,
        settlement: Option<&PyDate>,
        accuracy: f64,
        max_evaluations: usize,
        guess: f64,
    ) -> PyResult<f64> {
        finite(price)?;
        analytical_solver(accuracy, max_evaluations, guess)?;
        let settlement =
            self.analytical_settlement(settlement, matches!(price_type, PyBondPriceType::Clean))?;
        finite(
            BondFunctions::z_spread(
                &self.inner.borrow(),
                analytical_price(price, price_type),
                discount_curve.handle(),
                compounding.inner(),
                frequency.inner(),
                Some(settlement),
                Some(accuracy),
                Some(max_evaluations),
                Some(guess),
            )
            .map_err(PyQlError::from)?,
        )
    }
    /// Present value in currency units, discounted to the curve reference date.
    fn npv(&mut self) -> PyResult<f64> {
        self.prepare(false)?;
        finite(
            self.inner
                .with_mut(|bond| bond.npv())
                .map_err(PyQlError::from)?,
        )
    }
    /// Dirty currency value at settlement; the today-flow flag can override exclusion.
    fn settlement_value(&mut self) -> PyResult<f64> {
        self.prepare(false)?;
        finite(self.inner.settlement_value()?)
    }
    /// Settlement price per 100 of current notional.
    fn dirty_price(&mut self) -> PyResult<f64> {
        if !matches!(self.inner, BondBacking::Plain(_)) {
            self.prepare(false)?;
            return self.inner.callable_price(false);
        }
        let settlement = self.settlement_date(None)?.inner();
        let notional = self
            .inner
            .borrow()
            .notional(Some(settlement))
            .map_err(PyQlError::from)?;
        if notional == 0.0 {
            return Ok(0.0);
        }
        self.prepare(false)?;
        let value = self.inner.settlement_value()?;
        let scaled = value * 100.0;
        finite(if scaled.is_finite() {
            scaled / notional
        } else {
            value / notional * 100.0
        })
    }
    /// Dirty quote less accrued interest, per 100 of current notional.
    fn clean_price(&mut self) -> PyResult<f64> {
        if !matches!(self.inner, BondBacking::Plain(_)) {
            self.prepare(false)?;
            return self.inner.callable_price(true);
        }
        finite(self.dirty_price()? - self.accrued_amount(None)?)
    }
    /// Accrued interest per 100; omitted date uses settlement.
    #[pyo3(signature = (date=None))]
    fn accrued_amount(&self, date: Option<&PyDate>) -> PyResult<f64> {
        let date = match date {
            Some(date) => {
                if date.inner() == Date::null() {
                    return Err(ItofinError::new_err("null accrual date"));
                }
                date.inner()
            }
            None => self.settlement_date(None)?.inner(),
        };
        self.guard_accrued(date)?;
        finite(
            self.inner
                .borrow()
                .accrued_amount(Some(date))
                .map_err(PyQlError::from)?,
        )
    }
    /// Outstanding notional on date; omitted date uses settlement.
    #[pyo3(signature = (date=None))]
    fn notional(&self, date: Option<&PyDate>) -> PyResult<f64> {
        let date = match date {
            Some(date) => date.inner(),
            None => self.settlement_date(None)?.inner(),
        };
        if date == Date::null() {
            return Err(ItofinError::new_err("null notional date"));
        }
        finite(
            self.inner
                .borrow()
                .notional(Some(date))
                .map_err(PyQlError::from)?,
        )
    }
    /// Calendar-adjusted settlement, floored at the issue date.
    #[pyo3(signature = (date=None))]
    pub(crate) fn settlement_date(&self, date: Option<&PyDate>) -> PyResult<PyDate> {
        let bond = self.inner.borrow();
        let date = date
            .map(PyDate::inner)
            .or_else(|| bond.settings().evaluation_date())
            .ok_or_else(|| ItofinError::new_err("no evaluation date set"))?;
        let settled = advance(
            &self.calendar,
            date,
            i64::from(bond.settlement_days()),
            TimeUnit::Days,
            Bdc::Following,
            false,
        )?;
        Ok(PyDate::from_inner(
            bond.issue_date()
                .map_or(settled, |issue| settled.max(issue)),
        ))
    }
    fn settlement_days(&self) -> u32 {
        self.inner.borrow().settlement_days()
    }
    fn calendar(&self) -> PyCalendar {
        self.calendar.clone()
    }
    fn maturity_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .borrow()
                .maturity_date()
                .map_err(PyQlError::from)?,
        ))
    }
    fn issue_date(&self) -> Option<PyDate> {
        self.inner.borrow().issue_date().map(PyDate::from_inner)
    }
    fn cashflows(&self) -> PyLeg {
        let bond = self.inner.borrow();
        if self.indexed {
            PyLeg::with_cpi_settings(bond.cashflows().clone(), bond.settings_handle())
        } else {
            PyLeg::with_guard(bond.cashflows().clone(), self.guard.clone())
        }
    }
    fn redemptions(&self) -> PyLeg {
        if self.indexed {
            let bond = self.inner.borrow();
            return PyLeg::with_cpi_settings(bond.redemptions().clone(), bond.settings_handle());
        }
        let guard = self
            .guard
            .as_ref()
            .map(|_| libitofin::shared::shared(|_: Option<usize>| Ok(())) as CashFlowGuard);
        PyLeg::with_guard(self.inner.borrow().redemptions().clone(), guard)
    }
    /// Earliest coupon accrual start or non-coupon payment date; needs no engine.
    fn start_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            CashFlows::start_date(self.inner.borrow().cashflows()).map_err(PyQlError::from)?,
        ))
    }
    /// Whether outstanding notional is nonzero at settlement; omitted date uses bond settlement.
    #[pyo3(signature=(settlement=None))]
    fn is_tradable(&self, settlement: Option<&PyDate>) -> PyResult<bool> {
        let settlement = analytical_date(settlement)?
            .map_or_else(|| self.settlement_date(None).map(|date| date.inner()), Ok)?;
        Ok(
            BondFunctions::is_tradable(&self.inner.borrow(), Some(settlement))
                .map_err(PyQlError::from)?,
        )
    }
    /// Previous payment date, or None; excludes settlement flows unless today Settings override.
    #[pyo3(signature=(settlement=None))]
    fn previous_cash_flow_date(&self, settlement: Option<&PyDate>) -> PyResult<Option<PyDate>> {
        Ok(self
            .inspection_payment(settlement, false, false)?
            .0
            .map(PyDate::from_inner))
    }
    /// Next payment date, or None; today Settings can override settlement exclusion.
    #[pyo3(signature=(settlement=None))]
    fn next_cash_flow_date(&self, settlement: Option<&PyDate>) -> PyResult<Option<PyDate>> {
        Ok(self
            .inspection_payment(settlement, true, false)?
            .0
            .map(PyDate::from_inner))
    }
    /// Aggregate previous actual payment, including principal, or None; not a per-100 quote.
    #[pyo3(signature=(settlement=None))]
    fn previous_cash_flow_amount(&self, settlement: Option<&PyDate>) -> PyResult<Option<f64>> {
        Ok(self.inspection_payment(settlement, false, true)?.1)
    }
    /// Aggregate next actual payment, including principal, or None; not a per-100 quote.
    #[pyo3(signature=(settlement=None))]
    fn next_cash_flow_amount(&self, settlement: Option<&PyDate>) -> PyResult<Option<f64>> {
        Ok(self.inspection_payment(settlement, true, true)?.1)
    }
    fn is_expired(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .instrument()
            .is_expired()
            .map_err(PyQlError::from)?)
    }
    fn is_calculated(&self) -> bool {
        self.inner.instrument().base().is_calculated()
    }
    /// Pin cached results while unexpired; native expiry can still bypass freeze.
    fn freeze(&self) {
        self.frozen.set(true);
        self.inner.with_mut(|bond| bond.base_mut().freeze());
    }
    fn unfreeze(&self) {
        self.frozen.set(false);
        self.inner.with_mut(|bond| bond.base_mut().unfreeze());
    }
    fn recalculate(&mut self) -> PyResult<()> {
        self.prepare(true)?;
        self.inner
            .with_mut(|bond| bond.recalculate())
            .map_err(PyQlError::from)?;
        Ok(())
    }
}

/// Fixed coupon bond, retaining the common Bond valuation interface.
#[gen_stub_pyclass]
#[pyclass(name = "FixedRateBond", extends=PyBond, unsendable, module = "itofin.instruments")]
pub struct PyFixedRateBond {
    frequency: PyFrequency,
    day_counter: PyDayCounter,
    first_period_day_counter: Option<PyDayCounter>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyFixedRateBond {
    #[gen_stub(override_return_type(type_repr = "FixedRateBond"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, face_amount, schedule, coupons, accrual_day_counter, settings, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None, payment_calendar=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false, first_period_day_counter=None))]
    fn new(
        settlement_days: u32,
        face_amount: f64,
        schedule: &PySchedule,
        coupons: Vec<f64>,
        accrual_day_counter: &PyDayCounter,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
        payment_calendar: Option<&PyCalendar>,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_convention: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
        first_period_day_counter: Option<&PyDayCounter>,
    ) -> PyResult<PyClassInitializer<Self>> {
        if settlement_days > (Date::max_date() - Date::min_date()) as u32 {
            return Err(ItofinError::new_err(
                "settlement days exceed supported date range",
            ));
        }
        positive(face_amount)?;
        positive(redemption)?;
        positive(face_amount * redemption / 100.0)?;
        for &coupon in &coupons {
            finite(coupon)?;
        }
        let retained_calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        let calendar = payment_calendar
            .cloned()
            .unwrap_or_else(|| retained_calendar.clone());
        let ex_calendar = ex_coupon_calendar.cloned().unwrap_or_else(|| {
            PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new())
        });
        if issue_date.is_some_and(|date| date.inner() == Date::null()) {
            return Err(ItofinError::new_err("null issue date"));
        }
        for &date in schedule.dates().iter().skip(1) {
            let payment = adjust(&calendar, date, payment_convention.inner())?;
            if let Some(period) = ex_coupon_period {
                let period = period.inner();
                advance(
                    &ex_calendar,
                    payment,
                    -i64::from(period.length()),
                    period.units(),
                    ex_coupon_convention.inner(),
                    ex_coupon_end_of_month,
                )?;
            }
        }
        if schedule.has_tenor() && schedule.has_is_regular() && schedule.has_end_of_month() {
            let tenor = schedule.tenor();
            if !schedule.is_regular_at(1) {
                advance(
                    &retained_calendar,
                    schedule.date(1),
                    -i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
            if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &retained_calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
        }
        let bond = FixedRateBond::new(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            accrual_day_counter.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            Some(calendar.inner()),
            ex_coupon_period.map(PyPeriod::inner),
            ex_calendar.inner(),
            ex_coupon_convention.inner(),
            ex_coupon_end_of_month,
            first_period_day_counter.map(PyDayCounter::inner),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        for flow in bond.bond().cashflows() {
            finite(flow.amount().map_err(PyQlError::from)?)?;
        }
        let typed = Self {
            frequency: PyFrequency::from_inner(bond.frequency())?,
            day_counter: PyDayCounter::from_inner(bond.day_counter().clone()),
            first_period_day_counter: bond
                .first_period_day_counter()
                .map(|dc| PyDayCounter::from_inner(dc.clone())),
        };
        Ok(
            PyClassInitializer::from(PyBond::from_inner(bond.into_bond(), calendar))
                .add_subclass(typed),
        )
    }
    fn frequency(&self) -> PyFrequency {
        self.frequency
    }
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.day_counter.inner())
    }
    fn first_period_day_counter(&self) -> Option<PyDayCounter> {
        self.first_period_day_counter
            .as_ref()
            .map(|dc| PyDayCounter::from_inner(dc.inner()))
    }
}

/// One redemption on an adjusted payment date, with contractual maturity intact.
#[gen_stub_pyclass]
#[pyclass(name="ZeroCouponBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyZeroCouponBond;
#[gen_stub_pymethods]
#[pymethods]
impl PyZeroCouponBond {
    #[gen_stub(override_return_type(type_repr = "ZeroCouponBond"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, calendar, face_amount, maturity_date, settings, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None))]
    fn new(
        settlement_days: u32,
        calendar: &PyCalendar,
        face_amount: f64,
        maturity_date: &PyDate,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        if settlement_days > (Date::max_date() - Date::min_date()) as u32 {
            return Err(ItofinError::new_err(
                "settlement days exceed supported date range",
            ));
        }
        positive(face_amount)?;
        positive(redemption)?;
        positive(face_amount * redemption / 100.0)?;
        if issue_date.is_some_and(|date| date.inner() == Date::null()) {
            return Err(ItofinError::new_err("null issue date"));
        }
        adjust(calendar, maturity_date.inner(), payment_convention.inner())?;
        let bond = ZeroCouponBond::new(
            settlement_days,
            calendar.inner(),
            face_amount,
            maturity_date.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(
            PyClassInitializer::from(PyBond::from_inner(bond.into_bond(), calendar.clone()))
                .add_subclass(Self),
        )
    }
}

/// Observable discounting engine, with live curve and explicit retained settings.
#[gen_stub_pyclass]
#[pyclass(
    name = "DiscountingBondEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyDiscountingBondEngine {
    inner: SharedMut<DiscountingBondEngine>,
    settings: Shared<Settings<Date>>,
    curve: Handle<dyn YieldTermStructure>,
    include: Option<bool>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyDiscountingBondEngine {
    #[new]
    #[pyo3(signature=(discount_curve, settings, include_settlement_date_flows=None))]
    fn new(
        discount_curve: &PyYieldTermStructure,
        settings: &PySettings,
        include_settlement_date_flows: Option<bool>,
    ) -> Self {
        Self {
            inner: shared_mut(DiscountingBondEngine::new(
                discount_curve.handle(),
                include_settlement_date_flows,
                settings.inner(),
            )),
            settings: settings.inner(),
            curve: discount_curve.handle(),
            include: include_settlement_date_flows,
        }
    }
}

#[cfg(test)]
mod analytics_tests {
    use super::*;
    use libitofin::interestrate::Compounding;
    use libitofin::shared::shared;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::time::calendars::NullCalendar;
    use libitofin::time::date::Month;
    use libitofin::time::daycounters::actual360::Actual360;
    use libitofin::time::frequency::Frequency;

    #[test]
    fn pure_analytics_preserve_private_policy_flags_and_freeze_state() {
        let settings = shared(Settings::<Date>::new());
        let native = ZeroCouponBond::new(
            2,
            NullCalendar::new(),
            250.0,
            Date::new(1, Month::January, 2027),
            Bdc::Unadjusted,
            102.0,
            None,
            Shared::clone(&settings),
        )
        .expect("zero bond");
        let bond = PyBond::from_floating(
            native.into_bond(),
            PyCalendar::from_inner(NullCalendar::new()),
            shared(|_| Ok(())),
        );
        let old_flags = bond.flags;
        bond.freeze();
        settings.set_using_at_par_coupons(false);
        settings.set_include_reference_date_events(true);
        let settlement = PyDate::from_inner(Date::new(1, Month::February, 2026));
        let dc = PyDayCounter::from_inner(Actual360::new());
        let curve = shared(FlatForward::with_rate(
            Date::new(1, Month::January, 2026),
            0.025,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let curve =
            PyYieldTermStructure::from_inner(Handle::new(curve as Shared<dyn YieldTermStructure>));
        let mode = PyCompounding::Compounded;
        let freq = PyFrequency::Semiannual;
        let clean = bond
            .clean_price_from_curve(&curve, Some(&settlement))
            .expect("curve quote");
        bond.dirty_price_from_curve(&curve, Some(&settlement))
            .expect("dirty curve quote");
        let yield_price = bond
            .clean_price_from_yield(0.03, &dc, mode, &freq, Some(&settlement))
            .expect("yield quote");
        bond.dirty_price_from_yield(0.03, &dc, mode, &freq, Some(&settlement))
            .expect("dirty yield quote");
        bond.yield_rate(
            yield_price,
            &dc,
            mode,
            &freq,
            PyBondPriceType::Clean,
            Some(&settlement),
            1e-12,
            1000,
            0.05,
        )
        .expect("yield solve");
        let spread_price = bond
            .clean_price_at_z_spread(&curve, 0.007, mode, &freq, Some(&settlement))
            .expect("spread quote");
        bond.dirty_price_at_z_spread(&curve, 0.007, mode, &freq, Some(&settlement))
            .expect("dirty spread quote");
        bond.z_spread(
            spread_price,
            &curve,
            mode,
            &freq,
            PyBondPriceType::Clean,
            Some(&settlement),
            1e-12,
            1000,
            0.0,
        )
        .expect("spread solve");
        assert!(clean > 0.0);
        assert_eq!(bond.flags, old_flags);
        assert!(bond.frozen.get());
        assert!(!bond.inner.borrow().base().is_calculated());
    }
}

#[cfg(test)]
mod callable_tests {
    use super::*;
    use libitofin::errors::QlResult;
    use libitofin::instruments::{
        BondResults, Callability, CallabilityType, CallableBondArguments,
    };
    use libitofin::patterns::observable::{AsObservable, Observable, Observer};
    use libitofin::pricingengine::{Arguments, GenericEngine, Results};
    use libitofin::shared::shared;
    use libitofin::time::calendars::NullCalendar;
    use libitofin::time::date::Month;
    use libitofin::time::daycounters::actual360::Actual360;
    use libitofin::time::frequency::Frequency;
    use libitofin::time::schedule::MakeSchedule;

    struct CountingEngine {
        base: GenericEngine<CallableBondArguments, BondResults>,
        calls: Shared<Cell<usize>>,
    }
    impl AsObservable for CountingEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }
    impl PricingEngine for CountingEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }
        fn results(&self) -> &dyn Results {
            self.base.results()
        }
        fn reset(&mut self) {
            self.base.reset();
        }
        fn calculate(&mut self) -> QlResult<()> {
            let args = self.base.arguments();
            assert_eq!(args.callability_types, vec![CallabilityType::Put]);
            assert_eq!(args.callability_prices, vec![97.0]);
            assert_eq!(args.face_amount, 250.0);
            self.calls.set(self.calls.get() + 1);
            let result = self.base.results_mut();
            result.instrument.value = Some(200.0 + self.calls.get() as f64);
            result.settlement_value = Some(210.0 + self.calls.get() as f64);
            Ok(())
        }
    }
    struct Notifications(Shared<Cell<usize>>);
    impl Observer for Notifications {
        fn update(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    fn retained(zero: bool, settings: Shared<Settings<Date>>) -> PyBond {
        let calendar = PyCalendar::from_inner(NullCalendar::new());
        let maturity = Date::new(1, Month::January, 2028);
        let event = Callability::new(
            BondPrice::Dirty(97.0),
            CallabilityType::Put,
            Date::new(1, Month::January, 2027),
        )
        .unwrap();
        let backing = if zero {
            BondBacking::CallableZero(shared_mut(
                CallableZeroCouponBond::new(
                    0,
                    250.0,
                    calendar.inner(),
                    maturity,
                    Actual360::new(),
                    Bdc::Unadjusted,
                    100.0,
                    None,
                    vec![event],
                    settings,
                )
                .unwrap(),
            ))
        } else {
            let schedule = MakeSchedule::new()
                .from(Date::new(1, Month::January, 2026))
                .to(maturity)
                .with_frequency(Frequency::Annual)
                .with_calendar(calendar.inner())
                .with_convention(Bdc::Unadjusted)
                .build();
            BondBacking::CallableFixed(shared_mut(
                CallableFixedRateBond::new(
                    0,
                    250.0,
                    schedule,
                    vec![0.04],
                    Actual360::new(),
                    Bdc::Unadjusted,
                    100.0,
                    None,
                    vec![event],
                    settings,
                )
                .unwrap(),
            ))
        };
        PyBond::from_backing(backing, calendar)
    }
    #[test]
    fn typed_callable_dispatch_reuses_single_engine_arguments_results_and_cache() {
        for zero in [false, true] {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(Date::new(1, Month::February, 2026));
            let mut bond = retained(zero, Shared::clone(&settings));
            let calls = shared(Cell::new(0));
            let engine = shared_mut(CountingEngine {
                base: GenericEngine::new(CallableBondArguments::default(), BondResults::default()),
                calls: Shared::clone(&calls),
            });
            bond.inner
                .with_mut(|bond| bond.base_mut().set_pricing_engine(engine));
            assert!(!bond.is_calculated());
            assert_eq!(bond.npv().unwrap(), 201.0);
            assert_eq!(bond.settlement_value().unwrap(), 211.0);
            assert_eq!(bond.dirty_price().unwrap(), (211.0 / 250.0) * 100.0);
            assert_eq!(calls.get(), 1);
            assert!(bond.is_calculated());
            let curve: Shared<dyn YieldTermStructure> =
                shared(libitofin::termstructures::yields::FlatForward::with_rate(
                    Date::new(1, Month::February, 2026),
                    0.03,
                    Actual360::new(),
                    libitofin::interestrate::Compounding::Continuous,
                    Frequency::Annual,
                ));
            let curve = Handle::new(curve);
            let wrong = PyDiscountingBondEngine {
                inner: shared_mut(DiscountingBondEngine::new(
                    curve.clone(),
                    None,
                    Shared::clone(&settings),
                )),
                settings: Shared::clone(&settings),
                curve,
                include: None,
            };
            assert!(bond.set_engine(&wrong).is_err());
            assert!(bond.engine_inputs.is_none());
            assert!(bond.is_calculated());
            assert_eq!(bond.npv().unwrap(), 201.0);
            assert_eq!(calls.get(), 1);
            bond.freeze();
            bond.inner
                .instrument()
                .base()
                .observer()
                .borrow_mut()
                .update();
            assert_eq!(bond.npv().unwrap(), 201.0);
            bond.recalculate().unwrap();
            assert_eq!(bond.npv().unwrap(), 202.0);
            assert_eq!(calls.get(), 2);
            assert!(bond.frozen.get());
            let cached = bond.npv().unwrap();
            let flags = bond.flags;
            settings.set_include_reference_date_events(true);
            let dc = PyDayCounter::from_inner(Actual360::new());
            let settlement = PyDate::from_inner(Date::new(1, Month::March, 2026));
            assert!(
                bond.clean_price_from_yield(
                    0.03,
                    &dc,
                    PyCompounding::Continuous,
                    &PyFrequency::Annual,
                    Some(&settlement)
                )
                .unwrap()
                    > 0.0
            );
            assert_eq!(bond.flags, flags);
            assert_eq!(bond.npv().unwrap(), cached);
            assert_eq!(calls.get(), 2);
            bond.unfreeze();
            assert_eq!(bond.npv().unwrap(), 203.0);
            settings.set_evaluation_date(Date::new(2, Month::January, 2028));
            bond.freeze();
            assert_eq!(bond.npv().unwrap(), 0.0);
            assert_eq!(bond.settlement_value().unwrap(), 0.0);
            assert_eq!(bond.dirty_price().unwrap(), 0.0);
            assert_eq!(calls.get(), 3);
        }
    }
    #[test]
    fn callable_zero_typed_base_refreshes_silent_flags_without_wrapper_notification() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(2, Month::January, 2028));
        let mut bond = retained(true, Shared::clone(&settings));
        assert_eq!(bond.npv().unwrap(), 0.0);
        assert!(bond.is_calculated());
        let notifications = shared(Cell::new(0));
        let observer =
            shared_mut(Notifications(Shared::clone(&notifications))) as SharedMut<dyn Observer>;
        bond.inner.instrument().base().register_observer(&observer);
        bond.freeze();
        settings.set_include_reference_date_events(true);
        assert!(!bond.is_calculated());
        assert!(bond.frozen.get());
        assert_eq!(bond.npv().unwrap(), 0.0);
        assert!(bond.is_calculated());
        assert_eq!(notifications.get(), 0);
        assert!(
            Callability::new(BondPrice::Clean(100.0), CallabilityType::Call, Date::null()).is_err()
        );
    }
}

#[cfg(test)]
mod callable_black_iv_tests {
    use super::*;
    use libitofin::instruments::{Callability, CallabilityType};
    use libitofin::patterns::observable::Observer;
    use libitofin::pricingengines::bond::{
        BlackCallableBondArguments, BlackCallableFixedRateBondEngine,
    };
    use libitofin::quotes::{Quote, SimpleQuote};
    use libitofin::shared::shared;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::time::calendars::NullCalendar;
    use libitofin::time::date::Month;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;
    use libitofin::time::schedule::MakeSchedule;
    use std::any::Any;

    struct Notifications(Shared<Cell<usize>>);
    impl Observer for Notifications {
        fn update(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    fn day(year: i32) -> Date {
        Date::new(15, Month::January, year)
    }
    fn curve() -> PyYieldTermStructure {
        PyYieldTermStructure::from_inner(Handle::new(shared(FlatForward::with_rate(
            day(2026),
            0.04,
            Actual365Fixed::new(),
            libitofin::interestrate::Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>))
    }
    fn product(zero: bool, settings: Shared<Settings<Date>>) -> PyBond {
        let calendar = PyCalendar::from_inner(NullCalendar::new());
        let rights = vec![
            Callability::new(BondPrice::Clean(100.0), CallabilityType::Call, day(2028)).unwrap(),
        ];
        let backing = if zero {
            BondBacking::CallableZero(shared_mut(
                CallableZeroCouponBond::new(
                    3,
                    1000.0,
                    calendar.inner(),
                    day(2031),
                    Actual365Fixed::new(),
                    Bdc::Unadjusted,
                    100.0,
                    Some(day(2026)),
                    rights,
                    settings,
                )
                .unwrap(),
            ))
        } else {
            let schedule = MakeSchedule::new()
                .from(day(2026))
                .to(day(2031))
                .with_frequency(Frequency::Semiannual)
                .with_calendar(calendar.inner())
                .with_convention(Bdc::Unadjusted)
                .build();
            BondBacking::CallableFixed(shared_mut(
                CallableFixedRateBond::new(
                    3,
                    1000.0,
                    schedule,
                    vec![0.055],
                    Actual365Fixed::new(),
                    Bdc::Unadjusted,
                    100.0,
                    Some(day(2026)),
                    rights,
                    settings,
                )
                .unwrap(),
            ))
        };
        PyBond::from_backing(backing, calendar)
    }
    #[derive(Debug, PartialEq)]
    struct ArgumentSnapshot {
        settlement: Option<Date>,
        coupon_dates: Vec<Date>,
        coupon_amounts: Vec<f64>,
        exercise_dates: Vec<Date>,
        exercise_prices: Vec<f64>,
        original_count: usize,
        cashflows: Vec<(Date, f64)>,
        frequency: Frequency,
    }
    fn arguments(engine: &mut BlackCallableFixedRateBondEngine) -> ArgumentSnapshot {
        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_ref::<BlackCallableBondArguments>()
            .unwrap();
        ArgumentSnapshot {
            settlement: args.bond.settlement_date,
            coupon_dates: args.bond.coupon_dates.clone(),
            coupon_amounts: args.bond.coupon_amounts.clone(),
            exercise_dates: args.bond.callability_dates.clone(),
            exercise_prices: args.bond.callability_prices.clone(),
            original_count: args.callability_count,
            cashflows: args
                .cashflows
                .iter()
                .map(|flow| (flow.date(), flow.amount().unwrap()))
                .collect(),
            frequency: args.frequency,
        }
    }
    #[test]
    fn pure_iv_preserves_actual_engine_arguments_results_flags_freeze_and_notifications() {
        for zero in [false, true] {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(day(2026));
            let term = curve();
            let quote = shared(SimpleQuote::new(0.35));
            let engine = shared_mut(BlackCallableFixedRateBondEngine::new(
                Handle::new(quote.clone() as Shared<dyn Quote>),
                term.handle(),
                settings.clone(),
            ));
            let mut bond = product(zero, settings.clone());
            bond.inner
                .with_mut(|bond| bond.base_mut().set_pricing_engine(engine.clone()));
            let cached = bond.npv().unwrap();
            let target = bond.clean_price().unwrap();
            let stored = bond.inner.borrow().base().results().value;
            let saved_arguments = arguments(&mut engine.borrow_mut());
            let saved_results = engine
                .borrow()
                .results()
                .as_instrument_results()
                .unwrap()
                .value;
            let installed = bond.inner.borrow().base().pricing_engine().unwrap().clone();
            let notifications = shared(Cell::new(0));
            let observer =
                shared_mut(Notifications(notifications.clone())) as SharedMut<dyn Observer>;
            bond.inner.borrow().base().register_observer(&observer);
            bond.freeze();
            let flags = bond.flags;
            settings.set_include_reference_date_events(true);
            settings.set_include_todays_cash_flows(Some(true));
            assert!(bond.inner.borrow().base().is_calculated());
            assert!(
                (bond
                    .callable_implied_volatility(
                        target,
                        &term,
                        PyBondPriceType::Clean,
                        1e-10,
                        100,
                        0.0,
                        1.0
                    )
                    .unwrap()
                    - 0.35)
                    .abs()
                    < 2e-8
            );
            for (target, budget) in [(f64::NAN, 100), (-1e10, 100), (1e10, 100), (target, 1)] {
                assert!(
                    bond.callable_implied_volatility(
                        target,
                        &term,
                        PyBondPriceType::Clean,
                        1e-10,
                        budget,
                        0.0,
                        1.0
                    )
                    .is_err()
                );
            }
            assert_eq!(bond.flags, flags);
            assert!(bond.frozen.get());
            assert!(bond.engine_inputs.is_none());
            assert!(bond.inner.borrow().base().is_calculated());
            assert_eq!(bond.inner.borrow().base().results().value, stored);
            assert!(SharedMut::ptr_eq(
                &installed,
                bond.inner.borrow().base().pricing_engine().unwrap()
            ));
            assert_eq!(arguments(&mut engine.borrow_mut()), saved_arguments);
            assert_eq!(
                engine
                    .borrow()
                    .results()
                    .as_instrument_results()
                    .unwrap()
                    .value,
                saved_results
            );
            assert_eq!(quote.value().unwrap(), 0.35);
            assert_eq!(notifications.get(), 0);
            quote.set_value(0.7);
            assert_eq!(notifications.get(), 0);
            assert_eq!(bond.npv().unwrap(), cached);
            bond.unfreeze();
            assert_ne!(bond.npv().unwrap(), cached);
        }
    }
    #[test]
    fn pure_iv_without_engine_preserves_pending_policy_and_uncalculated_state() {
        for zero in [false, true] {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(day(2026));
            let bond = product(zero, settings.clone());
            let flags = bond.flags;
            settings.set_include_reference_date_events(true);
            let target = if zero {
                81.85454709204
            } else {
                102.10242051757368
            };
            let root = bond
                .callable_implied_volatility(
                    target,
                    &curve(),
                    PyBondPriceType::Dirty,
                    1e-10,
                    100,
                    0.0,
                    1.0,
                )
                .unwrap();
            assert!((root - 0.35).abs() < 2e-8);
            assert_eq!(bond.flags, flags);
            assert!(!bond.frozen.get());
            assert!(!bond.inner.borrow().base().is_calculated());
            assert!(bond.inner.borrow().base().pricing_engine().is_none());
            assert!(bond.inner.borrow().base().results().value.is_none());
        }
    }
}
