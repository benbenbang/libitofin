//! Facade for the swap-rate index SwapIndex, the index whose fixing is a
//! vanilla swap's fair rate.
//!
//! The swaption volatility cubes take two of these (a long and a short base) and
//! read the at-the-money forward off them, so this is the index the cube facades
//! stack on rather than one the Python user prices with directly.
//!
//! Both constructors take the currency as a Currency and the forecasting index
//! as the general IborIndex (#868). The currency stays inert for every ported
//! consumer - `underlying_swap` never reads it, and the cube's at-the-money
//! forward is a fair rate, not an amount - so currency() reads it back off the
//! core index, which is the only place it shows.
//!
//! Deferred (visible): the `clone` family (re-curving / re-tenoring) is deferred
//! in the core itself.

use crate::PyQlError;
use crate::currency::PyCurrency;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::indexes::{Index, InterestRateIndex, SwapIndex};
use libitofin::shared::{Shared, shared};
use libitofin::types::Natural;
use pyo3::prelude::*;

/// The index whose fixing is the fair rate of an on-the-fly vanilla swap,
/// assembled from the index tenor, the forecasting Ibor index and the fixed-leg
/// conventions.
///
/// The swap is assembled off the value date the fixing date implies. The
/// swaption volatility cubes take two of these (a long and a short base) and
/// read the at-the-money forward off them, so this is the index the cube
/// facades stack on rather than one priced with directly.
///
/// The currency is inert for every ported consumer, so currency() reading it
/// back off the core index is the only place it shows. Deferred (visible): the
/// clone family (re-curving / re-tenoring) is deferred in the core itself.
#[pyclass(name = "SwapIndex", unsendable)]
pub struct PySwapIndex {
    inner: Shared<SwapIndex>,
}

#[pymethods]
impl PySwapIndex {
    /// Build a swap index forecasting and discounting off one curve.
    ///
    /// Both legs use the ibor index's forwarding curve. The index registers
    /// with that index, so a relinked curve notifies observers.
    ///
    /// Args:
    ///     family_name (str): The index family the fixings are stored under.
    ///     tenor (Period): The tenor of the underlying swap.
    ///     settlement_days (int): The business days between the fixing date and
    ///         the swap's start.
    ///     currency (Currency): The index currency, inert for every ported
    ///         consumer and read back only by currency().
    ///     calendar (Calendar): The calendar the swap's dates roll on.
    ///     fixed_leg_tenor (Period): The fixed leg's payment tenor.
    ///     fixed_leg_convention (BusinessDayConvention): The fixed leg's
    ///         business-day convention.
    ///     fixed_leg_day_counter (DayCounter): The fixed leg's day count.
    ///     ibor_index (IborIndex): The index forecasting the floating leg,
    ///         whose forwarding curve also discounts.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (family_name, tenor, settlement_days, currency, calendar, fixed_leg_tenor, fixed_leg_convention, fixed_leg_day_counter, ibor_index, settings))]
    fn new(
        family_name: String,
        tenor: &PyPeriod,
        settlement_days: Natural,
        currency: &PyCurrency,
        calendar: &PyCalendar,
        fixed_leg_tenor: &PyPeriod,
        fixed_leg_convention: &PyBusinessDayConvention,
        fixed_leg_day_counter: &PyDayCounter,
        ibor_index: &PyIborIndex,
        settings: &PySettings,
    ) -> Self {
        PySwapIndex {
            inner: shared(SwapIndex::new(
                family_name,
                tenor.inner(),
                settlement_days,
                currency.inner(),
                calendar.inner(),
                fixed_leg_tenor.inner(),
                fixed_leg_convention.inner(),
                fixed_leg_day_counter.inner(),
                ibor_index.inner(),
                settings.inner(),
            )),
        }
    }

    /// Build a swap index discounting off a separate curve.
    ///
    /// The floating leg is still forecast off the ibor index's forwarding
    /// curve, but discounting uses discount. The index registers with both.
    ///
    /// Args:
    ///     family_name (str): The index family the fixings are stored under.
    ///     tenor (Period): The tenor of the underlying swap.
    ///     settlement_days (int): The business days between the fixing date and
    ///         the swap's start.
    ///     currency (Currency): The index currency, inert for every ported
    ///         consumer and read back only by currency().
    ///     calendar (Calendar): The calendar the swap's dates roll on.
    ///     fixed_leg_tenor (Period): The fixed leg's payment tenor.
    ///     fixed_leg_convention (BusinessDayConvention): The fixed leg's
    ///         business-day convention.
    ///     fixed_leg_day_counter (DayCounter): The fixed leg's day count.
    ///     ibor_index (IborIndex): The index forecasting the floating leg.
    ///     discount (YieldTermStructure): The separate curve both legs are
    ///         discounted on.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Returns:
    ///     SwapIndex: The index discounting off the exogenous curve.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (family_name, tenor, settlement_days, currency, calendar, fixed_leg_tenor, fixed_leg_convention, fixed_leg_day_counter, ibor_index, discount, settings))]
    fn with_exogenous_discount(
        family_name: String,
        tenor: &PyPeriod,
        settlement_days: Natural,
        currency: &PyCurrency,
        calendar: &PyCalendar,
        fixed_leg_tenor: &PyPeriod,
        fixed_leg_convention: &PyBusinessDayConvention,
        fixed_leg_day_counter: &PyDayCounter,
        ibor_index: &PyIborIndex,
        discount: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> Self {
        PySwapIndex {
            inner: shared(SwapIndex::with_exogenous_discount(
                family_name,
                tenor.inner(),
                settlement_days,
                currency.inner(),
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

    /// Return the underlying swap's fair rate for fixing_date.
    ///
    /// This is the at-the-money forward the volatility cubes read.
    ///
    /// Args:
    ///     fixing_date (Date): The date the underlying swap is struck off.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fair rate of the underlying swap.
    ///
    /// Raises:
    ///     ItofinError: If the forwarding handle is empty, the evaluation date
    ///         is unset, or the fixing date is invalid.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// Return the index currency, read back off the core index.
    ///
    /// Returns:
    ///     Currency: The currency the index was built with.
    fn currency(&self) -> PyCurrency {
        PyCurrency::from_inner(self.inner.currency().clone())
    }

    /// Return the fixed leg's payment tenor.
    ///
    /// Returns:
    ///     Period: The fixed-leg tenor.
    fn fixed_leg_tenor(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.fixed_leg_tenor())
    }

    /// Return whether the index discounts off a separate curve.
    ///
    /// Returns:
    ///     bool: True if the index was built by with_exogenous_discount.
    fn exogenous_discount(&self) -> bool {
        self.inner.exogenous_discount()
    }
}

impl PySwapIndex {
    /// A clone of the inner index for the volatility cube facades.
    pub(crate) fn inner(&self) -> Shared<SwapIndex> {
        Shared::clone(&self.inner)
    }
}
