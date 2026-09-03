//! External [`SimpleQuote`]s as additional bootstrap variables.
//!
//! Port of `ql/termstructures/globalbootstrapvars.{hpp,cpp}`: the one concrete
//! [`AdditionalBootstrapVariables`] implementation upstream ships. The quotes
//! it drives are read by rate helpers, so the global solve fits them jointly
//! with the curve nodes - the futures-convexity oracle
//! (`piecewiseyieldcurve.cpp:1486`) fits a Hull-White volatility this way.
//!
//! Each variable is optionally floored. With a lower bound the optimizer works
//! in log space (`exp(x) + lb` out, `ln(x - lb)` in); without one the transform
//! is the identity. A missing entry is C++'s `detail::get(v, i, Null<Real>())`
//! over a vector shorter than the quotes - here simply `Vec::get`.

use crate::errors::QlResult;
use crate::quotes::{Quote, SimpleQuote};
use crate::require;
use crate::shared::Shared;
use crate::termstructures::globalbootstrap::AdditionalBootstrapVariables;
use crate::types::{Real, Size};

/// External [`SimpleQuote`]s as additional bootstrap variables.
///
/// Mirrors QuantLib's `SimpleQuoteVariables`. `initial_guesses` and
/// `lower_bounds` may each be shorter than `quotes`, defaulting per variable to
/// a guess of 0.0 and to no lower bound.
pub struct SimpleQuoteVariables {
    quotes: Vec<Shared<SimpleQuote>>,
    initial_guesses: Vec<Real>,
    lower_bounds: Vec<Real>,
}

impl SimpleQuoteVariables {
    /// The variables over `quotes` (`globalbootstrapvars.cpp:10-17`).
    ///
    /// # Errors
    ///
    /// Fails if more initial guesses or more lower bounds than quotes are
    /// given.
    pub fn new(
        quotes: Vec<Shared<SimpleQuote>>,
        initial_guesses: Vec<Real>,
        lower_bounds: Vec<Real>,
    ) -> QlResult<SimpleQuoteVariables> {
        require!(
            initial_guesses.len() <= quotes.len(),
            "too many initialGuesses ({}) for {} quotes",
            initial_guesses.len(),
            quotes.len()
        );
        require!(
            lower_bounds.len() <= quotes.len(),
            "too many lowerBounds ({}) for {} quotes",
            lower_bounds.len(),
            quotes.len()
        );
        Ok(SimpleQuoteVariables {
            quotes,
            initial_guesses,
            lower_bounds,
        })
    }

    /// Optimizer space to quote space (`globalbootstrapvars.cpp:39-42`).
    fn transform_direct(&self, x: Real, i: Size) -> Real {
        match self.lower_bounds.get(i) {
            Some(lower_bound) => x.exp() + lower_bound,
            None => x,
        }
    }

    /// Quote space to optimizer space (`globalbootstrapvars.cpp:44-47`).
    fn transform_inverse(&self, x: Real, i: Size) -> Real {
        match self.lower_bounds.get(i) {
            Some(lower_bound) => (x - lower_bound).ln(),
            None => x,
        }
    }
}

impl AdditionalBootstrapVariables for SimpleQuoteVariables {
    /// The initial guesses (`globalbootstrapvars.cpp:19-32`). On a warm restart
    /// each quote's CURRENT value is the guess; otherwise the configured
    /// initial guess is both returned and WRITTEN INTO the quote, so the
    /// helpers reading it see a defined starting point before the first cost
    /// evaluation.
    fn initialize(&self, valid_data: bool) -> QlResult<Vec<Real>> {
        let mut guesses = Vec::with_capacity(self.quotes.len());
        for (i, quote) in self.quotes.iter().enumerate() {
            let guess = if valid_data {
                quote.value()?
            } else {
                let initial = self.initial_guesses.get(i).copied().unwrap_or(0.0);
                quote.set_value(initial);
                initial
            };
            guesses.push(self.transform_inverse(guess, i));
        }
        Ok(guesses)
    }

    /// Writes a trial point back into the quotes
    /// (`globalbootstrapvars.cpp:34-37`).
    ///
    /// # Errors
    ///
    /// Fails on a trial point longer than the variable count - C++ indexes
    /// `quotes_[i]` over `x` unguarded.
    fn update(&self, x: &[Real]) -> QlResult<()> {
        require!(
            x.len() <= self.quotes.len(),
            "trial point of size {} for {} quotes",
            x.len(),
            self.quotes.len()
        );
        for (i, (quote, value)) in self.quotes.iter().zip(x).enumerate() {
            quote.set_value(self.transform_direct(*value, i));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;

    fn variables(
        initial_guesses: Vec<Real>,
        lower_bounds: Vec<Real>,
    ) -> (Shared<SimpleQuote>, SimpleQuoteVariables) {
        let quote = shared(SimpleQuote::new(None));
        let variables =
            SimpleQuoteVariables::new(vec![Shared::clone(&quote)], initial_guesses, lower_bounds)
                .expect("one guess and one bound for one quote");
        (quote, variables)
    }

    /// The floored branch: `initialize` writes the initial guess into the quote
    /// and returns `ln(guess - lb)`, and `update` inverts it.
    ///
    /// The floor 0.25 is deliberately non-zero, so a port that dropped the
    /// `- lb` / `+ lb` term fails on both halves rather than only on one.
    #[test]
    fn a_lower_bound_puts_the_variable_in_log_space() {
        let (quote, variables) = variables(vec![1.25], vec![0.25]);

        let guesses = variables.initialize(false).unwrap();
        assert_eq!(quote.value().unwrap(), 1.25);
        assert!((guesses[0] - Real::ln(1.0)).abs() < 1.0e-15);

        variables.update(&[Real::ln(0.75)]).unwrap();
        assert!((quote.value().unwrap() - 1.0).abs() < 1.0e-15);
    }

    /// The identity branch (no lower bound for this variable): the guess is
    /// returned unchanged and `update` writes the trial value straight through.
    ///
    /// This branch is an honest gap in the oracle - `testGlobalBootstrapVariables`
    /// passes a lower bound - so it is pinned here directly.
    #[test]
    fn a_variable_without_a_lower_bound_is_untransformed() {
        let (quote, variables) = variables(vec![-0.5], Vec::new());

        let guesses = variables.initialize(false).unwrap();
        assert_eq!(quote.value().unwrap(), -0.5);
        assert_eq!(guesses[0], -0.5);

        variables.update(&[-2.0]).unwrap();
        assert_eq!(quote.value().unwrap(), -2.0);
    }

    /// The defaults for a short configuration vector: no initial guess means
    /// 0.0, no lower bound means the identity transform.
    #[test]
    fn a_missing_configuration_entry_falls_back_to_its_default() {
        let first = shared(SimpleQuote::new(None));
        let second = shared(SimpleQuote::new(None));
        let variables = SimpleQuoteVariables::new(
            vec![Shared::clone(&first), Shared::clone(&second)],
            vec![3.0],
            Vec::new(),
        )
        .unwrap();

        let guesses = variables.initialize(false).unwrap();

        assert_eq!(first.value().unwrap(), 3.0);
        assert_eq!(second.value().unwrap(), 0.0);
        assert_eq!(guesses, vec![3.0, 0.0]);
    }

    /// The warm-restart branch reads each quote's CURRENT value instead of the
    /// configured guess, and leaves the quote alone.
    #[test]
    fn valid_data_seeds_the_guess_from_the_current_quote_value() {
        let (quote, variables) = variables(vec![1.0], vec![0.0]);
        quote.set_value(4.0);

        let guesses = variables.initialize(true).unwrap();

        assert_eq!(quote.value().unwrap(), 4.0);
        assert!((guesses[0] - Real::ln(4.0)).abs() < 1.0e-15);
    }

    /// The two size preconditions (`globalbootstrapvars.cpp:15-16`), also
    /// unexercised by the oracle.
    #[test]
    fn more_configuration_entries_than_quotes_are_rejected() {
        let quote = shared(SimpleQuote::new(None));
        assert!(
            SimpleQuoteVariables::new(vec![Shared::clone(&quote)], vec![1.0, 2.0], Vec::new())
                .is_err()
        );
        assert!(SimpleQuoteVariables::new(vec![quote], Vec::new(), vec![0.0, 0.0]).is_err());
    }
}
