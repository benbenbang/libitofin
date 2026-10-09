# Implied Black volatility term structure

`ImpliedVolTermStructure` is an additive Rust-core wrapper over a live
`Handle<dyn BlackVolTermStructure>`. It rebases Black variance to a fixed future
reference date. No C, Go or Python facade is added.

## Contract

- Original forward variance: `W(shift + t, strike) - W(shift, strike)`.
- `shift` uses the **current original day counter and reference date** on every
  query. The implied reference date stays fixed after evaluation-date changes.
- Volatility is `sqrt(variance(t) / t)`. At exactly `t = 0`, both variance and
  denominator use `1e-5`; positive times smaller than `1e-5` are not clamped.
- Maximum date, strike bounds and day counter follow the live original link.
  Maximum time is measured from the implied reference date.
- The wrapper has no calendar or settlement days. Its business-day convention
  is `Following`, matching the upstream fixed-date constructor.
- The wrapper's extrapolation starts disabled and is independent of the
  original curve. Public date/time/strike checks run before the original's
  internally extrapolating forward-variance query.
- Existing parent checks reject nonfinite or negative times, nonfinite strikes,
  invalid variance and nonfinite volatility. Decreasing forward variance is an
  error, not a NaN result. There is no monotonicity repair.

Use time-dependent original curves for financial interpretation. QuantLib
warns that rebasing an asset-dependent smile is not financially meaningful.
Strike-dependent tests exercise numerical forwarding only.

## Ownership and updates

The original handle owns its current curve and forwards quote changes, relinks
and moving-reference notifications. Registration uses the retained base updater
and existing weak observer graph, without caches or additional listeners.
Dropping the wrapper releases its curve ownership and updater.

Construction permits an empty handle for later linking. Numerical queries
return errors when the current link, day counter or valid reference date is
missing. Empty-link inspectors return no day counter, the null maximum date
and unbounded strike sentinels, following existing parent conventions.

## Validation and limits

- Inspected source: QuantLib `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled numerical reference: PyPI `QuantLib==1.43`, a distinct revision.
  Its wheel lacks the implied wrapper. Fixtures use compiled original forward
  variance with the inspected rebasing/zero-time-adapter laws, not a claimed
  compiled wrapper execution.
- Reproducible generator and provenance:
  `crates/libitofin/tests/fixtures/medium_implied_vol/`.
- Tests include nonflat curves, a different-day-counter relink, live settings
  and quotes, bounded strikes, extrapolation, zero/tiny time and lifecycle.

Very short forward intervals can lose relative accuracy when subtracting two
near-equal total variances, as in the original forward-variance contract. This
wrapper does not substitute a numerical derivative or stabilize the original
curve's variance subtraction.

No new process, pricing engine, mutable calibration API or release is included.
