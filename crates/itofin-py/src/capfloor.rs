//! Facades for the cap/floor stack: the CapFloorType flag and the CapFloor
//! instrument.
//!
//! A cap or floor reaches Python two ways. The standard market builder
//! MakeCapFloor backs the constructor: a tenor, an ibor index, a single strike
//! and a forward start, the shape the core's own market fixtures use. It
//! derives the floating leg off `MakeVanillaSwap`, so a zero `forward_start`
//! drops the spot caplet (`makecapfloor.cpp:34`): the cap needs no historical
//! fixing at the evaluation date, which is what makes it buildable under the
//! explicit-fixings design (D5/D11).
//!
//! The core's raw cap, floor and collar constructors back the three
//! staticmethods instead. They take a leg of concrete `IborCoupon`s,
//! which IborLeg now builds (#626), so a leg laid out coupon by coupon - its
//! own notional, day counter and fixing days, spot caplet kept - can be capped
//! from Python. That is the only route to a collar on this side: MakeCapFloor
//! carries a single strike and refuses one outright.
//!
//! Deferred (visible): the general constructor taking a type flag with both
//! strike vectors, which the three named constructors cover between them, and
//! the leg accessors on a built instrument beyond coupon_count().

use crate::PyQlError;
use crate::capfloorengine::PyBlackCapFloorEngine;
use crate::cashflows::PyIborLeg;
use crate::hullwhite::PyIborIndex;
use crate::results::Results;
use crate::settings::PySettings;
use crate::time::PyPeriod;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloor, CapFloorType, MakeCapFloor};
use pyo3::prelude::*;

/// Whether the instrument caps, floors or collars its floating leg.
///
/// Collar reaches an instrument only through a raw coupon-vector constructor:
/// CapFloor.collar here, or the YoYInflationCapFloor ones on the inflation
/// side. MakeCapFloor refuses it, so CapFloor(...) does not accept it.
#[pyclass(name = "CapFloorType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCapFloorType {
    Cap,
    Floor,
    Collar,
}

impl PyCapFloorType {
    /// The core CapFloorType this variant stands for.
    pub(crate) fn inner(&self) -> CapFloorType {
        match self {
            PyCapFloorType::Cap => CapFloorType::Cap,
            PyCapFloorType::Floor => CapFloorType::Floor,
            PyCapFloorType::Collar => CapFloorType::Collar,
        }
    }
}

/// A cap, floor or collar over a floating (ibor) leg.
///
/// The constructor runs the standard market builder MakeCapFloor: its leg
/// carries a unit nominal and one strike, and a zero forward_start excludes the
/// spot caplet, so the leg is one coupon shorter than the schedule - that is
/// what lets the cap price without a historical index fixing at the evaluation
/// date.
///
/// The cap/floor/collar staticmethods take an IborLeg the caller laid out
/// instead and cap exactly it, spot caplet and all. They are the only route to
/// a collar on this side, and the route a hand-built leg's own notional, day
/// counter and fixing days reach the coupons by. Either way the core pads a
/// short strike list across every coupon by repeating its last entry.
///
/// Pricing needs an engine: call set_black_engine() before npv().
#[pyclass(name = "CapFloor", unsendable)]
pub struct PyCapFloor {
    inner: CapFloor,
}

#[pymethods]
impl PyCapFloor {
    /// Build a standard market cap or floor through MakeCapFloor.
    ///
    /// Args:
    ///     cap_floor_type (CapFloorType): Cap or Floor; the builder refuses
    ///         Collar.
    ///     tenor (Period): The length of the capped leg.
    ///     ibor_index (IborIndex): The index the floating leg fixes off.
    ///     strike (float): The single strike, padded across every coupon.
    ///     forward_start (Period): The delay before the leg starts; a zero
    ///         period excludes the spot caplet.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If cap_floor_type is Collar, if the derived schedule
    ///         is degenerate, or if the start has to be derived and no
    ///         evaluation date is set.
    #[new]
    fn new(
        cap_floor_type: PyCapFloorType,
        tenor: &PyPeriod,
        ibor_index: &PyIborIndex,
        strike: f64,
        forward_start: &PyPeriod,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(PyCapFloor {
            inner: MakeCapFloor::new(
                cap_floor_type.inner(),
                tenor.inner(),
                ibor_index.inner(),
                strike,
                forward_start.inner(),
                settings.inner(),
            )
            .build()
            .map_err(PyQlError::from)?,
        })
    }

    /// Build a cap over the coupons leg builds, struck at cap_rates.
    ///
    /// Unlike the constructor this keeps whatever leg it is given: the spot
    /// caplet stays, and the leg's own notional, day counter and fixing days
    /// reach the coupons.
    ///
    /// Args:
    ///     leg (IborLeg): The leg whose coupons are capped.
    ///     cap_rates (list[float]): The cap strikes, padded to the leg length
    ///         by repeating the last entry.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     CapFloor: The cap over that leg.
    ///
    /// Raises:
    ///     ItofinError: On an empty cap_rates list, or on whatever building
    ///         the leg's coupons reports, a missing notional above all.
    #[staticmethod]
    fn cap(leg: &PyIborLeg, cap_rates: Vec<f64>, settings: &PySettings) -> PyResult<Self> {
        Ok(PyCapFloor {
            inner: CapFloor::cap(leg.coupons()?, cap_rates, settings.inner())
                .map_err(PyQlError::from)?,
        })
    }

    /// Build a floor over the coupons leg builds, struck at floor_rates.
    ///
    /// Args:
    ///     leg (IborLeg): The leg whose coupons are floored.
    ///     floor_rates (list[float]): The floor strikes, padded as cap() pads.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     CapFloor: The floor over that leg.
    ///
    /// Raises:
    ///     ItofinError: Fallible as cap(), on an empty list or a leg whose
    ///         coupons cannot be built.
    #[staticmethod]
    fn floor(leg: &PyIborLeg, floor_rates: Vec<f64>, settings: &PySettings) -> PyResult<Self> {
        Ok(PyCapFloor {
            inner: CapFloor::floor(leg.coupons()?, floor_rates, settings.inner())
                .map_err(PyQlError::from)?,
        })
    }

    /// Build a collar: long the cap at cap_rates, short the floor at floor_rates.
    ///
    /// The collar is worth the one less the other, and this is the only route
    /// to one over a floating leg.
    ///
    /// Args:
    ///     leg (IborLeg): The leg whose coupons are collared.
    ///     cap_rates (list[float]): The cap strikes, padded as cap() pads.
    ///     floor_rates (list[float]): The floor strikes, padded the same way.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     CapFloor: The collar over that leg.
    ///
    /// Raises:
    ///     ItofinError: On either list being empty, both being required, or on
    ///         a leg whose coupons cannot be built.
    #[staticmethod]
    fn collar(
        leg: &PyIborLeg,
        cap_rates: Vec<f64>,
        floor_rates: Vec<f64>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(PyCapFloor {
            inner: CapFloor::collar(leg.coupons()?, cap_rates, floor_rates, settings.inner())
                .map_err(PyQlError::from)?,
        })
    }

    /// Return the cap strikes, one per coupon.
    ///
    /// Returns:
    ///     list[float]: The cap strikes; empty for a floor.
    fn cap_rates(&self) -> Vec<f64> {
        self.inner.cap_rates().to_vec()
    }

    /// Return the floor strikes, one per coupon.
    ///
    /// Returns:
    ///     list[float]: The floor strikes; empty for a cap.
    fn floor_rates(&self) -> Vec<f64> {
        self.inner.floor_rates().to_vec()
    }

    /// Return the number of optionlets.
    ///
    /// Returns:
    ///     int: One per floating coupon on the leg.
    fn coupon_count(&self) -> usize {
        self.inner.coupons().len()
    }

    /// Attach a Black engine, pricing each optionlet off a volatility surface.
    ///
    /// The engine is built separately, so the same one can be shared across
    /// instruments. It must resolve its dates against the same Settings object
    /// as this cap/floor: two different settings would price the leg and the
    /// optionlets on different dates with no error raised.
    ///
    /// Args:
    ///     engine (BlackCapFloorEngine): The engine and its optionlet
    ///         volatility surface.
    fn set_black_engine(&mut self, engine: &PyBlackCapFloorEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the engine refuses the instrument.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self.inner.calculate().map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// The Black engine observes its volatility handle, so moving a quote the
    /// engine was built over reaches the cap and flips this back to False.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }

    /// Attach engine and return the NPV.
    ///
    /// Args:
    ///     engine (BlackCapFloorEngine): The engine to install and price on.
    ///
    /// Returns:
    ///     float: The cap/floor value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, engine: &PyBlackCapFloorEngine) -> PyResult<f64> {
        self.set_black_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Return a frozen snapshot of the valuation, calculating first.
    ///
    /// Returns:
    ///     Results: A copy of the valuation results.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        Ok(Results::snapshot(self.inner.base()))
    }

    /// Return the cap/floor NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, which the core reports as
    ///         "null pricing engine".
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }
}
