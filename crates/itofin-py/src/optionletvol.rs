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

/// Shared base for every caplet/floorlet volatility surface: volatility,
/// Black variance and the lognormal displacement.
///
/// A single option axis, unlike the swaption surfaces: a query takes one option
/// tenor (or date) and a strike.
#[pyclass(name = "OptionletVolatilityStructure", subclass, unsendable)]
pub struct PyOptionletVolatilityStructure {
    inner: Handle<dyn OptionletVolatilityStructure>,
}

#[pymethods]
impl PyOptionletVolatilityStructure {
    /// Return the caplet volatility for an option tenor and strike.
    ///
    /// Args:
    ///     option_tenor (Period): The option's tenor, resolved against the
    ///         surface's reference date and calendar.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The caplet volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn volatility(&self, option_tenor: &PyPeriod, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the caplet volatility for an option date and strike.
    ///
    /// The date form the optionlet stripper and the cap/floor engine use, both
    /// addressing the surface by a coupon's fixing date.
    ///
    /// Args:
    ///     option_date (Date): The option date the volatility is read at.
    ///     strike (float): The strike the volatility is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The caplet volatility.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
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

    /// Return the Black variance, the squared volatility times option time.
    ///
    /// Args:
    ///     option_tenor (Period): The option's tenor.
    ///     strike (float): The strike the variance is read at.
    ///     extrapolate (bool): Whether to answer outside the surface's grid.
    ///
    /// Returns:
    ///     float: The Black variance.
    ///
    /// Raises:
    ///     ItofinError: If the query falls outside the grid and extrapolation
    ///         is not allowed.
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

    /// Return whether the surface answers dates and times beyond its maximum.
    ///
    /// Returns:
    ///     bool: True when extrapolation is enabled on the surface itself.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allow extrapolation past the maximum date and time.
    ///
    /// A stripped surface ends at its last optionlet fixing, so a cap whose
    /// own last caplet fixes there queries the boundary.
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbid extrapolation past the maximum date and time.
    fn disable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .disable_extrapolation();
        Ok(())
    }

    /// Return the lognormal shift applied to forwards and strikes.
    ///
    /// This is what BlackCapFloorEngine checks a caller-supplied displacement
    /// against, so it is the number to read before pinning one on the engine.
    ///
    /// Returns:
    ///     float: The shift; zero for the unshifted lognormal and the normal
    ///         model.
    fn displacement(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .displacement())
    }

    /// Return the date every option time is measured from.
    ///
    /// Pinned at construction on the fixed-reference surfaces; derived from
    /// the Settings evaluation date (settlement days on the calendar) on the
    /// moving ones, so it follows a later set_evaluation_date.
    ///
    /// Returns:
    ///     Date: The surface's reference date.
    ///
    /// Raises:
    ///     ItofinError: On a moving surface whose Settings has no evaluation
    ///         date set.
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

/// A single caplet volatility with no option-time or strike dependence.
///
/// The constructor and with_quote pin the reference date, so every query's
/// option time runs from reference_date rather than the evaluation date. The
/// moving and moving_with_quote forms float the reference date off the
/// Settings evaluation date instead (#627).
#[pyclass(name = "ConstantOptionletVolatility", extends = PyOptionletVolatilityStructure, unsendable)]
pub struct PyConstantOptionletVolatility;

#[pymethods]
impl PyConstantOptionletVolatility {
    /// Build the surface at a fixed volatility.
    ///
    /// Args:
    ///     reference_date (Date): The date every query's option time runs
    ///         from.
    ///     calendar (Calendar): The calendar option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (float): The single volatility answered everywhere,
    ///         wrapped in an internal quote the caller cannot later mutate.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     displacement (float): The lognormal shift applied to forwards and
    ///         strikes.
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

    /// Build the surface reading its volatility from a live quote.
    ///
    /// Args:
    ///     reference_date (Date): The date every query's option time runs
    ///         from.
    ///     calendar (Calendar): The calendar option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (SimpleQuote): The volatility; a later set_value
    ///         notifies the surface's observers.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     displacement (float): The lognormal shift applied to forwards and
    ///         strikes.
    ///
    /// Returns:
    ///     ConstantOptionletVolatility: The surface over that quote.
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

    /// Build the surface with a reference date floating off the evaluation date.
    ///
    /// The reference date is the evaluation date advanced by settlement_days
    /// business days on calendar, so it follows a later set_evaluation_date.
    ///
    /// Args:
    ///     settlement_days (int): Business days between the evaluation date
    ///         and the reference date.
    ///     calendar (Calendar): The calendar the reference date is derived on
    ///         and option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (float): The single volatility answered everywhere,
    ///         wrapped in an internal quote the caller cannot later mutate.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     settings (Settings): The evaluation context the reference date
    ///         floats off.
    ///     displacement (float): The lognormal shift applied to forwards and
    ///         strikes.
    ///
    /// Returns:
    ///     ConstantOptionletVolatility: The moving surface.
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

    /// Build the moving surface reading its volatility from a live quote.
    ///
    /// Args:
    ///     settlement_days (int): Business days between the evaluation date
    ///         and the reference date.
    ///     calendar (Calendar): The calendar the reference date is derived on
    ///         and option tenors resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a tenor to a date.
    ///     volatility (SimpleQuote): The volatility; a later set_value
    ///         notifies the surface's observers.
    ///     day_counter (DayCounter): The day count option times are measured
    ///         in.
    ///     volatility_type (VolatilityType): Whether the quote is
    ///         shifted-lognormal or normal.
    ///     settings (Settings): The evaluation context the reference date
    ///         floats off.
    ///     displacement (float): The lognormal shift applied to forwards and
    ///         strikes.
    ///
    /// Returns:
    ///     ConstantOptionletVolatility: The moving surface over that quote.
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

/// Bootstraps caplet volatilities out of a market cap/floor term-volatility
/// surface.
///
/// Not itself a volatility surface: it produces a grid of caplet volatilities
/// that StrippedOptionletAdapter interpolates into one. Stripping is lazy and
/// cached, and re-runs only when a surface quote or the index changes.
///
/// term_vol_surface must come from CapFloorTermVolSurface.moving or
/// moving_with_quotes; a pinned-reference surface carries no settlement days
/// and fails the adapter. VolatilityType.Normal is deferred (#440/#577) and
/// fails at the strip, not at construction.
#[pyclass(name = "OptionletStripper1", unsendable)]
pub struct PyOptionletStripper1 {
    inner: Shared<OptionletStripper1>,
}

#[pymethods]
impl PyOptionletStripper1 {
    /// Build the stripper over a term-volatility surface and an index.
    ///
    /// It prices a cap at each of its own lengths off term_vol_surface,
    /// differences consecutive prices into a single caplet price, and inverts
    /// that for the caplet's implied volatility.
    ///
    /// Args:
    ///     term_vol_surface (CapFloorTermVolSurface): The market term
    ///         volatilities; it must be one of the moving forms, a
    ///         pinned-reference surface carrying no settlement days.
    ///     ibor_index (IborIndex): The index the caplets fix off.
    ///     volatility_type (VolatilityType): The quoting convention; Normal is
    ///         deferred and fails at the strip, not here.
    ///     accuracy (float): The tolerance of the implied-volatility solve.
    ///     max_iter (int): The iteration cap of that solve.
    ///     displacement (float): The lognormal shift applied to forwards and
    ///         strikes.
    ///     discount (YieldTermStructure | None): The curve the caps are priced
    ///         on; None falls back to the index's own forwarding curve.
    ///     optionlet_frequency (Period | None): The caplet step; None uses the
    ///         index tenor.
    ///
    /// Raises:
    ///     ItofinError: On whatever the core rejects about the surface, the
    ///         index or the solve parameters.
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

    /// Return the floating switch strike, the mean at-the-money caplet rate.
    ///
    /// It decides whether each strike is stripped out of caps or out of
    /// floors. The first call triggers the strip.
    ///
    /// Returns:
    ///     float: The switch strike.
    ///
    /// Raises:
    ///     ItofinError: On a stripping failure, which a Normal volatility_type
    ///         always is.
    fn switch_strike(&self) -> PyResult<f64> {
        Ok(self.inner.switch_strike().map_err(PyQlError::from)?)
    }

    /// Return the at-the-money forward rate of each caplet.
    ///
    /// Returns:
    ///     list[float]: One rate per maturity.
    ///
    /// Raises:
    ///     ItofinError: On a stripping failure.
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

/// Serves a stripper's caplet volatility grid as an
/// OptionletVolatilityStructure: linear in strike within each maturity, then
/// linear across maturities.
///
/// This closes the cap/floor volatility loop - a BlackCapFloorEngine on this
/// surface reprices the caps the term volatilities were quoted on. The
/// reference date floats off the evaluation date carried by settings, advanced
/// by the term-volatility surface's settlement days. The surface ends at the
/// last caplet fixing, so pricing a cap that reaches it wants
/// enable_extrapolation().
#[pyclass(name = "StrippedOptionletAdapter", extends = PyOptionletVolatilityStructure, unsendable)]
pub struct PyStrippedOptionletAdapter;

#[pymethods]
impl PyStrippedOptionletAdapter {
    /// Build the interpolated surface over a stripper.
    ///
    /// It strips eagerly: the constructor reads the caplet strikes and fixing
    /// dates to snapshot its strike domain and maximum date.
    ///
    /// Args:
    ///     stripper (OptionletStripper1): The stripper whose caplet grid is
    ///         served.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the reference date floats off.
    ///
    /// Raises:
    ///     ItofinError: On a stripper whose term-volatility surface carries no
    ///         settlement days, which is every pinned-reference surface, and
    ///         on a stripping failure.
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
