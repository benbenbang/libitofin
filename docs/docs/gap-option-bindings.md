# European gap options

The additive gap factory returns the existing `VanillaOption` facade in C/Go and
Python. Existing vanilla and cash-or-nothing constructors are unchanged. This is
not a standalone payoff-object API or complete payoff/engine compatibility.

| Facade | Construction | Supported pricing path |
| --- | --- | --- |
| C | `itofin_option_gap_new` | Existing FD engine attached by `itofin_option_set_engine`, kind 2 |
| Go | `Session.NewGapOption` | `SetFdEngine` or `PriceFd` |
| Python | `VanillaOption.gap` | `set_fd_engine` or `price_fd` |

## Signed payoff and validation

Let `K` be the trigger strike and `L` the payment strike (`second_strike` in Python,
`secondStrike` in Go). A call pays `S - L` if `S >= K`, otherwise zero. A put pays
`L - S` if `S <= K`, otherwise zero. Equality activates the payoff. Negative
payments and negative option values are **not clamped** to zero.

The factory requires finite nonnegative `K`, finite signed `L`, valid expiry and
explicit settings. Zero trigger construction is accepted; that does not guarantee
that the existing log-space FD mesher can price it. The FD process retains its
existing positive-spot/grid/domain requirements. Invalid nonfinite parameters are
rejected before an option is created. C output is written only on success.

Use the existing `FdBlackScholesVanillaEngine`, supported Douglas or implicit Euler
scheme, and explicitly selected grids/damping when examining convergence. FD
provides NPV, delta, gamma and theta; other unavailable result fields return their
existing errors. Without an engine valuation fails. The analytic European Black
path rejects `GapPayoff`; the factory does not silently select a vanilla payoff or
expand any analytic/Heston/Monte Carlo engine's supported payoff types.

## Inherited FD limits

The existing terminal-cell averaging heuristic derives Simpson accuracy from the
sum of endpoint payoffs. Negative endpoint sums and cancelling sums at/below
machine epsilon fall back to the point payoff. This native behavior is preserved,
not repaired by the binding. A discontinuous signed payoff can therefore show
nonmonotone grid errors. Use multiple grids and independently qualified fixtures;
a single finite price is not an accuracy or full QuantLib-parity claim. In the
independent QuantLib 1.43 wheel grid ladder, case `gap-put-K100-H-20` uses
spot/trigger 100, payment strike -20, r=0.05, q=0.02, volatility=0.2, 360 days
under Actual360 and Douglas with two damping steps. Its 800-by-800 grid retains
about 0.07371 absolute error against the signed closed-form expectation. Same-grid native matching and finiteness are separate from
this per-route analytic bias; neither establishes high-precision convergence.

The native payoff trigger is inclusive. With the default reference-date-event
policy the instrument returns zero at expiry; explicitly including reference-date
events changes whether that date is expired. Zero volatility, near-zero time and zero trigger
must be interpreted through the existing FD domain/error behavior, not assumed to
follow an unavailable analytic engine. No core pricing formula was changed.

Supported ordinary FD cases require finite numerical results. Deliberate
zero-volatility, zero-trigger and unsupported-engine probes are qualified
separately as inherited diagnostics, not accepted by a passing NaN comparison.
Generic option raw-query semantics are unchanged; no global finite-result fix or
payoff-name dispatch is introduced by this factory.

## Ownership and live inputs

The option retains settings, and its attached engine retains the process/curve
and volatility graph. Supported quote/curve updates invalidate cached prices.
Python dependencies survive wrapper deletion. Go objects are session-owned;
close wrappers explicitly, and never mix sessions. C contexts and handles remain
thread-confined and typed; release source handles only after dependents retain
them. Missing/closed/mixed handles and unavailable Greeks report existing errors.

## Examples

Python:

```python
--8<-- "example/python/gap_option.py"
```

Go:

```go
--8<-- "sdk/go/examples/gap_option/main.go"
```
