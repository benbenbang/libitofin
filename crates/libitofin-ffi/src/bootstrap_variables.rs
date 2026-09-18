//! Retained external quote variables for global bootstrap.
use crate::boundary::*;
use libitofin::quotes::SimpleQuote;
use libitofin::shared::Shared;
use libitofin::termstructures::globalbootstrapvars::SimpleQuoteVariables;

#[derive(Clone)]
pub(crate) struct Variables {
    pub quotes: Vec<Shared<SimpleQuote>>,
    guesses: Vec<f64>,
    bounds: Vec<f64>,
}
impl Variables {
    pub fn build(&self) -> BindingResult<SimpleQuoteVariables> {
        Ok(SimpleQuoteVariables::new(
            self.quotes.clone(),
            self.guesses.clone(),
            self.bounds.clone(),
        )?)
    }
}

/// Create retained quote variables. Guesses and bounds may be shorter than quotes.
/// Missing guesses are zero; supplied bounds must be finite and below the guess.
/// # Safety
/// Follow the crate-level context and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_simple_quote_variables_new(
    ctx: *mut Context,
    quotes: *const u64,
    count: usize,
    guesses: *const f64,
    guess_count: usize,
    bounds: *const f64,
    bound_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = Variables {
                quotes: input_slice(quotes, count)?
                    .iter()
                    .map(|id| c.get(*id))
                    .collect::<BindingResult<_>>()?,
                guesses: input_slice(guesses, guess_count)?.to_vec(),
                bounds: input_slice(bounds, bound_count)?.to_vec(),
            };
            value.build()?;
            for i in 0..value.quotes.len() {
                let guess = value.guesses.get(i).copied().unwrap_or(0.0);
                if !guess.is_finite()
                    || value
                        .bounds
                        .get(i)
                        .is_some_and(|bound| !bound.is_finite() || guess <= *bound)
                {
                    return Err(BindingError::invalid(
                        "initial guesses must be finite and strictly above finite lower bounds",
                    ));
                }
            }
            output(out, c.insert(value)?)
        })
    }
}
