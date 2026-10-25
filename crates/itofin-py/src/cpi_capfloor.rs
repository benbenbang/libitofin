//! Cumulative CPI options on immutable, bilinear quoted-price snapshots.
use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::inflation::{PyCpiInterpolationType, PyZeroInflationIndex};
use crate::option::PyOptionType;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::instrument::Instrument;
use libitofin::instruments::{CpiCapFloor, CpiCapFloorConfig};
use libitofin::pricingengines::InterpolatingCpiCapFloorEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::cpicapfloortermpricesurface::{
    CpiCapFloorSurfaceConfig, CpiCapFloorTermPriceSurface,
};
use libitofin::time::{period::Period, timeunit::TimeUnit};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Frozen bilinear CPI quoted-price grid, with extrapolation enabled.
///
/// Rebuild after curve, fixing or evaluation-date changes. Nominal is metadata:
/// parity and the instrument engine return quote units without rescaling.
#[gen_stub_pyclass]
#[pyclass(
    name = "CpiCapFloorTermPriceSurface",
    unsendable,
    module = "itofin.termstructures"
)]
pub struct PyCpiCapFloorTermPriceSurface {
    inner: Shared<CpiCapFloorTermPriceSurface>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiCapFloorTermPriceSurface {
    /// Build an immutable quote snapshot. Maturities are positive whole years;
    /// price matrices have strike rows and maturity columns. Both axes must
    /// increase. Flat/Linear are supported; deprecated AsIndex is not.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        nominal: f64,
        base_rate: f64,
        observation_lag: &PyPeriod,
        calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        index: &PyZeroInflationIndex,
        interpolation: &PyCpiInterpolationType,
        discount: &PyYieldTermStructure,
        cap_strikes: Vec<f64>,
        floor_strikes: Vec<f64>,
        maturity_years: Vec<i32>,
        cap_prices: Vec<Vec<f64>>,
        floor_prices: Vec<Vec<f64>>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        let today = settings
            .inner()
            .evaluation_date()
            .ok_or_else(|| crate::ItofinError::new_err("evaluation date not set"))?;
        let reference = crate::bonds::adjust(
            calendar,
            today,
            libitofin::time::businessdayconvention::BusinessDayConvention::Following,
        )?;
        for y in &maturity_years {
            crate::bonds::advance(
                calendar,
                reference,
                i64::from(*y),
                TimeUnit::Years,
                convention.inner(),
                false,
            )?;
        }
        Ok(Self {
            inner: shared(
                CpiCapFloorTermPriceSurface::new(CpiCapFloorSurfaceConfig {
                    nominal,
                    base_rate,
                    observation_lag: observation_lag.inner(),
                    calendar: calendar.inner(),
                    business_day_convention: convention.inner(),
                    day_counter: day_counter.inner(),
                    index: index.shared(),
                    interpolation: interpolation.inner(),
                    discount: discount.handle(),
                    cap_strikes,
                    floor_strikes,
                    maturities: maturity_years
                        .into_iter()
                        .map(|y| Period::new(y, TimeUnit::Years))
                        .collect(),
                    cap_prices: crate::capfloortermvol::matrix_from_rows(&cap_prices)?,
                    floor_prices: crate::capfloortermvol::matrix_from_rows(&floor_prices)?,
                    settings: settings.inner(),
                })
                .map_err(PyQlError::from)?,
            ),
        })
    }
    /// Return the quoted cap price, with unrestricted bilinear extrapolation.
    fn cap_price(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        self.inner
            .cap_price(date.inner(), strike)
            .map_err(|e| PyQlError::from(e).into())
    }
    /// Return the quoted floor price; extrapolated results may be negative.
    fn floor_price(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        self.inner
            .floor_price(date.inner(), strike)
            .map_err(|e| PyQlError::from(e).into())
    }
    /// The captured reference date, which never rolls with settings.
    fn reference_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.reference_date())
    }
}

/// Price-surface interpolation engine, without instrument notional scaling.
#[gen_stub_pyclass]
#[pyclass(
    name = "InterpolatingCpiCapFloorEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyInterpolatingCpiCapFloorEngine {
    inner: SharedMut<InterpolatingCpiCapFloorEngine>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyInterpolatingCpiCapFloorEngine {
    /// Retain an immutable CPI price surface.
    #[new]
    fn new(surface: &PyCpiCapFloorTermPriceSurface) -> Self {
        Self {
            inner: shared_mut(InterpolatingCpiCapFloorEngine::new(surface.inner.clone())),
        }
    }
}

/// A single cumulative CPI cap/floor, not an option on each coupon.
///
/// NPV is the surface quote, independent of nominal, base_cpi and start_date.
/// Expiry is after raw maturity, not the adjusted payment date.
#[gen_stub_pyclass]
#[pyclass(name = "CpiCapFloor", unsendable, module = "itofin.instruments")]
pub struct PyCpiCapFloor {
    inner: SharedMut<CpiCapFloor>,
    fixing_calendar: PyCalendar,
    payment_calendar: PyCalendar,
}
impl PyCpiCapFloor {
    fn guard(&self) -> PyResult<()> {
        let inner = self.inner.borrow();
        let a = inner.config();
        crate::bonds::advance(
            &self.fixing_calendar,
            a.maturity,
            -i64::from(a.observation_lag.length()),
            a.observation_lag.units(),
            a.fixing_convention,
            false,
        )?;
        crate::bonds::adjust(&self.payment_calendar, a.maturity, a.payment_convention)?;
        Ok(())
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiCapFloor {
    /// Build a cumulative CPI option with explicit calendars and settings.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        option_type: &PyOptionType,
        nominal: f64,
        start_date: &PyDate,
        base_cpi: f64,
        maturity: &PyDate,
        fixing_calendar: &PyCalendar,
        fixing_convention: &PyBusinessDayConvention,
        payment_calendar: &PyCalendar,
        payment_convention: &PyBusinessDayConvention,
        strike: f64,
        index: &PyZeroInflationIndex,
        observation_lag: &PyPeriod,
        interpolation: &PyCpiInterpolationType,
        settings: &PySettings,
        engine: &PyInterpolatingCpiCapFloorEngine,
    ) -> PyResult<Self> {
        crate::bonds::advance(
            fixing_calendar,
            maturity.inner(),
            -i64::from(observation_lag.inner().length()),
            observation_lag.inner().units(),
            fixing_convention.inner(),
            false,
        )?;
        crate::bonds::adjust(
            payment_calendar,
            maturity.inner(),
            payment_convention.inner(),
        )?;
        let mut inner = CpiCapFloor::new(CpiCapFloorConfig {
            option_type: option_type.inner(),
            nominal,
            start_date: start_date.inner(),
            base_cpi,
            maturity: maturity.inner(),
            fixing_calendar: fixing_calendar.inner(),
            fixing_convention: fixing_convention.inner(),
            payment_calendar: payment_calendar.inner(),
            payment_convention: payment_convention.inner(),
            strike,
            index: index.shared(),
            observation_lag: observation_lag.inner(),
            interpolation: interpolation.inner(),
            settings: settings.inner(),
        })
        .map_err(PyQlError::from)?;
        inner.set_pricing_engine(engine.inner.clone());
        Ok(Self {
            inner: shared_mut(inner),
            fixing_calendar: fixing_calendar.clone(),
            payment_calendar: payment_calendar.clone(),
        })
    }
    /// Surface-quoted NPV, zero after raw maturity.
    fn npv(&self) -> PyResult<f64> {
        self.guard()?;
        self.inner
            .borrow_mut()
            .npv()
            .map_err(|e| PyQlError::from(e).into())
    }
    /// Observation-lagged, adjusted fixing date.
    fn fixing_date(&self) -> PyResult<PyDate> {
        self.guard()?;
        self.inner
            .borrow()
            .fixing_date()
            .map(PyDate::from_inner)
            .map_err(|e| PyQlError::from(e).into())
    }
    /// Adjusted payment date, distinct from the expiry date.
    fn pay_date(&self) -> PyResult<PyDate> {
        self.guard()?;
        self.inner
            .borrow()
            .pay_date()
            .map(PyDate::from_inner)
            .map_err(|e| PyQlError::from(e).into())
    }
    /// Whether the live evaluation date is after raw maturity.
    fn is_expired(&self) -> PyResult<bool> {
        self.inner
            .borrow()
            .is_expired()
            .map_err(|e| PyQlError::from(e).into())
    }
}
