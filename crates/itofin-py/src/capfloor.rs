//! Facades for the cap/floor stack: the [`PyCapFloorType`] flag and the
//! [`PyCapFloor`] instrument.
//!
//! The core [`CapFloor`] constructors (`CapFloor::cap` / `floor` / `collar`)
//! take a leg of concrete `IborCoupon`s, which Python cannot build: there is no
//! `IborLeg` facade. [`PyCapFloor`] therefore wraps the standard market builder
//! [`MakeCapFloor`] instead, the same shape the core's own cap/floor fixtures
//! use: a tenor, an ibor index, a single strike and a forward start.
//!
//! The builder derives the floating leg off `MakeVanillaSwap`, so a zero
//! `forward_start` drops the spot caplet (`makecapfloor.cpp:34`): the cap needs
//! no historical fixing at the evaluation date, which is what makes it buildable
//! under the explicit-fixings design (D5/D11).
//!
//! `CapFloorType.Collar` is exposed, but only the year-on-year inflation
//! cap/floor can be built as one: its raw constructors take a coupon vector
//! Python can now assemble (#859).
//!
//! Deferred (visible): the ibor-side collar. [`MakeCapFloor`] rejects one - it
//! has no single-strike form (`makecapfloor.rs:135`) - and the raw-leg
//! `CapFloor::collar` constructor needs an `IborLeg` facade that does not exist
//! yet, so a collar over a floating leg still has no reachable construction path
//! here. Exposing it needs that leg facade first; tracked as #626.

use crate::PyQlError;
use crate::capfloorengine::PyBlackCapFloorEngine;
use crate::hullwhite::PyEuribor;
use crate::settings::PySettings;
use crate::time::PyPeriod;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloor, CapFloorType, MakeCapFloor};
use pyo3::prelude::*;

/// Python `CapFloorType`: whether the instrument caps or floors its floating leg
/// (`instruments::capfloor::CapFloorType`).
///
/// A fieldless pyo3 enum. Its third variant, `Collar`, reaches an instrument
/// only through the raw
/// [`YoYInflationCapFloor`](crate::inflation::PyYoYInflationCapFloor)
/// constructors: see the module docs for the ibor-side deferral.
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
/// Built through [`MakeCapFloor`] (fallible: it derives the floating leg, so a
/// degenerate schedule or an unset evaluation date surfaces as an
/// `ItofinError`). The leg carries a unit nominal and one strike, padded across
/// every coupon by the core constructor.
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
        ibor_index: &PyEuribor,
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
