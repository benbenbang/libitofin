//! Concrete default-density curves with explicit interpolation conventions.
use crate::PyQlError;
use crate::credit::PyDefaultProbabilityTermStructure;
use crate::credithelpers::PyDefaultProbabilityHelper;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::{flat::BackwardFlat, linear::Linear};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::credit::{
    defaulttermstructure::DefaultProbabilityTermStructure,
    interpolateddefaultdensitycurve::InterpolatedDefaultDensityCurve,
    piecewisedefaultcurve::PiecewiseDefaultCurve, probabilitytraits::DefaultDensity,
};
use libitofin::time::date::Date;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// A density curve using BackwardFlat or Linear interpolation and flat-density extrapolation.
#[gen_stub_pyclass]
#[pyclass(name = "InterpolatedDefaultDensityCurve", extends = PyDefaultProbabilityTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyInterpolatedDefaultDensityCurve {
    dates: Vec<Date>,
    times: Vec<f64>,
    densities: Vec<f64>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyInterpolatedDefaultDensityCurve {
    /// Build from dated density nodes; interpolation is BackwardFlat or Linear.
    #[new]
    #[gen_stub(override_return_type(type_repr = "InterpolatedDefaultDensityCurve"))]
    #[pyo3(signature = (dates, densities, day_counter, interpolation = "BackwardFlat", calendar = None))]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        densities: Vec<f64>,
        day_counter: &PyDayCounter,
        interpolation: &str,
        calendar: Option<&PyCalendar>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|date| date.inner()).collect();
        let calendar = calendar.map(PyCalendar::inner);
        let (curve, times): (Shared<dyn DefaultProbabilityTermStructure>, Vec<f64>) =
            match interpolation {
                "BackwardFlat" => {
                    let curve = shared(
                        InterpolatedDefaultDensityCurve::with_calendar(
                            dates.clone(),
                            densities.clone(),
                            day_counter.inner(),
                            calendar,
                            BackwardFlat,
                        )
                        .map_err(PyQlError::from)?,
                    );
                    let times = curve.times().to_vec();
                    (curve, times)
                }
                "Linear" => {
                    let curve = shared(
                        InterpolatedDefaultDensityCurve::with_calendar(
                            dates.clone(),
                            densities.clone(),
                            day_counter.inner(),
                            calendar,
                            Linear,
                        )
                        .map_err(PyQlError::from)?,
                    );
                    let times = curve.times().to_vec();
                    (curve, times)
                }
                _ => {
                    return Err(PyValueError::new_err(
                        "interpolation must be BackwardFlat or Linear",
                    ));
                }
            };
        Ok(
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(Self {
                dates,
                times,
                densities,
            }),
        )
    }

    /// Return copied node dates.
    fn dates(&self) -> Vec<PyDate> {
        self.dates.iter().copied().map(PyDate::from_inner).collect()
    }

    /// Return copied node times.
    fn times(&self) -> Vec<f64> {
        self.times.clone()
    }

    /// Return copied node densities.
    fn data(&self) -> Vec<f64> {
        self.densities.clone()
    }

    /// Return copied node densities.
    fn default_densities(&self) -> Vec<f64> {
        self.data()
    }

    /// Return copied date and density pairs.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.dates().into_iter().zip(self.data()).collect()
    }
}

enum DensityBootstrap {
    BackwardFlat(Shared<PiecewiseDefaultCurve<DefaultDensity, BackwardFlat>>),
    Linear(Shared<PiecewiseDefaultCurve<DefaultDensity, Linear>>),
}

/// A lazy CDS bootstrap solving default-density nodes with BackwardFlat or Linear interpolation.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseDefaultDensityCurve", extends = PyDefaultProbabilityTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseDefaultDensityCurve {
    concrete: DensityBootstrap,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseDefaultDensityCurve {
    /// Retain helpers and bootstrap lazily; quote changes invalidate the solved nodes.
    #[new]
    #[gen_stub(override_return_type(type_repr = "PiecewiseDefaultDensityCurve"))]
    #[pyo3(signature = (reference_date, helpers, day_counter, interpolation = "BackwardFlat"))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyDefaultProbabilityHelper>>,
        day_counter: &PyDayCounter,
        interpolation: &str,
    ) -> PyResult<PyClassInitializer<Self>> {
        let helpers = helpers.iter().map(|helper| helper.shared()).collect();
        let (curve, concrete): (Shared<dyn DefaultProbabilityTermStructure>, _) =
            match interpolation {
                "BackwardFlat" => {
                    let curve = PiecewiseDefaultCurve::<DefaultDensity, BackwardFlat>::new(
                        reference_date.inner(),
                        helpers,
                        day_counter.inner(),
                        BackwardFlat,
                    )
                    .map_err(PyQlError::from)?;
                    (curve.clone(), DensityBootstrap::BackwardFlat(curve))
                }
                "Linear" => {
                    let curve = PiecewiseDefaultCurve::<DefaultDensity, Linear>::new(
                        reference_date.inner(),
                        helpers,
                        day_counter.inner(),
                        Linear,
                    )
                    .map_err(PyQlError::from)?;
                    (curve.clone(), DensityBootstrap::Linear(curve))
                }
                _ => {
                    return Err(PyValueError::new_err(
                        "interpolation must be BackwardFlat or Linear",
                    ));
                }
            };
        Ok(
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(Self { concrete }),
        )
    }

    /// Run the bootstrap if its cache is stale.
    fn calculate(&self) -> PyResult<()> {
        match &self.concrete {
            DensityBootstrap::BackwardFlat(curve) => curve.calculate(),
            DensityBootstrap::Linear(curve) => curve.calculate(),
        }
        .map_err(|error| PyQlError::from(error).into())
    }

    /// Return copied node dates after bootstrapping.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        let dates = match &self.concrete {
            DensityBootstrap::BackwardFlat(curve) => curve.dates(),
            DensityBootstrap::Linear(curve) => curve.dates(),
        }
        .map_err(PyQlError::from)?;
        Ok(dates.into_iter().map(PyDate::from_inner).collect())
    }

    /// Return copied node times after bootstrapping.
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(match &self.concrete {
            DensityBootstrap::BackwardFlat(curve) => curve.times(),
            DensityBootstrap::Linear(curve) => curve.times(),
        }
        .map_err(PyQlError::from)?)
    }

    /// Return copied solved densities after bootstrapping.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(match &self.concrete {
            DensityBootstrap::BackwardFlat(curve) => curve.data(),
            DensityBootstrap::Linear(curve) => curve.data(),
        }
        .map_err(PyQlError::from)?)
    }

    /// Return copied solved densities after bootstrapping.
    fn default_densities(&self) -> PyResult<Vec<f64>> {
        self.data()
    }

    /// Return copied date and density pairs after bootstrapping.
    fn nodes(&self) -> PyResult<Vec<(PyDate, f64)>> {
        Ok(self.dates()?.into_iter().zip(self.data()?).collect())
    }
}
