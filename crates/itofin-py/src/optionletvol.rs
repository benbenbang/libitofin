//! Facades for the optionlet (caplet/floorlet) volatility stack: the
//! [`PyOptionletVolatilityStructure`] base, the constant surface
//! [`PyConstantOptionletVolatility`], and the stripping pair
//! [`PyOptionletStripper1`] / [`PyStrippedOptionletAdapter`] that turns market
//! cap term volatilities into a caplet surface.
//!
//! The base holds the erased `Handle<dyn OptionletVolatilityStructure>` and
//! exposes the queries every concrete surface inherits; concrete surfaces
//! subclass it and supply only their constructor. They build the base through
//! [`from_handle`](PyOptionletVolatilityStructure::from_handle) rather than a
//! struct literal, so the surfaces stacking on it in later tickets never need
//! access to the private field.
//!
//! Unlike the swaption surfaces, an optionlet surface has a single option axis:
//! a query takes one option tenor (or date) and a strike, not an option/swap
//! tenor pair.
//!
//! The constant surface exposes both reference-date families (#627): `new` and
//! `with_quote` pin the reference date, while `moving` and `moving_with_quote`
//! float it off the `Settings` evaluation date by a settlement-day count on a
//! calendar, as for the constant swaption surface.

use crate::PyQlError;
use crate::capfloortermvol::PyCapFloorTermVolSurface;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::swaptionvol::PyVolatilityType;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    ConstantOptionletVolatility, OptionletStripper1, OptionletVolatilityStructure,
    StrippedOptionletAdapter, StrippedOptionletBase,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use pyo3::prelude::*;

/// Python `OptionletVolatilityStructure`: the shared base for every caplet
/// volatility surface
/// (`termstructures::volatility::OptionletVolatilityStructure`).
///
/// The option axis is addressed by tenor, the form the surfaces are quoted in;
/// the core resolves the tenor against the surface's reference date and calendar
/// before reading the volatility. The date form is exposed too, since the
/// optionlet stripper and the cap/floor engine both address the surface by a
/// coupon's fixing date.
#[pyclass(name = "OptionletVolatilityStructure", subclass, unsendable)]
pub struct PyOptionletVolatilityStructure {
    inner: Handle<dyn OptionletVolatilityStructure>,
}

#[pymethods]
impl PyOptionletVolatilityStructure {
    /// The caplet volatility for an option tenor and strike.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn volatility(&self, option_tenor: &PyPeriod, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The caplet volatility for an option date and strike.
    #[pyo3(signature = (option_date, strike, extrapolate = false))]
    fn volatility_date(
        &self,
        option_date: &PyDate,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_date(option_date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The Black variance (`vol^2 * option_time`) for an option tenor and strike.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn black_variance(
        &self,
        option_tenor: &PyPeriod,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Whether the surface answers dates/times beyond its maximum.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allows extrapolation past the maximum date/time.
    ///
    /// A stripped surface ends at its last optionlet fixing, so a cap whose own
    /// last caplet fixes there queries the boundary; the core's round-trip
    /// fixture enables extrapolation before repricing
    /// (`strippedoptionletadapter.rs:393`).
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbids extrapolation past the maximum date/time.
    fn disable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .disable_extrapolation();
        Ok(())
    }

    /// The lognormal shift applied to forwards and strikes; `0.0` for the
    /// unshifted lognormal and the normal model.
    ///
    /// This is what `BlackCapFloorEngine` checks a caller-supplied displacement
    /// against, so it is the number to read before pinning one on the engine.
    fn displacement(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .displacement())
    }

    /// The date every option time is measured from.
    ///
    /// Pinned at construction on the fixed-reference surfaces; derived from the
    /// `Settings` evaluation date (settlement days on the calendar) on the
    /// moving ones, so it follows a later `set_evaluation_date`.
    fn reference_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .reference_date()
                .map_err(PyQlError::from)?,
        ))
    }
}

impl PyOptionletVolatilityStructure {
    /// A clone of the inner surface handle for the engine facades.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn OptionletVolatilityStructure> {
        self.inner.clone()
    }

    /// The base a concrete surface's `#[new]` extends, built from its erased
    /// handle. The named constructor keeps the private field an implementation
    /// detail of this module.
    pub(crate) fn from_handle(inner: Handle<dyn OptionletVolatilityStructure>) -> Self {
        PyOptionletVolatilityStructure { inner }
    }
}

/// Python `ConstantOptionletVolatility`: a single caplet volatility with no
/// option-time or strike dependence
/// (`termstructures::volatility::ConstantOptionletVolatility`).
///
/// Extends [`PyOptionletVolatilityStructure`] and supplies only the
/// constructors; the query surface is inherited. Unbounded in time and strike,
/// so queries never need extrapolation enabled. [`new`](Self::new) and
/// [`with_quote`](Self::with_quote) pin the reference date;
/// [`moving`](Self::moving) and [`moving_with_quote`](Self::moving_with_quote)
/// float it off the `Settings` evaluation date (#627).
#[pyclass(name = "ConstantOptionletVolatility", extends = PyOptionletVolatilityStructure, unsendable)]
pub struct PyConstantOptionletVolatility;

#[pymethods]
impl PyConstantOptionletVolatility {
    /// A constant surface at a fixed `volatility`, wrapped in an internal quote
    /// the caller cannot later mutate.
    #[new]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, displacement = 0.0))]
    fn new(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: f64,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        displacement: f64,
    ) -> PyClassInitializer<Self> {
        let surface = shared(ConstantOptionletVolatility::new(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility,
            day_counter.inner(),
            volatility_type.inner(),
            displacement,
        )) as Shared<dyn OptionletVolatilityStructure>;
        PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
            surface,
        )))
        .add_subclass(PyConstantOptionletVolatility)
    }

    /// A constant surface reading `volatility` from the caller's quote; a later
    /// `set_value` on that quote notifies the surface's observers.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, displacement = 0.0))]
    fn with_quote(
        py: Python<'_>,
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: &PySimpleQuote,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        displacement: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantOptionletVolatility::with_quote(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility.handle(),
            day_counter.inner(),
            volatility_type.inner(),
            displacement,
        )) as Shared<dyn OptionletVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantOptionletVolatility),
        )
    }

    /// A constant surface whose reference date floats off `settings`'
    /// evaluation date by `settlement_days` on `calendar`, at a fixed
    /// `volatility` wrapped in an internal quote the caller cannot later mutate.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (settlement_days, calendar, business_day_convention, volatility, day_counter, volatility_type, settings, displacement = 0.0))]
    fn moving(
        py: Python<'_>,
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: f64,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        settings: &PySettings,
        displacement: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantOptionletVolatility::moving(
            settlement_days,
            calendar.inner(),
            business_day_convention.inner(),
            volatility,
            day_counter.inner(),
            volatility_type.inner(),
            displacement,
            settings.inner(),
        )) as Shared<dyn OptionletVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantOptionletVolatility),
        )
    }

    /// A constant surface whose reference date floats off `settings`'
    /// evaluation date, reading `volatility` from the caller's quote; a later
    /// `set_value` on that quote notifies the surface's observers.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (settlement_days, calendar, business_day_convention, volatility, day_counter, volatility_type, settings, displacement = 0.0))]
    fn moving_with_quote(
        py: Python<'_>,
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: &PySimpleQuote,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        settings: &PySettings,
        displacement: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantOptionletVolatility::moving_with_quote(
            settlement_days,
            calendar.inner(),
            business_day_convention.inner(),
            volatility.handle(),
            day_counter.inner(),
            volatility_type.inner(),
            displacement,
            settings.inner(),
        )) as Shared<dyn OptionletVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantOptionletVolatility),
        )
    }
}

/// Python `OptionletStripper1`: bootstraps caplet volatilities out of a market
/// cap/floor term-volatility surface
/// (`termstructures::volatility::OptionletStripper1`).
///
/// The stripper is not itself a volatility surface: it produces a grid of
/// caplet volatilities that [`PyStrippedOptionletAdapter`] interpolates into
/// one. It prices a cap at each of its own lengths off `term_vol_surface`,
/// differences consecutive prices into a single caplet price, and inverts that
/// for the caplet's implied volatility.
///
/// Stripping is lazy and cached: nothing runs until a query needs the grid, and
/// it re-runs only when a surface quote or the index changes. The
/// [`Normal`](crate::swaptionvol::PyVolatilityType::Normal) model is deferred
/// (#440/#577) and fails at the strip rather than at construction.
#[pyclass(name = "OptionletStripper1", unsendable)]
pub struct PyOptionletStripper1 {
    inner: Shared<OptionletStripper1>,
}

#[pymethods]
impl PyOptionletStripper1 {
    /// A stripper over `term_vol_surface` and `ibor_index`.
    ///
    /// `term_vol_surface` must be one of the MOVING forms: the adapter reads
    /// its settlement days back off the surface, and a pinned-reference surface
    /// has none. `discount` is the curve the caps are priced on; `None` falls
    /// back to the index's own forwarding curve. `accuracy` and `max_iter` size
    /// the implied-volatility solve, and `optionlet_frequency` overrides the
    /// index tenor as the caplet step.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (term_vol_surface, ibor_index, volatility_type, accuracy = 1e-6, max_iter = 100, displacement = 0.0, discount = None, optionlet_frequency = None))]
    fn new(
        term_vol_surface: &PyCapFloorTermVolSurface,
        ibor_index: &PyIborIndex,
        volatility_type: PyVolatilityType,
        accuracy: f64,
        max_iter: u32,
        displacement: f64,
        discount: Option<&PyYieldTermStructure>,
        optionlet_frequency: Option<&PyPeriod>,
    ) -> PyResult<Self> {
        let discount = match discount {
            Some(curve) => curve.handle(),
            None => Handle::<dyn YieldTermStructure>::empty(),
        };
        Ok(PyOptionletStripper1 {
            inner: shared(
                OptionletStripper1::new(
                    term_vol_surface.inner(),
                    ibor_index.inner(),
                    discount,
                    accuracy,
                    max_iter,
                    volatility_type.inner(),
                    displacement,
                    optionlet_frequency.map(|period| period.inner()),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// The floating switch strike: the mean at-the-money caplet rate, which
    /// decides whether each strike is stripped out of caps or out of floors.
    ///
    /// Fallible, and the first call triggers the strip.
    fn switch_strike(&self) -> PyResult<f64> {
        Ok(self.inner.switch_strike().map_err(PyQlError::from)?)
    }

    /// The at-the-money forward rate of each caplet, one per maturity.
    fn atm_optionlet_rates(&self) -> PyResult<Vec<f64>> {
        Ok(self.inner.atm_optionlet_rates().map_err(PyQlError::from)?)
    }
}

impl PyOptionletStripper1 {
    /// The wrapped stripper, erased to the trait the adapter takes.
    fn erased(&self) -> Shared<dyn StrippedOptionletBase> {
        Shared::clone(&self.inner) as Shared<dyn StrippedOptionletBase>
    }
}

/// Python `StrippedOptionletAdapter`: serves a stripper's caplet volatility
/// grid as an [`PyOptionletVolatilityStructure`]
/// (`termstructures::volatility::StrippedOptionletAdapter`).
///
/// This is what closes the cap/floor volatility loop: a
/// [`PyBlackCapFloorEngine`](crate::capfloorengine::PyBlackCapFloorEngine)
/// built on this surface reprices the caps the term volatilities were quoted
/// on. Linear in strike within each maturity, then linear across maturities.
///
/// Extends [`PyOptionletVolatilityStructure`] and supplies only the
/// constructor; the query surface is inherited. Its reference date floats off
/// the evaluation date carried by `settings`, advanced by the settlement days
/// the underlying term-volatility surface carries. The surface ends at the last
/// caplet fixing, so pricing a cap that reaches it wants
/// `enable_extrapolation()`.
#[pyclass(name = "StrippedOptionletAdapter", extends = PyOptionletVolatilityStructure, unsendable)]
pub struct PyStrippedOptionletAdapter;

#[pymethods]
impl PyStrippedOptionletAdapter {
    /// The interpolated surface over `stripper`.
    ///
    /// Fallible, and it strips eagerly: the constructor reads the caplet
    /// strikes and fixing dates to snapshot its strike domain and maximum date.
    /// It fails on a stripper whose term-volatility surface carries no
    /// settlement days, which is every pinned-reference surface.
    #[new]
    fn new(
        stripper: &PyOptionletStripper1,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let adapter = shared(
            StrippedOptionletAdapter::new(stripper.erased(), settings.inner())
                .map_err(PyQlError::from)?,
        ) as Shared<dyn OptionletVolatilityStructure>;
        Ok(
            PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
                adapter,
            )))
            .add_subclass(PyStrippedOptionletAdapter),
        )
    }
}
