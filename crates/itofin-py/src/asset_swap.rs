//! Par full-maturity plain-bond asset swaps with retained live dependencies.
use crate::bonds::{PyBond, adjust, advance, finite};
use crate::curve::PyYieldTermStructure;
use crate::float_float_swap::LegGuard;
use crate::hullwhite::PyIborIndex;
use crate::settings::PySettings;
use crate::time::{PyDate, PyDayCounter, PySchedule};
use crate::{ItofinError, PyQlError};
use libitofin::handle::Handle;
use libitofin::indexes::InterestRateIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::AssetSwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, date::Date, schedule::MakeSchedule,
    timeunit::TimeUnit,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

type Flags = (bool, Option<bool>, bool, bool);
fn flags(s: &Settings<Date>) -> Flags {
    (
        s.include_reference_date_events(),
        s.include_todays_cash_flows(),
        s.enforces_todays_historic_fixings(),
        s.using_at_par_coupons(),
    )
}
/// Full-maturity par asset swap over a plain fixed/zero bullet bond and Ibor index.
/// An explicit floating schedule must end at adjusted full bond maturity.
/// Without one, generate backwards from settlement using the index tenor.
#[gen_stub_pyclass]
#[pyclass(name = "AssetSwap", unsendable, module = "itofin.instruments")]
pub struct PyAssetSwap {
    inner: AssetSwap,
    _bond: Py<PyBond>,
    settings: Shared<Settings<Date>>,
    guard: LegGuard,
    discount: Option<Handle<dyn YieldTermStructure>>,
    flags: Flags,
    frozen: bool,
}
impl PyAssetSwap {
    fn prepare(&self, force: bool) -> PyResult<()> {
        if !self.frozen && self.flags != flags(&self.settings) {
            self.inner.base().observer().borrow_mut().update();
        }
        if !force && (self.frozen || self.inner.base().is_calculated()) {
            return Ok(());
        }
        if self.inner.is_expired().map_err(PyQlError::from)? {
            return Ok(());
        }
        let Some(discount) = &self.discount else {
            return Ok(());
        };
        let settlement = discount
            .current_link()
            .map_err(PyQlError::from)?
            .reference_date()
            .map_err(PyQlError::from)?;
        for flow in self.inner.floating_leg() {
            if flow
                .has_occurred(
                    &self.settings,
                    Some(settlement),
                    Some(self.settings.include_reference_date_events()),
                )
                .map_err(PyQlError::from)?
            {
                continue;
            }
            if let Some(coupon) = flow.as_coupon() {
                self.guard.check(
                    coupon.accrual_start_date(),
                    coupon.accrual_end_date(),
                    &self.settings,
                )?;
            }
        }
        Ok(())
    }
    fn value(&mut self, query: u8) -> PyResult<f64> {
        self.prepare(false)?;
        let value = match query {
            0 => self.inner.npv(),
            1 => self.inner.bond_leg_npv(),
            2 => self.inner.floating_leg_npv(),
            3 => self.inner.floating_leg_bps(),
            4 => self.inner.fair_spread(),
            _ => self.inner.fair_clean_price(),
        }
        .map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        finite(value)
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyAssetSwap {
    /// Build a par swap with an optional floating schedule and index day-counter default.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(pay_bond_coupon, bond, clean_price, index, spread, floating_schedule=None, floating_day_counter=None))]
    fn new(
        py: Python<'_>,
        pay_bond_coupon: bool,
        bond: Py<PyBond>,
        clean_price: f64,
        index: &PyIborIndex,
        spread: f64,
        floating_schedule: Option<&PySchedule>,
        floating_day_counter: Option<&PyDayCounter>,
    ) -> PyResult<Self> {
        let plain = bond.borrow(py).forward_plain()?;
        let guard = LegGuard {
            index: index.inner(),
            calendars: index.retained_calendars(),
        };
        let (schedule, calendar) = if let Some(schedule) = floating_schedule {
            (schedule.inner(), schedule.retained_calendar())
        } else {
            let settlement = bond.borrow(py).settlement_date(None)?.inner();
            let maturity = plain.borrow().maturity_date().map_err(PyQlError::from)?;
            let tenor = guard.index.tenor();
            if settlement >= maturity || tenor.length() <= 0 || tenor.units() == TimeUnit::Days {
                return Err(ItofinError::new_err(
                    "unmatured bond and positive non-day tenor required",
                ));
            }
            let calendar = guard.calendars[0].clone();
            adjust(&calendar, maturity, guard.index.business_day_convention())?;
            let mut count = 1i32;
            loop {
                let amount = count
                    .checked_mul(tenor.length())
                    .and_then(i32::checked_neg)
                    .ok_or_else(|| ItofinError::new_err("schedule tenor overflow"))?;
                let previous = advance(
                    &calendar,
                    maturity,
                    i64::from(amount),
                    tenor.units(),
                    BusinessDayConvention::Unadjusted,
                    false,
                )?;
                adjust(&calendar, previous, guard.index.business_day_convention())?;
                if previous < settlement {
                    break;
                }
                count += 1;
            }
            let schedule = MakeSchedule::new()
                .from(settlement)
                .to(maturity)
                .with_tenor(tenor)
                .with_calendar(calendar.inner())
                .with_convention(guard.index.business_day_convention())
                .with_termination_date_convention(guard.index.business_day_convention())
                .backwards()
                .end_of_month(false)
                .build();
            (schedule, calendar)
        };
        guard.construction(&schedule, &calendar, Some(BusinessDayConvention::Following))?;
        let snapshot = shared(plain.borrow().unpriced_snapshot());
        let settings = snapshot.settings_handle();
        let inner = AssetSwap::new(
            pay_bond_coupon,
            snapshot,
            finite(clean_price)?,
            guard.index.clone(),
            finite(spread)?,
            Some(schedule),
            floating_day_counter.map(PyDayCounter::inner),
        )
        .map_err(PyQlError::from)?;
        Ok(Self {
            inner,
            _bond: bond,
            flags: flags(&settings),
            settings,
            guard,
            discount: None,
            frozen: false,
        })
    }
    /// Attach a live discount curve; Settings must be the bond's identical object.
    fn set_engine(&mut self, curve: &PyYieldTermStructure, settings: &PySettings) -> PyResult<()> {
        let settings = settings.inner();
        if !Shared::ptr_eq(&self.settings, &settings) {
            return Err(ItofinError::new_err("engine must share bond settings"));
        }
        let discount = curve.handle();
        self.inner
            .base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                discount.clone(),
                None,
                None,
                None,
                settings,
            )));
        self.discount = Some(discount);
        Ok(())
    }
    /// Signed present value of both legs, including eligible upfront exchange.
    fn npv(&mut self) -> PyResult<f64> {
        self.value(0)
    }
    /// Signed bond-leg present value.
    fn bond_leg_npv(&mut self) -> PyResult<f64> {
        self.value(1)
    }
    /// Signed floating-leg present value, including principal exchanges.
    fn floating_leg_npv(&mut self) -> PyResult<f64> {
        self.value(2)
    }
    /// Signed floating-leg value for a one-basis-point spread change.
    fn floating_leg_bps(&mut self) -> PyResult<f64> {
        self.value(3)
    }
    /// Absolute floating spread that zeroes the current NPV.
    fn fair_spread(&mut self) -> PyResult<f64> {
        self.value(4)
    }
    /// Native fair clean quote per 100; zeroes NPV only if upfront is included.
    fn fair_clean_price(&mut self) -> PyResult<f64> {
        self.value(5)
    }
    /// Original quoted clean price, per 100.
    fn clean_price(&self) -> f64 {
        self.inner.clean_price()
    }
    /// Original floating spread in decimal rate units.
    fn spread(&self) -> f64 {
        self.inner.spread()
    }
    /// Whether the bond coupons are paid rather than received.
    fn pay_bond_coupon(&self) -> bool {
        self.inner.pay_bond_coupon()
    }
    /// This bounded API always constructs a par swap.
    fn par_swap(&self) -> bool {
        self.inner.par_swap()
    }
    /// Date of the dirty-price upfront payment.
    fn upfront_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.upfront_date())
    }
    /// Populate native cached results if needed.
    fn calculate(&mut self) -> PyResult<()> {
        self.prepare(false)?;
        self.inner.calculate().map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        Ok(())
    }
    /// Force repricing, including while frozen.
    fn recalculate(&mut self) -> PyResult<()> {
        self.prepare(true)?;
        self.inner.recalculate().map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        Ok(())
    }
    /// Pin native cached results until unfreeze or explicit recalculation.
    fn freeze(&mut self) {
        self.inner.base_mut().freeze();
        self.frozen = true;
    }
    /// Resume native observer-driven repricing.
    fn unfreeze(&mut self) {
        self.inner.base_mut().unfreeze();
        self.frozen = false;
    }
    /// Read native cache state without pricing.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }
    /// Test native expiry without pricing.
    fn is_expired(&self) -> PyResult<bool> {
        self.inner
            .is_expired()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
}
