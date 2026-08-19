//! Facades for the cap/floor stack: the [`PyCapFloorType`] flag and the
//! [`PyCapFloor`] instrument.
//!
//! A cap or floor reaches Python two ways. The standard market builder
//! [`MakeCapFloor`] backs the constructor: a tenor, an ibor index, a single
//! strike and a forward start, the shape the core's own market fixtures use. It
//! derives the floating leg off `MakeVanillaSwap`, so a zero `forward_start`
//! drops the spot caplet (`makecapfloor.cpp:34`): the cap needs no historical
//! fixing at the evaluation date, which is what makes it buildable under the
//! explicit-fixings design (D5/D11).
//!
//! The core's raw constructors (`CapFloor::cap` / `floor` / `collar`) back the
//! three staticmethods instead. They take a leg of concrete `IborCoupon`s,
//! which [`IborLeg`](crate::cashflows::PyIborLeg) now builds (#626), so a leg
//! laid out coupon by coupon - its own notional, day counter and fixing days,
//! spot caplet kept - can be capped from Python. That is the only route to a
//! collar on this side: [`MakeCapFloor`] carries a single strike and refuses
//! one outright (`makecapfloor.rs:135`).
//!
//! Deferred (visible): the general `CapFloor::new` taking a type flag with both
//! strike vectors (`capfloor.rs:129`), which the three named constructors cover
//! between them, and the leg accessors on a built instrument beyond
//! [`coupon_count`](PyCapFloor::coupon_count).

use crate::PyQlError;
use crate::capfloorengine::PyBlackCapFloorEngine;
use crate::cashflows::PyIborLeg;
use crate::hullwhite::PyIborIndex;
use crate::settings::PySettings;
use crate::time::PyPeriod;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloor, CapFloorType, MakeCapFloor};
use pyo3::prelude::*;

/// Python `CapFloorType`: whether the instrument caps or floors its floating leg
/// (`instruments::capfloor::CapFloorType`).
///
/// A fieldless pyo3 enum. Its third variant, `Collar`, reaches an instrument
/// only through a raw coupon-vector constructor - [`PyCapFloor::collar`] here,
/// or the
/// [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor) ones on
/// the inflation side. [`MakeCapFloor`] refuses it, so the flag is not
/// accepted by [`PyCapFloor::new`].
#[pyclass(name = "CapFloorType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCapFloorType {
    Cap,
    Floor,
    Collar,
}

impl PyCapFloorType {
    /// The core [`CapFloorType`] this variant stands for.
    pub(crate) fn inner(&self) -> CapFloorType {
        match self {
            PyCapFloorType::Cap => CapFloorType::Cap,
            PyCapFloorType::Floor => CapFloorType::Floor,
            PyCapFloorType::Collar => CapFloorType::Collar,
        }
    }
}

/// Python `CapFloor`: a cap or floor over a floating (ibor) leg
/// (`instruments::capfloor::CapFloor`).
///
/// Built two ways. The constructor runs [`MakeCapFloor`] (fallible: it derives
/// the floating leg, so a degenerate schedule or an unset evaluation date
/// surfaces as an `ItofinError`), whose leg carries a unit nominal and drops the
/// spot caplet. The [`cap`](Self::cap), [`floor`](Self::floor) and
/// [`collar`](Self::collar) staticmethods take a leg the caller laid out through
/// [`IborLeg`](crate::cashflows::PyIborLeg) instead, and cap exactly it. Either
/// way the core pads a short strike list across every coupon.
///
/// Pricing needs an engine: call
/// [`set_black_engine`](Self::set_black_engine) before [`npv`](Self::npv).
#[pyclass(name = "CapFloor", unsendable)]
pub struct PyCapFloor {
    inner: CapFloor,
}

#[pymethods]
impl PyCapFloor {
    /// A standard market cap or floor of `tenor` on `ibor_index`, struck at
    /// `strike` and starting `forward_start` after spot.
    ///
    /// A zero `forward_start` excludes the spot caplet, so the leg is one coupon
    /// shorter than the schedule; see the module docs.
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

    /// A cap over the coupons `leg` builds, struck at `cap_rates`.
    ///
    /// The strike list is padded to the leg length by repeating its last entry,
    /// so a single rate caps every coupon. Unlike [`new`](Self::new) this keeps
    /// whatever leg it is given: the spot caplet stays, and the leg's own
    /// notional, day counter and fixing days reach the coupons.
    ///
    /// # Errors
    ///
    /// Reports an empty `cap_rates` list, and whatever building the leg's
    /// coupons reports - a missing notional above all.
    #[staticmethod]
    fn cap(leg: &PyIborLeg, cap_rates: Vec<f64>, settings: &PySettings) -> PyResult<Self> {
        Ok(PyCapFloor {
            inner: CapFloor::cap(leg.coupons()?, cap_rates, settings.inner())
                .map_err(PyQlError::from)?,
        })
    }

    /// A floor over the coupons `leg` builds, struck at `floor_rates`. Padded
    /// and fallible as [`cap`](Self::cap).
    #[staticmethod]
    fn floor(leg: &PyIborLeg, floor_rates: Vec<f64>, settings: &PySettings) -> PyResult<Self> {
        Ok(PyCapFloor {
            inner: CapFloor::floor(leg.coupons()?, floor_rates, settings.inner())
                .map_err(PyQlError::from)?,
        })
    }

    /// A collar over the coupons `leg` builds: long the cap at `cap_rates`,
    /// short the floor at `floor_rates`, so it is worth the one less the other.
    ///
    /// The only route to a collar over a floating leg; see the module docs.
    /// Both lists are padded and both are required. Fallible as
    /// [`cap`](Self::cap), reporting either list empty.
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

    /// The cap strikes, one per coupon; empty for a floor.
    fn cap_rates(&self) -> Vec<f64> {
        self.inner.cap_rates().to_vec()
    }

    /// The floor strikes, one per coupon; empty for a cap.
    fn floor_rates(&self) -> Vec<f64> {
        self.inner.floor_rates().to_vec()
    }

    /// The number of optionlets, one per floating coupon.
    fn coupon_count(&self) -> usize {
        self.inner.coupons().len()
    }

    /// Attaches a [`PyBlackCapFloorEngine`] so the cap/floor prices each
    /// optionlet off an optionlet volatility surface.
    ///
    /// The engine is built separately and installed here, so the same engine can
    /// be shared across instruments. It must resolve its dates against the same
    /// `Settings` object this cap/floor was built with: two different settings
    /// would price the leg and the optionlets on different dates without any
    /// error being raised.
    fn set_black_engine(&mut self, engine: &PyBlackCapFloorEngine) {
        self.inner.base_mut().set_pricing_engine(engine.engine());
    }

    /// The cap/floor NPV under the attached engine.
    ///
    /// Fallible: with no engine attached the core reports `"null pricing
    /// engine"` as an `ItofinError`.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }
}
