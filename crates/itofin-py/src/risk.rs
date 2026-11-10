//! Bump-and-reprice sensitivities: parallel, bucket and matrix bucket PV01 over
//! SimpleQuotes, and parameter sensitivities of any quotes to a SimpleQuote
//! bump.
//!
//! Supported instruments: every bond (each runs the same refresh and
//! floating-coupon preflight as `Bond.npv` once, before the first reprice),
//! `VanillaSwap`, `OvernightIndexedSwap`, `VanillaOption`, `Swaption`,
//! `CapFloor` and `CreditDefaultSwap`, the last four repricing through plain
//! `npv()`.
//!
//! Under a Monte Carlo engine each reprice carries sampling error that does not
//! shrink with the shift, so the finite differences are noisy; prefer an
//! analytic, tree or finite-difference engine for sensitivities.
use crate::PyQlError;
use crate::bonds::PyBond;
use crate::capfloor::PyCapFloor;
use crate::credit::PyCreditDefaultSwap;
use crate::market::{PyQuote, PySimpleQuote};
use crate::ois::PyOvernightIndexedSwap;
use crate::option::PyVanillaOption;
use crate::swap::PyVanillaSwap;
use crate::swaption::PySwaption;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::risk::{self, DEFAULT_SHIFT, Rows, SensitivityAnalysis};
use libitofin::shared::{Shared, SharedMut};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyMapping};
use pyo3_stub_gen::derive::{gen_stub_pyclass_enum, gen_stub_pyfunction};

/// Finite-difference scheme.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "SensitivityAnalysis",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.risk"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PySensitivityAnalysis {
    /// Forward difference: first order only.
    OneSide = 0,
    /// Central difference: first and second order.
    Centered = 1,
}
impl PySensitivityAnalysis {
    fn inner(self) -> SensitivityAnalysis {
        match self {
            Self::OneSide => SensitivityAnalysis::OneSide,
            Self::Centered => SensitivityAnalysis::Centered,
        }
    }
}

fn instruments(items: &[Bound<'_, PyAny>]) -> PyResult<Vec<SharedMut<dyn Instrument>>> {
    items
        .iter()
        .map(|item| {
            if let Ok(mut bond) = item.extract::<PyRefMut<'_, PyBond>>() {
                bond.sensitivity_instrument()
            } else if let Ok(swap) = item.extract::<PyRef<'_, PyVanillaSwap>>() {
                Ok(swap.inner() as SharedMut<dyn Instrument>)
            } else if let Ok(ois) = item.extract::<PyRef<'_, PyOvernightIndexedSwap>>() {
                Ok(ois.inner() as SharedMut<dyn Instrument>)
            } else if let Ok(option) = item.extract::<PyRef<'_, PyVanillaOption>>() {
                Ok(option.inner() as SharedMut<dyn Instrument>)
            } else if let Ok(swaption) = item.extract::<PyRef<'_, PySwaption>>() {
                Ok(swaption.inner() as SharedMut<dyn Instrument>)
            } else if let Ok(cap) = item.extract::<PyRef<'_, PyCapFloor>>() {
                Ok(cap.inner() as SharedMut<dyn Instrument>)
            } else if let Ok(cds) = item.extract::<PyRef<'_, PyCreditDefaultSwap>>() {
                Ok(cds.inner() as SharedMut<dyn Instrument>)
            } else {
                Err(PyTypeError::new_err(
                    "expected Bond, VanillaSwap, OvernightIndexedSwap, VanillaOption, Swaption, \
                     CapFloor or CreditDefaultSwap",
                ))
            }
        })
        .collect()
}
fn shared(quotes: &[PyRef<'_, PySimpleQuote>]) -> Vec<Shared<SimpleQuote>> {
    quotes.iter().map(|quote| quote.shared()).collect()
}

/// Parallel PV01: every quote bumped together by `shift`.
///
/// Returns `(first, second)`; `second` is None for `OneSide`. Empty
/// `quantities` mean unit weights; a None `reference_npv` is computed from the
/// instruments. A zero shift is an error. Quotes are restored on return.
///
/// Under a Monte Carlo engine each reprice carries sampling error that does not
/// shrink with the shift, so the result is noisy; prefer an analytic, tree or
/// finite-difference engine.
#[gen_stub_pyfunction(module = "itofin.risk")]
#[pyfunction]
#[pyo3(signature = (quotes, instruments, quantities = Vec::new(), shift = DEFAULT_SHIFT, kind = PySensitivityAnalysis::Centered, reference_npv = None))]
pub(crate) fn parallel_analysis(
    quotes: Vec<PyRef<'_, PySimpleQuote>>,
    #[gen_stub(override_type(type_repr = "typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap | instruments.VanillaOption | instruments.Swaption | instruments.CapFloor | instruments.CreditDefaultSwap]", imports = ("typing", "itofin.instruments")))]
    instruments: Vec<Bound<'_, PyAny>>,
    quantities: Vec<f64>,
    shift: f64,
    kind: PySensitivityAnalysis,
    reference_npv: Option<f64>,
) -> PyResult<(f64, Option<f64>)> {
    crate::bootstrap::ensure_quote_mutation_allowed()?;
    let instruments = self::instruments(&instruments)?;
    Ok(risk::parallel_analysis(
        &shared(&quotes),
        &instruments,
        &quantities,
        shift,
        kind.inner(),
        reference_npv,
    )
    .map_err(PyQlError::from)?)
}

/// Bucket PV01: each quote bumped alone, against one shared reference NPV.
///
/// A sequence of quotes returns a list of `(first, second)` in quote order; a
/// mapping of labels to quotes returns a dict with the same labels, in the
/// mapping's order. Same conventions as `parallel_analysis`.
///
/// Under a Monte Carlo engine each reprice carries sampling error that does not
/// shrink with the shift, so the result is noisy; prefer an analytic, tree or
/// finite-difference engine.
#[gen_stub_pyfunction(
    module = "itofin.risk",
    python_overload = r#"
    @typing.overload
    def bucket_analysis(quotes: typing.Mapping[builtins.str, quotes.SimpleQuote], instruments: typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap | instruments.VanillaOption | instruments.Swaption | instruments.CapFloor | instruments.CreditDefaultSwap], quantities: typing.Sequence[builtins.float] = [], shift: builtins.float = 0.0001, kind: SensitivityAnalysis = SensitivityAnalysis.Centered) -> builtins.dict[builtins.str, tuple[builtins.float, builtins.float | None]]: ...
    @typing.overload
    def bucket_analysis(quotes: typing.Sequence[quotes.SimpleQuote], instruments: typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap | instruments.VanillaOption | instruments.Swaption | instruments.CapFloor | instruments.CreditDefaultSwap], quantities: typing.Sequence[builtins.float] = [], shift: builtins.float = 0.0001, kind: SensitivityAnalysis = SensitivityAnalysis.Centered) -> builtins.list[tuple[builtins.float, builtins.float | None]]: ...
    "#,
    no_default_overload = true
)]
#[pyfunction]
#[pyo3(signature = (quotes, instruments, quantities = Vec::new(), shift = DEFAULT_SHIFT, kind = PySensitivityAnalysis::Centered))]
pub(crate) fn bucket_analysis<'py>(
    quotes: &Bound<'py, PyAny>,
    instruments: Vec<Bound<'py, PyAny>>,
    quantities: Vec<f64>,
    shift: f64,
    kind: PySensitivityAnalysis,
) -> PyResult<Bound<'py, PyAny>> {
    crate::bootstrap::ensure_quote_mutation_allowed()?;
    let py = quotes.py();
    let (labels, quotes) = match quotes.cast::<PyMapping>() {
        Ok(mapping) => {
            let items: Vec<(String, PyRef<'_, PySimpleQuote>)> = mapping.items()?.extract()?;
            let (labels, quotes): (Vec<String>, _) = items.into_iter().unzip();
            (Some(labels), quotes)
        }
        Err(_) => (None, quotes.extract::<Vec<PyRef<'_, PySimpleQuote>>>()?),
    };
    let instruments = self::instruments(&instruments)?;
    let (first, second) = risk::bucket_analysis_vector(
        &shared(&quotes),
        &instruments,
        &quantities,
        shift,
        kind.inner(),
    )
    .map_err(PyQlError::from)?;
    let results = first.into_iter().zip(second);
    match labels {
        Some(labels) => {
            let report = PyDict::new(py);
            for (label, result) in labels.into_iter().zip(results) {
                report.set_item(label, result)?;
            }
            Ok(report.into_any())
        }
        None => results.collect::<Vec<_>>().into_pyobject(py),
    }
}

/// Bucket PV01 over a ragged matrix of quotes, each bumped alone, against one
/// shared reference NPV.
///
/// Returns a list of lists of `(first, second)` shaped like `quotes`. Empty
/// rows are allowed and stay empty; an empty outer sequence is an error. Same
/// conventions as `parallel_analysis`.
///
/// Under a Monte Carlo engine each reprice carries sampling error that does not
/// shrink with the shift, so the result is noisy; prefer an analytic, tree or
/// finite-difference engine.
#[gen_stub_pyfunction(module = "itofin.risk")]
#[pyfunction]
#[pyo3(signature = (quotes, instruments, quantities = Vec::new(), shift = DEFAULT_SHIFT, kind = PySensitivityAnalysis::Centered))]
pub(crate) fn bucket_analysis_matrix(
    quotes: Vec<Vec<PyRef<'_, PySimpleQuote>>>,
    #[gen_stub(override_type(type_repr = "typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap | instruments.VanillaOption | instruments.Swaption | instruments.CapFloor | instruments.CreditDefaultSwap]", imports = ("typing", "itofin.instruments")))]
    instruments: Vec<Bound<'_, PyAny>>,
    quantities: Vec<f64>,
    shift: f64,
    kind: PySensitivityAnalysis,
) -> PyResult<Rows<(f64, Option<f64>)>> {
    crate::bootstrap::ensure_quote_mutation_allowed()?;
    let instruments = self::instruments(&instruments)?;
    let rows: Vec<_> = quotes.iter().map(|row| shared(row)).collect();
    let (first, second) =
        risk::bucket_analysis_matrix(&rows, &instruments, &quantities, shift, kind.inner())
            .map_err(PyQlError::from)?;
    Ok(first
        .into_iter()
        .zip(second)
        .map(|(first, second)| first.into_iter().zip(second).collect())
        .collect())
}

/// One parameter's `(first, second)` derivatives; both None for a skipped slot.
type ParameterSlot = (Option<f64>, Option<f64>);

fn handles(parameters: &[PyRef<'_, PyQuote>]) -> Vec<Handle<dyn Quote>> {
    parameters
        .iter()
        .map(|parameter| parameter.handle())
        .collect()
}

fn finite_inputs(
    reference_values: Option<&[Option<f64>]>,
    shift: f64,
) -> libitofin::errors::QlResult<()> {
    let references = reference_values.into_iter().flatten().flatten();
    libitofin::require!(
        shift.is_finite() && references.copied().all(f64::is_finite),
        "non-finite rate or amount"
    );
    Ok(())
}

/// Sensitivities of `parameters` to a bump of `quote` by `shift`.
///
/// Returns one `(first, second)` per parameter, in parameter order; `second`
/// is None for `OneSide`. Parameters may be any Quote, typically a
/// DerivedQuote or CompositeQuote built on `quote`. A None `reference_values`
/// reads them from the parameters before the bump; otherwise it needs one
/// entry per parameter, and a None entry skips its parameter, giving
/// `(None, None)`. A zero or non-finite shift, or a non-finite reference value,
/// is an error. The quote is restored on return.
///
/// There is no QuantLib test-suite oracle: the parameter forms were removed
/// upstream. An invalid parameter would also be skipped, but no binding can
/// build an invalid quote.
#[gen_stub_pyfunction(module = "itofin.risk")]
#[pyfunction]
#[pyo3(signature = (quote, parameters, reference_values = None, shift = DEFAULT_SHIFT, kind = PySensitivityAnalysis::Centered))]
pub(crate) fn parameter_sensitivity(
    quote: PyRef<'_, PySimpleQuote>,
    parameters: Vec<PyRef<'_, PyQuote>>,
    reference_values: Option<Vec<Option<f64>>>,
    shift: f64,
    kind: PySensitivityAnalysis,
) -> PyResult<Vec<ParameterSlot>> {
    crate::bootstrap::ensure_quote_mutation_allowed()?;
    finite_inputs(reference_values.as_deref(), shift).map_err(PyQlError::from)?;
    let (first, second) = risk::parameter_sensitivity(
        &quote.shared(),
        &handles(&parameters),
        reference_values.as_deref(),
        shift,
        kind.inner(),
    )
    .map_err(PyQlError::from)?;
    Ok(first.into_iter().zip(second).collect())
}

/// `parameter_sensitivity` for each quote, bumped one at a time, with the
/// reference values read once from the parameters.
///
/// Returns one list of `(first, second)` per quote, each in parameter order.
/// An empty `quotes` is an error. Same conventions as `parameter_sensitivity`.
#[gen_stub_pyfunction(module = "itofin.risk")]
#[pyfunction]
#[pyo3(signature = (quotes, parameters, shift = DEFAULT_SHIFT, kind = PySensitivityAnalysis::Centered))]
pub(crate) fn parameter_sensitivity_vector(
    quotes: Vec<PyRef<'_, PySimpleQuote>>,
    parameters: Vec<PyRef<'_, PyQuote>>,
    shift: f64,
    kind: PySensitivityAnalysis,
) -> PyResult<Rows<ParameterSlot>> {
    crate::bootstrap::ensure_quote_mutation_allowed()?;
    finite_inputs(None, shift).map_err(PyQlError::from)?;
    let (first, second) = risk::parameter_sensitivity_vector(
        &shared(&quotes),
        &handles(&parameters),
        shift,
        kind.inner(),
    )
    .map_err(PyQlError::from)?;
    Ok(first
        .into_iter()
        .zip(second)
        .map(|(first, second)| first.into_iter().zip(second).collect())
        .collect())
}
