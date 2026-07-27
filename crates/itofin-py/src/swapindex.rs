//! Facade for the swap-rate index [`PySwapIndex`] (`indexes::SwapIndex`), the
//! index whose fixing is a vanilla swap's fair rate.
//!
//! The swaption volatility cubes take two of these (a long and a short base) and
//! read the at-the-money forward off them, so this is the index the cube facades
//! stack on rather than one the Python user prices with directly.
//!
//! Deferred (visible): the currency is hard-coded to EUR. There is no
//! `PyCurrency` facade yet, and the index's currency is inert for every ported
//! consumer - `underlying_swap` never reads it, and the cube's at-the-money
//! forward is a fair rate, not an amount. A `PyCurrency` ticket carries the
//! general form; until then a non-EUR index is not expressible here. The
//! `clone` family (re-curving / re-tenoring) is deferred in the core itself.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyEuribor;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::currency::Currency;
use libitofin::indexes::{Index, SwapIndex};
use libitofin::shared::{Shared, shared};
use libitofin::types::Natural;
use pyo3::prelude::*;

/// Python `SwapIndex`: the index whose fixing is the fair rate of an on-the-fly
/// vanilla swap (`indexes::SwapIndex`).
///
/// The swap is assembled from the index tenor, the forecasting `Euribor` index
/// and the fixed-leg conventions, off the value date the fixing date implies.
/// [`new`](PySwapIndex::new) forecasts and discounts off the ibor index's
/// forwarding curve; [`with_exogenous_discount`](PySwapIndex::with_exogenous_discount)
/// discounts off a separate curve.
#[pyclass(name = "SwapIndex", unsendable)]
pub struct PySwapIndex {
    inner: Shared<SwapIndex>,
}

#[pymethods]
impl PySwapIndex {
    /// A swap index forecasting and discounting off `ibor_index`'s forwarding
    /// curve, registering with that index so a relinked curve notifies
    /// observers.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (family_name, tenor, settlement_days, calendar, fixed_leg_tenor, fixed_leg_convention, fixed_leg_day_counter, ibor_index, settings))]
    fn new(
        family_name: String,
        tenor: &PyPeriod,
        settlement_days: Natural,
        calendar: &PyCalendar,
        fixed_leg_tenor: &PyPeriod,
        fixed_leg_convention: &PyBusinessDayConvention,
        fixed_leg_day_counter: &PyDayCounter,
        ibor_index: &PyEuribor,
        settings: &PySettings,
    ) -> Self {
        PySwapIndex {
            inner: shared(SwapIndex::new(
                family_name,
                tenor.inner(),
                settlement_days,
                Currency::eur(),
                calendar.inner(),
                fixed_leg_tenor.inner(),
                fixed_leg_convention.inner(),
                fixed_leg_day_counter.inner(),
                ibor_index.inner(),
                settings.inner(),
            )),
        }
    }

    /// A swap index forecasting off `ibor_index`'s forwarding curve but
    /// discounting off the separate `discount` curve, registering with both.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (family_name, tenor, settlement_days, calendar, fixed_leg_tenor, fixed_leg_convention, fixed_leg_day_counter, ibor_index, discount, settings))]
    fn with_exogenous_discount(
        family_name: String,
        tenor: &PyPeriod,
        settlement_days: Natural,
        calendar: &PyCalendar,
        fixed_leg_tenor: &PyPeriod,
        fixed_leg_convention: &PyBusinessDayConvention,
        fixed_leg_day_counter: &PyDayCounter,
        ibor_index: &PyEuribor,
        discount: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> Self {
        PySwapIndex {
            inner: shared(SwapIndex::with_exogenous_discount(
                family_name,
                tenor.inner(),
                settlement_days,
                Currency::eur(),
                calendar.inner(),
                fixed_leg_tenor.inner(),
                fixed_leg_convention.inner(),
                fixed_leg_day_counter.inner(),
                ibor_index.inner(),
                discount.handle(),
                settings.inner(),
            )),
        }
    }

    /// The index's fixing for `fixing_date`: the fair rate of the underlying
    /// swap, the number the volatility cubes read as the at-the-money forward.
    /// Fallible: an empty forwarding handle, an unset evaluation date or an
    /// invalid fixing date is an error.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// The fixed-leg tenor.
    fn fixed_leg_tenor(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.fixed_leg_tenor())
    }

    /// Whether the index discounts off a separate curve.
    fn exogenous_discount(&self) -> bool {
        self.inner.exogenous_discount()
    }
}

impl PySwapIndex {
    /// A clone of the inner index for the volatility cube facades.
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Shared<SwapIndex> {
        Shared::clone(&self.inner)
    }
}
