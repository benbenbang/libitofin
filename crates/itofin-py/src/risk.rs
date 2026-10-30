//! Bump-and-reprice sensitivities: parallel and bucket PV01 over SimpleQuotes.
//!
//! Supported instruments: every bond (each runs the same refresh and
//! floating-coupon preflight as `Bond.npv` once, before the first reprice),
//! `VanillaSwap` and `OvernightIndexedSwap`. Deferred and rejected with
//! `TypeError`: options, swaptions, cap/floors and CDS (#1322).
use crate::PyQlError;
use crate::bonds::PyBond;
use crate::market::PySimpleQuote;
use crate::ois::PyOvernightIndexedSwap;
use crate::swap::PyVanillaSwap;
use libitofin::instrument::Instrument;
use libitofin::quotes::SimpleQuote;
use libitofin::risk::{self, DEFAULT_SHIFT, SensitivityAnalysis};
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
            } else {
                Err(PyTypeError::new_err(
                    "expected Bond, VanillaSwap or OvernightIndexedSwap; options, swaptions, \
                     cap/floors and CDS are deferred (#1322)",
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
#[gen_stub_pyfunction(module = "itofin.risk")]
#[pyfunction]
#[pyo3(signature = (quotes, instruments, quantities = Vec::new(), shift = DEFAULT_SHIFT, kind = PySensitivityAnalysis::Centered, reference_npv = None))]
pub(crate) fn parallel_analysis(
    quotes: Vec<PyRef<'_, PySimpleQuote>>,
    #[gen_stub(override_type(type_repr = "typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap]", imports = ("typing", "itofin.instruments")))]
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
#[gen_stub_pyfunction(
    module = "itofin.risk",
    python_overload = r#"
    @typing.overload
    def bucket_analysis(quotes: typing.Mapping[builtins.str, quotes.SimpleQuote], instruments: typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap], quantities: typing.Sequence[builtins.float] = [], shift: builtins.float = 0.0001, kind: SensitivityAnalysis = SensitivityAnalysis.Centered) -> builtins.dict[builtins.str, tuple[builtins.float, builtins.float | None]]: ...
    @typing.overload
    def bucket_analysis(quotes: typing.Sequence[quotes.SimpleQuote], instruments: typing.Sequence[instruments.Bond | instruments.VanillaSwap | instruments.OvernightIndexedSwap], quantities: typing.Sequence[builtins.float] = [], shift: builtins.float = 0.0001, kind: SensitivityAnalysis = SensitivityAnalysis.Centered) -> builtins.list[tuple[builtins.float, builtins.float | None]]: ...
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
