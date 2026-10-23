//! Plain Ibor cross-currency swaps with live discounting and snapshot spot.
use crate::bonds::adjust;
use crate::currency::PyCurrency;
use crate::curve::PyYieldTermStructure;
use crate::float_float_swap::LegGuard;
use crate::hullwhite::PyIborIndex;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PySchedule};
use crate::{ItofinError, PyQlError};
use libitofin::exchangerate::ExchangeRate;
use libitofin::instruments::XccyBasisSwap;
use libitofin::settings::Settings;
use libitofin::shared::Shared;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, date::Date, schedule::Schedule,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Constant-notional Ibor swap paying base and receiving quote, including principal exchanges.
/// Spot is an immutable quote-per-base snapshot. Every query rereads live curves/fixings/Settings.
/// This is not a resetting, overnight, live-spot or cached pricing-engine facade.
#[gen_stub_pyclass]
#[pyclass(name = "XccyBasisSwap", unsendable, module = "itofin.instruments")]
pub struct PyXccyBasisSwap {
    inner: XccyBasisSwap,
    guards: [LegGuard; 2],
    settings: Shared<Settings<Date>>,
    valuation: Option<(Date, bool)>,
    _schedules: [Schedule; 2],
    _calendars: [PyCalendar; 2],
}
impl PyXccyBasisSwap {
    fn value(&self, query: i32) -> PyResult<f64> {
        for (side, (leg, guard)) in [self.inner.base_leg(), self.inner.quote_leg()]
            .into_iter()
            .zip(&self.guards)
            .enumerate()
        {
            if (query == 0 && side != 0) || ((query == 1 || query == 2) && side != 1) {
                continue;
            }
            let (settlement, include) = match self.valuation {
                Some((date, include)) => (Some(date), Some(include)),
                None => (self.settings.evaluation_date(), None),
            };
            for flow in leg {
                if !flow
                    .has_occurred(&self.settings, settlement, include)
                    .map_err(PyQlError::from)?
                    && let Some(coupon) = flow.as_coupon()
                {
                    guard.check(
                        coupon.accrual_start_date(),
                        coupon.accrual_end_date(),
                        &self.settings,
                    )?;
                }
            }
        }
        let value = match query {
            0 => self.inner.base_leg_pv(),
            1 => self.inner.quote_leg_pv(),
            2 => self.inner.quote_leg_bps(),
            3 => self.inner.npv().map(|money| money.value()),
            4 => self.inner.npv_in_quote().map(|money| money.value()),
            _ => self.inner.fair_quote_basis_spread(),
        };
        Ok(value.map_err(PyQlError::from)?)
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyXccyBasisSwap {
    /// Copy currencies and spot, retaining the exact live curves, indexes, schedules and Settings.
    /// Omitted payment convention means Following. Both valuation dates must be supplied together.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(base_schedule,base_index,base_curve,base_notional,base_day_count,quote_schedule,quote_index,quote_curve,quote_notional,quote_day_count,quote_spread,base_currency,quote_currency,spot,settings,*,payment_convention=None,settlement_date=None,npv_date=None,include_settlement_flows=false,spot_settlement_date=None))]
    fn new(
        base_schedule: &PySchedule,
        base_index: &PyIborIndex,
        base_curve: &PyYieldTermStructure,
        base_notional: f64,
        base_day_count: &PyDayCounter,
        quote_schedule: &PySchedule,
        quote_index: &PyIborIndex,
        quote_curve: &PyYieldTermStructure,
        quote_notional: f64,
        quote_day_count: &PyDayCounter,
        quote_spread: f64,
        base_currency: &PyCurrency,
        quote_currency: &PyCurrency,
        spot: f64,
        settings: &PySettings,
        payment_convention: Option<&PyBusinessDayConvention>,
        settlement_date: Option<&PyDate>,
        npv_date: Option<&PyDate>,
        include_settlement_flows: bool,
        spot_settlement_date: Option<&PyDate>,
    ) -> PyResult<Self> {
        let valuation = match (settlement_date, npv_date) {
            (None, None) => None,
            (Some(settlement), Some(npv)) => {
                Some((settlement.inner(), npv.inner(), include_settlement_flows))
            }
            _ => {
                return Err(ItofinError::new_err(
                    "settlement_date and npv_date must be supplied together",
                ));
            }
        };
        if valuation.is_none() && include_settlement_flows {
            return Err(ItofinError::new_err(
                "include_settlement_flows requires fixed valuation dates",
            ));
        }
        let guards = [
            LegGuard {
                index: base_index.inner(),
                calendars: base_index.retained_calendars(),
            },
            LegGuard {
                index: quote_index.inner(),
                calendars: quote_index.retained_calendars(),
            },
        ];
        let schedules = [base_schedule.inner(), quote_schedule.inner()];
        let calendars = [
            base_schedule.retained_calendar(),
            quote_schedule.retained_calendar(),
        ];
        let payment = payment_convention.map(PyBusinessDayConvention::inner);
        for ((guard, schedule), calendar) in guards.iter().zip(&schedules).zip(&calendars) {
            guard.construction(
                schedule,
                calendar,
                Some(payment.unwrap_or(BusinessDayConvention::Following)),
            )?;
        }
        if schedules.iter().all(|schedule| schedule.len() >= 2) {
            let start = schedules[0].start_date().min(schedules[1].start_date());
            let end = schedules[0].end_date().max(schedules[1].end_date());
            for (schedule, calendar) in schedules.iter().zip(&calendars) {
                adjust(calendar, start, schedule.business_day_convention())?;
                adjust(calendar, end, schedule.business_day_convention())?;
            }
        }
        let settings = settings.inner();
        let spot = ExchangeRate::checked_new(base_currency.inner(), quote_currency.inner(), spot)
            .map_err(PyQlError::from)?;
        let mut inner = XccyBasisSwap::new(
            schedules[0].clone(),
            guards[0].index.clone(),
            base_curve.handle(),
            base_notional,
            base_day_count.inner(),
            schedules[1].clone(),
            guards[1].index.clone(),
            quote_curve.handle(),
            quote_notional,
            quote_day_count.inner(),
            quote_spread,
            spot,
            payment,
            settings.clone(),
        )
        .map_err(PyQlError::from)?;
        if let Some((settlement, npv, include)) = valuation {
            inner = inner
                .with_valuation_dates(settlement, npv, include)
                .map_err(PyQlError::from)?;
        }
        if let Some(date) = spot_settlement_date {
            inner = inner
                .with_spot_settlement_date(date.inner())
                .map_err(PyQlError::from)?;
        }
        Ok(Self {
            inner,
            guards,
            settings,
            valuation: valuation.map(|(date, _, include)| (date, include)),
            _schedules: schedules,
            _calendars: calendars,
        })
    }
    /// Contractual base-currency leg PV before its paid-leg sign.
    fn base_leg_pv(&self) -> PyResult<f64> {
        self.value(0)
    }
    /// Contractual quote-currency leg PV before its received-leg sign.
    fn quote_leg_pv(&self) -> PyResult<f64> {
        self.value(1)
    }
    /// Quote-currency sensitivity per one basis point of spread, excluding principal.
    fn quote_leg_bps(&self) -> PyResult<f64> {
        self.value(2)
    }
    /// Signed base-currency NPV at the selected NPV date.
    fn npv(&self) -> PyResult<f64> {
        self.value(3)
    }
    /// Signed quote-currency NPV at the selected NPV date.
    fn npv_in_quote(&self) -> PyResult<f64> {
        self.value(4)
    }
    /// Absolute quote spread repricing the swap to zero, not a spread adjustment.
    fn fair_quote_basis_spread(&self) -> PyResult<f64> {
        self.value(5)
    }
    /// Immutable quote-currency units per one base unit.
    fn spot(&self) -> f64 {
        self.inner.spot().rate()
    }
    /// Independent copy of the base currency.
    fn base_currency(&self) -> PyCurrency {
        PyCurrency::from_inner(self.inner.spot().source().clone())
    }
    /// Independent copy of the quote currency.
    fn quote_currency(&self) -> PyCurrency {
        PyCurrency::from_inner(self.inner.spot().target().clone())
    }
}
