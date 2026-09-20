use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::helpers::PyRateHelper;
use crate::hullwhite::PyIborIndex;
use crate::market::PySimpleQuote;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::termstructures::yields::{BasisSwapHelperConfig, JointYieldCurves};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// A basis spread helper, with the spread paid on the base-index leg.
///
/// JointYieldCurves copies its construction inputs into private helpers. The
/// original helper remains independent and is not assigned to a joint curve;
/// its quote and date inspectors remain usable, but implied_quote needs a
/// standalone curve assignment. Quotes, indices and discount curves are retained.
#[gen_stub_pyclass]
#[pyclass(name = "IborIborBasisSwapRateHelper", extends = PyRateHelper, unsendable, module = "itofin.termstructures")]
pub struct PyIborIborBasisSwapRateHelper {
    config: BasisSwapHelperConfig,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyIborIborBasisSwapRateHelper {
    /// Construct a live basis helper fitting either the base or other index.
    #[gen_stub(override_return_type(type_repr = "IborIborBasisSwapRateHelper"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: &PySimpleQuote,
        tenor: &PyPeriod,
        settlement_days: u32,
        calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
        end_of_month: bool,
        base_index: &PyIborIndex,
        other_index: &PyIborIndex,
        discount: &PyYieldTermStructure,
        bootstrap_base_curve: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        let config = BasisSwapHelperConfig {
            quote: quote.handle(),
            tenor: tenor.inner(),
            settlement_days,
            calendar: calendar.inner(),
            convention: convention.inner(),
            end_of_month,
            base_index: base_index.inner(),
            other_index: other_index.inner(),
            discount: discount.handle(),
            bootstrap_base_curve,
        };
        let helper = config.build().map_err(PyQlError::from)?;
        Ok(
            PyClassInitializer::from(PyRateHelper::from_inner(helper))
                .add_subclass(Self { config }),
        )
    }
}

/// Two mutually coupled Discount/LogLinear/GlobalBootstrap curves.
///
/// Member 0 forecasts the base index and member 1 the other index. Basis helpers
/// must include both bootstrap sides and share the same index objects/settings.
/// Plain helper strips are reserved for this assembly and must not be reused by
/// another curve. Returned curves retain both members and the joint owner after
/// this Python object is collected. No internal helper or forecast link escapes.
#[gen_stub_pyclass]
#[pyclass(
    name = "JointYieldCurves",
    unsendable,
    module = "itofin.termstructures"
)]
pub struct PyJointYieldCurves {
    inner: JointYieldCurves,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyJointYieldCurves {
    /// Assemble both curves; numerical bootstrap errors are raised on query.
    #[new]
    #[pyo3(signature = (reference_date, first_helpers, second_helpers, basis_helpers, day_counter, accuracy = 1e-10))]
    fn new(
        reference_date: &PyDate,
        first_helpers: Vec<PyRef<PyRateHelper>>,
        second_helpers: Vec<PyRef<PyRateHelper>>,
        basis_helpers: Vec<PyRef<PyIborIborBasisSwapRateHelper>>,
        day_counter: &PyDayCounter,
        accuracy: f64,
    ) -> PyResult<Self> {
        let helpers = [first_helpers, second_helpers]
            .map(|strip| strip.iter().map(|helper| helper.inner()).collect());
        let configs: Vec<_> = basis_helpers
            .iter()
            .map(|helper| helper.config.clone())
            .collect();
        let inner = JointYieldCurves::new(
            reference_date.inner(),
            helpers,
            &configs,
            day_counter.inner(),
            accuracy,
        )
        .map_err(PyQlError::from)?;
        Ok(Self { inner })
    }

    /// Return member 0 or 1, retaining the complete joint assembly.
    fn curve(&self, member: usize) -> PyResult<PyYieldTermStructure> {
        Ok(PyYieldTermStructure::from_inner(
            self.inner.curve(member).map_err(PyQlError::from)?,
        ))
    }
}
