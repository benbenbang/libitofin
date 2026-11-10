use crate::boundary::*;
use crate::market_api::quote;
use crate::parameter_sensitivity_api::*;
use crate::quotes_api::*;
use crate::quotes_api_tests::{error, simple, value};
use libitofin::errors::QlResult;
use libitofin::handle::AsObservable;
use libitofin::patterns::observable::Observable;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::risk::{self, SensitivityAnalysis};
use libitofin::shared::{Shared, shared};
use libitofin::{ensure, types::Real};
use std::ptr::{null, null_mut};

const X: f64 = 0.75;
const H: f64 = 0.0625;
const ONE_SIDE: i32 = 0;
const CENTERED: i32 = 1;

type Slot = (Option<f64>, Option<f64>);

fn message(e: &ItofinError) -> String {
    let text = unsafe { std::ffi::CStr::from_ptr(e.message.as_ptr()) }.to_string_lossy();
    match text.split_once(": ") {
        Some((location, message)) if location.contains(".rs:") => message.to_owned(),
        _ => text.into_owned(),
    }
}

fn dependent(c: &mut Context, q: u64, a: Option<f64>) -> u64 {
    let mut out = 0;
    let code = match a {
        Some(a) => unsafe { itofin_derived_quote_new(c, q, a, 0.0, &mut out, null_mut()) },
        None => unsafe {
            itofin_composite_quote_new(c, q, q, ITOFIN_COMPOSITE_MULTIPLY, &mut out, null_mut())
        },
    };
    assert_eq!(code, 0);
    out
}

fn fixture(c: &mut Context) -> (u64, [u64; 2]) {
    let q = simple(c, X);
    (q, [dependent(c, q, None), dependent(c, q, Some(3.0))])
}

fn vector(
    c: &mut Context,
    quotes: &[u64],
    parameters: &[u64],
    shift: f64,
) -> Result<Vec<Slot>, (i32, String)> {
    let mut out =
        vec![ItofinParameterSensitivity::default(); (quotes.len() * parameters.len()).max(1)];
    let mut e = error();
    let (q, p) = (quotes.as_ptr(), parameters.as_ptr());
    let (nq, np) = (quotes.len(), parameters.len());
    let code = unsafe {
        itofin_parameter_sensitivity_vector(
            c,
            q,
            nq,
            p,
            np,
            shift,
            CENTERED,
            out.as_mut_ptr(),
            &mut e,
        )
    };
    if code != 0 {
        return Err((code, message(&e)));
    }
    Ok(out[..nq * np].iter().map(slot).collect())
}

fn single(
    c: &mut Context,
    q: u64,
    parameters: &[u64],
    references: Option<&[Option<f64>]>,
    shift: f64,
    kind: i32,
) -> Result<Vec<Slot>, (i32, String)> {
    let (values, has): (Vec<f64>, Vec<u8>) = references
        .unwrap_or_default()
        .iter()
        .map(|r| (r.unwrap_or(0.0), u8::from(r.is_some())))
        .unzip();
    let values_ptr = references.map_or(null(), |_| values.as_ptr());
    let mut out = vec![ItofinParameterSensitivity::default(); parameters.len().max(1)];
    let mut e = error();
    let code = unsafe {
        itofin_parameter_sensitivity(
            c,
            q,
            parameters.as_ptr(),
            parameters.len(),
            values_ptr,
            has.as_ptr(),
            values.len(),
            shift,
            kind,
            out.as_mut_ptr(),
            &mut e,
        )
    };
    if code != 0 {
        return Err((code, message(&e)));
    }
    Ok(out[..parameters.len()].iter().map(slot).collect())
}

fn slot(s: &ItofinParameterSensitivity) -> Slot {
    (
        (s.has_first != 0).then_some(s.first),
        (s.has_second != 0).then_some(s.second),
    )
}

fn core(c: &Context, q: u64, parameters: &[u64], kind: SensitivityAnalysis) -> Vec<Slot> {
    let bumped = c.get::<Shared<SimpleQuote>>(q).unwrap();
    let handles: Vec<_> = parameters.iter().map(|&id| quote(c, id).unwrap()).collect();
    let (first, second) = risk::parameter_sensitivity(&bumped, &handles, None, H, kind).unwrap();
    first.into_iter().zip(second).collect()
}

#[test]
fn the_single_form_matches_the_core_and_skips_a_missing_reference() {
    let mut c = Context::new();
    let (q, parameters) = fixture(&mut c);
    let centered = single(&mut c, q, &parameters, None, H, CENTERED).unwrap();
    assert_eq!(centered, [(Some(1.5), Some(2.0)), (Some(3.0), Some(0.0))]);
    assert_eq!(
        centered,
        core(&c, q, &parameters, SensitivityAnalysis::Centered)
    );
    let one_side = single(&mut c, q, &parameters, None, H, ONE_SIDE).unwrap();
    assert_eq!(one_side, [(Some(1.5625), None), (Some(3.0), None)]);
    let references = [Some(X * X), None];
    let skipped = single(&mut c, q, &parameters, Some(&references), H, CENTERED).unwrap();
    assert_eq!(skipped, [(Some(1.5), Some(2.0)), (None, None)]);
    assert_eq!(
        one_side,
        core(&c, q, &parameters, SensitivityAnalysis::OneSide)
    );
    assert_eq!(value(&mut c, q), X);
}

fn composite(c: &mut Context, q1: u64, q2: u64, op: i32) -> u64 {
    let mut out = 0;
    assert_eq!(
        unsafe { itofin_composite_quote_new(c, q1, q2, op, &mut out, null_mut()) },
        0
    );
    out
}

/// Two quotes x = 0.75, y = 0.5 against three parameters x * x, x - y and
/// x * y, centered with h = 0.0625. The differences are exact: d/dx gives
/// (2x, 2), (1, 0), (y, 0) and d/dy gives (0, 0), (-1, 0), (x, 0). The 2 x 3
/// buffer is asymmetric, so a column-major write or swapped counts fail.
#[test]
fn the_vector_form_is_row_major_and_matches_the_core() {
    let mut c = Context::new();
    let (q1, q2) = (simple(&mut c, X), simple(&mut c, 0.5));
    let quotes = [q1, q2];
    let parameters = [
        composite(&mut c, q1, q1, ITOFIN_COMPOSITE_MULTIPLY),
        composite(&mut c, q1, q2, ITOFIN_COMPOSITE_SUBTRACT),
        composite(&mut c, q1, q2, ITOFIN_COMPOSITE_MULTIPLY),
    ];
    let out = vector(&mut c, &quotes, &parameters, H).unwrap();
    let cells = [
        (1.5, 2.0),
        (1.0, 0.0),
        (0.5, 0.0),
        (0.0, 0.0),
        (-1.0, 0.0),
        (X, 0.0),
    ];
    let want: Vec<Slot> = cells.iter().map(|&(f, s)| (Some(f), Some(s))).collect();
    assert_eq!(out, want);
    let rows: Vec<Slot> = quotes
        .iter()
        .flat_map(|&q| core(&c, q, &parameters, SensitivityAnalysis::Centered))
        .collect();
    assert_eq!(out, rows);
    assert_eq!((value(&mut c, q1), value(&mut c, q2)), (X, 0.5));
}

#[test]
fn non_finite_shifts_and_reference_values_are_invalid_arguments() {
    let mut c = Context::new();
    let (q, parameters) = fixture(&mut c);
    let invalid = (INVALID_ARGUMENT, "non-finite rate or amount".to_owned());
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let references = [Some(bad), None];
        for (references, shift) in [(None, bad), (Some(&references[..]), H)] {
            let got = single(&mut c, q, &parameters, references, shift, CENTERED);
            assert_eq!(got.unwrap_err(), invalid);
        }
        assert_eq!(vector(&mut c, &[q], &parameters, bad).unwrap_err(), invalid);
    }
    assert_eq!(value(&mut c, q), X);
}

/// A supplied reference of 0.0 enters the formulas: for x * x at x = 0.75,
/// h = 0.0625, the centered second is ((x + h)^2 + (x - h)^2) / h^2 = 290 and
/// the one-sided first is (x + h)^2 / h = 10.5625.
#[test]
fn a_zero_reference_value_is_a_value() {
    let mut c = Context::new();
    let (q, parameters) = fixture(&mut c);
    let references = [Some(0.0), None];
    let centered = single(&mut c, q, &parameters, Some(&references), H, CENTERED).unwrap();
    assert_eq!(centered, [(Some(1.5), Some(290.0)), (None, None)]);
    let one_side = single(&mut c, q, &parameters, Some(&references), H, ONE_SIDE).unwrap();
    assert_eq!(one_side, [(Some(10.5625), None), (None, None)]);
    assert_eq!(value(&mut c, q), X);
}

#[test]
fn invalid_inputs_are_errors_that_leave_the_quote() {
    let mut c = Context::new();
    let (q, parameters) = fixture(&mut c);
    let mut run = |parameters: &[u64], references: Option<&[Option<f64>]>, shift, kind| {
        single(&mut c, q, parameters, references, shift, kind).unwrap_err()
    };
    assert_eq!(
        run(&parameters, None, 0.0, CENTERED).1,
        "zero shift not allowed"
    );
    assert_eq!(run(&[], None, H, CENTERED).1, "empty parameters vector");
    assert_eq!(
        run(&parameters, Some(&[None]), H, CENTERED).1,
        "referenceValues has size 1, instead of 2"
    );
    assert_eq!(
        run(&parameters, None, H, 2),
        (INVALID_ARGUMENT, "invalid sensitivity analysis kind".into())
    );
    assert_eq!(
        single(&mut c, parameters[0], &parameters, None, H, CENTERED)
            .unwrap_err()
            .0,
        INVALID_HANDLE
    );
    assert_eq!(
        vector(&mut c, &[], &parameters, H).unwrap_err().1,
        "empty SimpleQuote vector"
    );
    assert_eq!(value(&mut c, q), X);
}

struct BumpSensitiveQuote {
    source: Shared<SimpleQuote>,
    observable: Observable,
}

impl AsObservable for BumpSensitiveQuote {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Quote for BumpSensitiveQuote {
    fn value(&self) -> QlResult<Real> {
        let value = self.source.value()?;
        ensure!(value == X, "bumped away from {X}");
        Ok(value)
    }

    fn is_valid(&self) -> bool {
        true
    }
}

#[test]
fn a_parameter_failing_after_the_bump_is_an_error_and_restores_the_quote() {
    let mut c = Context::new();
    let (q, [square, _]) = fixture(&mut c);
    let failing = BumpSensitiveQuote {
        source: c.get::<Shared<SimpleQuote>>(q).unwrap(),
        observable: Observable::new(),
    };
    let failing = c.insert(shared(failing) as Shared<dyn Quote>).unwrap();
    for kind in [ONE_SIDE, CENTERED] {
        let (_, message) = single(&mut c, q, &[square, failing], None, H, kind).unwrap_err();
        assert_eq!(message, "bumped away from 0.75");
        assert_eq!(value(&mut c, q), X);
    }
}
