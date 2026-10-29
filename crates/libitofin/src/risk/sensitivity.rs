//! Bump-and-reprice sensitivities: parallel and bucket PV01 over SimpleQuotes,
//! and the sensitivities of dependent quotes to a SimpleQuote bump.
//!
//! Port of QuantLib code removed in 1.36 (`6f379f4e9`); not part of current
//! QuantLib. The source is `ql/experimental/risk/sensitivityanalysis.{hpp,cpp}`
//! as of `6f379f4e9^`, deprecated upstream as "out of scope" (`9f0c9b90c`).
//!
//! Each PV01 function shifts one or more [`SimpleQuote`]s, reprices the instruments
//! through their (lazy, observer-driven) [`Instrument::npv`], and returns the
//! finite-difference first and second derivatives. The quotes are always
//! restored to their original values before returning, on `Ok` and on `Err`
//! alike (`.cpp:110-114`). There is no drop guard: restoring calls
//! [`SimpleQuote::set_value`], which notifies observers, and that must not run
//! during a panic unwind. A panic (a bug, not a QuantLib exception) therefore
//! leaves the quotes bumped.
//!
//! The C++ `Handle<SimpleQuote>` arguments become [`Shared<SimpleQuote>`]: a
//! `Handle<dyn Quote>` cannot be set, and every caller already holds the shared
//! quote it wrapped into its curve handles. The C++ `Null<Real>` reference NPV
//! and second order become `None`.
//!
//! Divergences from QuantLib (D10: silent zeros become explicit errors):
//! - empty `instruments` returns `Err`; QuantLib returns `(0, 0)` (`.cpp:74,129`)
//!   or zero vectors (`.cpp:265`).
//! - an invalid quote returns `Err` in every form; QuantLib returns `(0, 0)` from
//!   the single-quote bucket form (`.cpp:134`) and silently skips it in the
//!   parallel form (`.cpp:81-86`).
//! - the matrix form with empty `instruments` returns `Err`; QuantLib returns
//!   zero matrices (`.cpp:333`).
//! - an invalid bump quote in the parameter forms returns `Err`; QuantLib fills
//!   the results with `Null` (`.cpp:181-187`).
//!
//! The parameter forms are fault tolerant as in QuantLib: an invalid parameter
//! yields `None` in its slot (`.cpp:198`), not an error. Their reference values
//! are input only, where QuantLib's single-quote form fills an in/out vector
//! (`.cpp:196-200`).
//!
//! Not ported: the matrix overload of `parallelAnalysis`, declared
//! (`.hpp:104-110`) but never defined upstream. A caller can flatten the quotes
//! into [`parallel_analysis`].
//!
//! Deferred and not ported: the C, Go and Python bindings (#1324).

use std::fmt;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::{Shared, SharedMut};
use crate::types::Real;
use crate::{fail, require};

/// The QuantLib default shift, one basis point (`.hpp:85`).
pub const DEFAULT_SHIFT: Real = 1.0e-4;

/// Ragged row-major results, one row per row of input quotes.
pub type Rows<T> = Vec<Vec<T>>;

/// Parameter derivatives, `None` where a parameter has no reference value.
pub type ParameterDerivatives = Vec<Option<Real>>;

/// Finite-differences scheme (`SensitivityAnalysis`, `.hpp:41`).
///
/// The `Default` is [`Centered`](SensitivityAnalysis::Centered), the C++
/// default argument (`.hpp:86`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SensitivityAnalysis {
    /// Forward difference: first order only.
    OneSide,
    /// Central difference: first and second order.
    #[default]
    Centered,
}

impl fmt::Display for SensitivityAnalysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensitivityAnalysis::OneSide => f.write_str("OneSide"),
            SensitivityAnalysis::Centered => f.write_str("Centered"),
        }
    }
}

/// Weighted sum of the instruments' NPVs (`aggregateNPV`, `.cpp:44-57`).
///
/// Empty `quantities`, or the single quantity `[1.0]`, mean unit weights;
/// otherwise there must be one quantity per instrument.
pub fn aggregate_npv(
    instruments: &[SharedMut<dyn Instrument>],
    quantities: &[Real],
) -> QlResult<Real> {
    if quantities.is_empty() || quantities == [1.0] {
        let mut npv = 0.0;
        for instrument in instruments {
            npv += instrument.borrow_mut().npv()?;
        }
        return Ok(npv);
    }
    require!(
        quantities.len() == instruments.len(),
        "dimension mismatch between instruments ({}) and quantities ({})",
        instruments.len(),
        quantities.len()
    );
    let mut npv = 0.0;
    for (instrument, quantity) in instruments.iter().zip(quantities) {
        npv += quantity * instrument.borrow_mut().npv()?;
    }
    Ok(npv)
}

/// Parallel-shift PV01 over a quote vector (`parallelAnalysis`, `.cpp:60-118`).
///
/// All quotes are bumped together by `shift`. Returns the first derivative and,
/// for [`Centered`](SensitivityAnalysis::Centered), the second derivative
/// (`None` for [`OneSide`](SensitivityAnalysis::OneSide)). A `None`
/// `reference_npv` is computed with [`aggregate_npv`].
pub fn parallel_analysis(
    quotes: &[Shared<SimpleQuote>],
    instruments: &[SharedMut<dyn Instrument>],
    quantities: &[Real],
    shift: Real,
    kind: SensitivityAnalysis,
    reference_npv: Option<Real>,
) -> QlResult<(Real, Option<Real>)> {
    require!(!quotes.is_empty(), "empty SimpleQuote vector");
    let (originals, reference_npv) =
        prepare(quotes, instruments, quantities, shift, reference_npv)?;
    let result = finite_differences(shift, kind, reference_npv, |bump| {
        for (quote, original) in quotes.iter().zip(&originals) {
            quote.set_value(original + bump);
        }
        aggregate_npv(instruments, quantities)
    });
    restore(quotes, &originals);
    result
}

/// Bucket PV01 for a single quote (`bucketAnalysis`, `.cpp:120-160`).
///
/// Same conventions as [`parallel_analysis`], bumping `quote` alone.
pub fn bucket_analysis(
    quote: &Shared<SimpleQuote>,
    instruments: &[SharedMut<dyn Instrument>],
    quantities: &[Real],
    shift: Real,
    kind: SensitivityAnalysis,
    reference_npv: Option<Real>,
) -> QlResult<(Real, Option<Real>)> {
    parallel_analysis(
        std::slice::from_ref(quote),
        instruments,
        quantities,
        shift,
        kind,
        reference_npv,
    )
}

/// Bucket PV01 for each quote of a vector, bumped one at a time
/// (`bucketAnalysis`, `.cpp:255-279`).
///
/// The reference NPV is computed once and shared by every bucket. Returns the
/// first derivatives and the second derivatives, index-aligned with `quotes`.
pub fn bucket_analysis_vector(
    quotes: &[Shared<SimpleQuote>],
    instruments: &[SharedMut<dyn Instrument>],
    quantities: &[Real],
    shift: Real,
    kind: SensitivityAnalysis,
) -> QlResult<(Vec<Real>, Vec<Option<Real>>)> {
    require!(!quotes.is_empty(), "empty SimpleQuote vector");
    let (_, reference_npv) = prepare(quotes, instruments, quantities, shift, None)?;
    let mut first = Vec::with_capacity(quotes.len());
    let mut second = Vec::with_capacity(quotes.len());
    for quote in quotes {
        let (d1, d2) = bucket_analysis(
            quote,
            instruments,
            quantities,
            shift,
            kind,
            Some(reference_npv),
        )?;
        first.push(d1);
        second.push(d2);
    }
    Ok((first, second))
}

/// Bucket PV01 for each quote of a ragged matrix, bumped one at a time
/// (`bucketAnalysis`, `.cpp:313-347`).
///
/// Same conventions as [`bucket_analysis_vector`]: one reference NPV shared by
/// every bucket, results shaped like `quotes`. Empty rows are allowed; an
/// empty outer slice is an error.
pub fn bucket_analysis_matrix(
    quotes: &[Vec<Shared<SimpleQuote>>],
    instruments: &[SharedMut<dyn Instrument>],
    quantities: &[Real],
    shift: Real,
    kind: SensitivityAnalysis,
) -> QlResult<(Rows<Real>, Rows<Option<Real>>)> {
    require!(!quotes.is_empty(), "empty SimpleQuote range");
    let (_, reference_npv) = prepare(&quotes.concat(), instruments, quantities, shift, None)?;
    let mut first = Vec::with_capacity(quotes.len());
    let mut second = Vec::with_capacity(quotes.len());
    for row in quotes {
        let mut first_row = Vec::with_capacity(row.len());
        let mut second_row = Vec::with_capacity(row.len());
        for quote in row {
            let (d1, d2) = bucket_analysis(
                quote,
                instruments,
                quantities,
                shift,
                kind,
                Some(reference_npv),
            )?;
            first_row.push(d1);
            second_row.push(d2);
        }
        first.push(first_row);
        second.push(second_row);
    }
    Ok((first, second))
}

/// Sensitivities of `parameters` to a bump of `quote` (`bucketAnalysis`,
/// `.cpp:167-253`).
///
/// Returns, index-aligned with `parameters`, the first derivatives and the
/// second derivatives (`None` throughout for
/// [`OneSide`](SensitivityAnalysis::OneSide)). A slot is `None` when its
/// reference value is `None`: an invalid parameter is skipped, not an error
/// (QuantLib's "fault tolerant", `.cpp:198`). A `None` `reference_values` is
/// read from the parameters before the bump; QuantLib's in/out vector is
/// input-only here.
pub fn parameter_sensitivity(
    quote: &Shared<SimpleQuote>,
    parameters: &[Handle<dyn Quote>],
    reference_values: Option<&[Option<Real>]>,
    shift: Real,
    kind: SensitivityAnalysis,
) -> QlResult<(ParameterDerivatives, ParameterDerivatives)> {
    require!(shift != 0.0, "zero shift not allowed");
    require!(!parameters.is_empty(), "empty parameters vector");
    let Ok(original) = quote.value() else {
        fail!("invalid SimpleQuote");
    };
    let reference_values = match reference_values {
        Some(values) => {
            require!(
                values.len() == parameters.len(),
                "referenceValues has size {}, instead of {}",
                values.len(),
                parameters.len()
            );
            values.to_vec()
        }
        None => parameter_values(parameters)?,
    };
    let result = parameter_differences(quote, original, parameters, &reference_values, shift, kind);
    quote.set_value(original);
    result
}

/// [`parameter_sensitivity`] for each quote of a vector, bumped one at a time
/// (`bucketAnalysis`, `.cpp:285-311`).
///
/// The reference values are read once and shared by every quote. Returns one
/// row of first and second derivatives per quote.
pub fn parameter_sensitivity_vector(
    quotes: &[Shared<SimpleQuote>],
    parameters: &[Handle<dyn Quote>],
    shift: Real,
    kind: SensitivityAnalysis,
) -> QlResult<(Vec<ParameterDerivatives>, Vec<ParameterDerivatives>)> {
    require!(!quotes.is_empty(), "empty SimpleQuote vector");
    require!(!parameters.is_empty(), "empty parameters vector");
    let reference_values = parameter_values(parameters)?;
    let mut first = Vec::with_capacity(quotes.len());
    let mut second = Vec::with_capacity(quotes.len());
    for quote in quotes {
        let (d1, d2) =
            parameter_sensitivity(quote, parameters, Some(&reference_values), shift, kind)?;
        first.push(d1);
        second.push(d2);
    }
    Ok((first, second))
}

fn parameter_values(parameters: &[Handle<dyn Quote>]) -> QlResult<Vec<Option<Real>>> {
    parameters
        .iter()
        .map(|parameter| {
            let parameter = parameter.current_link()?;
            Ok(if parameter.is_valid() {
                Some(parameter.value()?)
            } else {
                None
            })
        })
        .collect()
}

fn parameter_differences(
    quote: &SimpleQuote,
    original: Real,
    parameters: &[Handle<dyn Quote>],
    reference_values: &[Option<Real>],
    shift: Real,
    kind: SensitivityAnalysis,
) -> QlResult<(ParameterDerivatives, ParameterDerivatives)> {
    let values_at = |bump: Real| -> QlResult<Vec<Option<Real>>> {
        quote.set_value(original + bump);
        parameters
            .iter()
            .zip(reference_values)
            .map(|(parameter, reference)| {
                reference
                    .map(|_| parameter.current_link()?.value())
                    .transpose()
            })
            .collect()
    };
    let up = values_at(shift)?;
    match kind {
        SensitivityAnalysis::OneSide => {
            let first = up
                .iter()
                .zip(reference_values)
                .map(|(up, reference)| up.zip(*reference).map(|(up, r)| (up - r) / shift))
                .collect();
            Ok((first, vec![None; parameters.len()]))
        }
        SensitivityAnalysis::Centered => {
            let down = values_at(-shift)?;
            let mut first = Vec::with_capacity(parameters.len());
            let mut second = Vec::with_capacity(parameters.len());
            for ((up, down), reference) in up.iter().zip(&down).zip(reference_values) {
                let (d1, d2) = match (up, down, reference) {
                    (Some(up), Some(down), Some(reference)) => (
                        Some((up - down) / (2.0 * shift)),
                        Some((up - 2.0 * reference + down) / (shift * shift)),
                    ),
                    _ => (None, None),
                };
                first.push(d1);
                second.push(d2);
            }
            Ok((first, second))
        }
    }
}

fn prepare(
    quotes: &[Shared<SimpleQuote>],
    instruments: &[SharedMut<dyn Instrument>],
    quantities: &[Real],
    shift: Real,
    reference_npv: Option<Real>,
) -> QlResult<(Vec<Real>, Real)> {
    require!(shift != 0.0, "zero shift not allowed");
    require!(!instruments.is_empty(), "empty instrument vector");
    let mut originals = Vec::with_capacity(quotes.len());
    for (i, quote) in quotes.iter().enumerate() {
        let Ok(value) = quote.value() else {
            fail!("invalid SimpleQuote at position {i}");
        };
        originals.push(value);
    }
    let reference_npv = match reference_npv {
        Some(npv) => npv,
        None => aggregate_npv(instruments, quantities)?,
    };
    Ok((originals, reference_npv))
}

fn finite_differences(
    shift: Real,
    kind: SensitivityAnalysis,
    reference_npv: Real,
    mut npv_at: impl FnMut(Real) -> QlResult<Real>,
) -> QlResult<(Real, Option<Real>)> {
    let up = npv_at(shift)?;
    match kind {
        SensitivityAnalysis::OneSide => Ok(((up - reference_npv) / shift, None)),
        SensitivityAnalysis::Centered => {
            let down = npv_at(-shift)?;
            Ok((
                (up - down) / (2.0 * shift),
                Some((up - 2.0 * reference_npv + down) / (shift * shift)),
            ))
        }
    }
}

fn restore(quotes: &[Shared<SimpleQuote>], originals: &[Real]) {
    for (quote, original) in quotes.iter().zip(originals) {
        quote.set_value(*original);
    }
}
