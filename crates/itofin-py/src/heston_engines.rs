//! Concrete alternative Heston pricing engines.
use crate::{PyQlError, heston::PyHestonModel};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::coshestonengine::CosHestonEngine;
use libitofin::pricingengines::vanilla::exponentialfittinghestonengine::{
    ExponentialFittingControlVariate, ExponentialFittingHestonEngine,
};
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};

/// Control variate used by the exponentially fitted Heston quadrature.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "ExponentialFittingControlVariate",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.pricingengines"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyExponentialFittingControlVariate {
    Optimal,
    AndersenPiterbarg,
    AndersenPiterbargOptCV,
    AsymptoticChF,
    AngledContour,
    AngledContourNoCV,
}
impl PyExponentialFittingControlVariate {
    pub(crate) fn inner(self) -> ExponentialFittingControlVariate {
        match self {
            Self::Optimal => ExponentialFittingControlVariate::Optimal,
            Self::AndersenPiterbarg => ExponentialFittingControlVariate::AndersenPiterbarg,
            Self::AndersenPiterbargOptCV => {
                ExponentialFittingControlVariate::AndersenPiterbargOptCV
            }
            Self::AsymptoticChF => ExponentialFittingControlVariate::AsymptoticChF,
            Self::AngledContour => ExponentialFittingControlVariate::AngledContour,
            Self::AngledContourNoCV => ExponentialFittingControlVariate::AngledContourNoCV,
        }
    }
}

/// Fourier cosine expansion retaining its live Heston model.
#[gen_stub_pyclass]
#[pyclass(name = "CosHestonEngine", unsendable, module = "itofin.pricingengines")]
pub struct PyCosHestonEngine {
    inner: SharedMut<CosHestonEngine>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCosHestonEngine {
    /// Construct a COS engine with positive truncation width and series size.
    #[new]
    #[pyo3(signature = (model, l=16.0, n=200))]
    fn new(model: &PyHestonModel, l: f64, n: usize) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(CosHestonEngine::new(model.inner(), l, n).map_err(PyQlError::from)?),
        })
    }
    /// Return normalized log-return cumulant 1 at nonnegative time.
    fn c1(&self, t: f64) -> PyResult<f64> {
        validate_inspector(t, 0.0).map_err(PyQlError::from)?;
        validate_inspector_value(if t == 0.0 {
            0.0
        } else {
            self.inner.borrow().c1(t)
        })
        .map_err(|e| PyQlError::from(e).into())
    }

    /// Return normalized log-return cumulant 2 at nonnegative time.
    fn c2(&self, t: f64) -> PyResult<f64> {
        validate_inspector(t, 0.0).map_err(PyQlError::from)?;
        validate_inspector_value(if t == 0.0 {
            0.0
        } else {
            self.inner.borrow().c2(t)
        })
        .map_err(|e| PyQlError::from(e).into())
    }

    /// Return normalized log-return cumulant 3 at nonnegative time.
    fn c3(&self, t: f64) -> PyResult<f64> {
        validate_inspector(t, 0.0).map_err(PyQlError::from)?;
        validate_inspector_value(if t == 0.0 {
            0.0
        } else {
            self.inner.borrow().c3(t)
        })
        .map_err(|e| PyQlError::from(e).into())
    }

    /// Return normalized log-return cumulant 4 at nonnegative time.
    fn c4(&self, t: f64) -> PyResult<f64> {
        validate_inspector(t, 0.0).map_err(PyQlError::from)?;
        validate_inspector_value(if t == 0.0 {
            0.0
        } else {
            self.inner.borrow().c4(t)
        })
        .map_err(|e| PyQlError::from(e).into())
    }

    /// Return the normalized characteristic function as (real, imaginary).
    fn chf(&self, u: f64, t: f64) -> PyResult<(f64, f64)> {
        validate_inspector(t, u).map_err(PyQlError::from)?;
        let value = self.inner.borrow().chf(u, t);
        validate_inspector_value(value.re).map_err(PyQlError::from)?;
        validate_inspector_value(value.im).map_err(PyQlError::from)?;
        Ok((value.re, value.im))
    }

    /// Return the logarithm of the forward-to-spot ratio.
    fn mu_t(&self, t: f64) -> PyResult<f64> {
        validate_inspector(t, 0.0).map_err(PyQlError::from)?;
        self.inner
            .borrow()
            .mu_t(t)
            .and_then(validate_inspector_value)
            .map_err(|e| PyQlError::from(e).into())
    }
}
impl PyCosHestonEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        self.inner.clone()
    }
}

/// Exponentially fitted quadrature retaining its live Heston model.
#[gen_stub_pyclass]
#[pyclass(
    name = "ExponentialFittingHestonEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyExponentialFittingHestonEngine {
    inner: SharedMut<dyn PricingEngine>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyExponentialFittingHestonEngine {
    /// Construct the selected control variate with optional fixed scaling.
    #[new]
    #[pyo3(signature = (model, control_variate=PyExponentialFittingControlVariate::Optimal, scaling=None, alpha=-0.5))]
    fn new(
        model: &PyHestonModel,
        control_variate: PyExponentialFittingControlVariate,
        scaling: Option<f64>,
        alpha: f64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                ExponentialFittingHestonEngine::new(
                    model.inner(),
                    control_variate.inner(),
                    scaling,
                    alpha,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }
}
impl PyExponentialFittingHestonEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        self.inner.clone()
    }
}

fn validate_inspector(t: f64, u: f64) -> libitofin::errors::QlResult<()> {
    libitofin::require!(
        t.is_finite() && t >= 0.0,
        "time must be finite and nonnegative"
    );
    libitofin::require!(u.is_finite(), "frequency must be finite");
    Ok(())
}

fn validate_inspector_value(value: f64) -> libitofin::errors::QlResult<f64> {
    libitofin::require!(value.is_finite(), "COS inspector result is not finite");
    Ok(value)
}
