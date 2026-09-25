//! Finite-difference Black-Scholes vanilla pricing facade.

use crate::market::PyBlackScholesProcess;
use libitofin::methods::finitedifferences::solvers::FdmSchemeDesc;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::fdblackscholesvanillaengine::FdBlackScholesVanillaEngine;
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};

/// Supported rollback schemes for a one-dimensional Black-Scholes grid.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "FdScheme",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.pricingengines"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyFdScheme {
    Douglas = 0,
    ImplicitEuler = 1,
}

impl PyFdScheme {
    fn inner(self) -> FdmSchemeDesc {
        match self {
            Self::Douglas => FdmSchemeDesc::douglas(),
            Self::ImplicitEuler => FdmSchemeDesc::implicit_euler(),
        }
    }
}

/// Finite-difference engine for European, American and Bermudan vanilla options.
#[gen_stub_pyclass]
#[pyclass(
    name = "FdBlackScholesVanillaEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyFdBlackScholesVanillaEngine {
    inner: SharedMut<FdBlackScholesVanillaEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyFdBlackScholesVanillaEngine {
    /// Build a grid engine retaining the Black-Scholes process.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The market process to price under.
    ///     t_grid (int): Positive number of time steps; defaults to 100.
    ///     x_grid (int): Spatial grid size, at least 3; defaults to 100.
    ///     damping_steps (int): Initial implicit-Euler steps; defaults to 0.
    ///     scheme (FdScheme): Douglas by default; ImplicitEuler is also supported.
    ///
    /// Raises:
    ///     ValueError: If the grid dimensions are invalid or overflow.
    #[new]
    #[pyo3(signature = (process, t_grid=100, x_grid=100, damping_steps=0, scheme=PyFdScheme::Douglas))]
    fn new(
        process: &PyBlackScholesProcess,
        t_grid: usize,
        x_grid: usize,
        damping_steps: usize,
        scheme: PyFdScheme,
    ) -> PyResult<Self> {
        if t_grid == 0 || x_grid < 3 || t_grid.checked_add(damping_steps).is_none() {
            return Err(PyValueError::new_err(
                "FD grid requires t_grid > 0, x_grid >= 3, and finite step count",
            ));
        }
        Ok(Self {
            inner: shared_mut(FdBlackScholesVanillaEngine::new(
                process.inner(),
                t_grid,
                x_grid,
                damping_steps,
                scheme.inner(),
            )),
        })
    }
}

impl PyFdBlackScholesVanillaEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
