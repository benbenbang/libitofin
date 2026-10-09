# Quanto-adjusted dividend curve

`QuantoTermStructure` is an additive Rust-core yield curve. It combines five
live handles into a continuously compounded dividend zero yield:

\[
q_Q(t)=q(t)+r_d(t)-r_f(t)+\rho\,\sigma_S(t,K)\,\sigma_{FX}(t,L).
\]

| Input | Meaning |
| --- | --- |
| Dividend yield curve | Underlying asset dividend zero yield `q` |
| Domestic / foreign yield curves | Interest-rate differential `r_d - r_f` |
| Asset Black volatility curve | Volatility at fixed underlying strike `K` |
| FX Black volatility curve | Volatility at fixed FX ATM level `L` |
| Correlation | Fixed asset/FX correlation `rho`, in `[-1, 1]` |

The discount factor is `exp(-q_Q(t) * t)`. This is a building block, not a
complete quanto option engine. The feature does not activate deferred finite-difference quanto paths, change existing engines, or add language bindings.

## Construction and live inputs

Import `QuantoTermStructure` from `libitofin::termstructures::yields`.
`new(dividend, domestic, foreign, asset_vol, strike, fx_vol, fx_atm, correlation)`
returns `QlResult<Self>`. Existing `YieldTermStructure` methods expose discounts,
zero rates and forward rates; `ZeroYieldStructure::zero_yield_impl` exposes the
continuous zero-yield formula directly.

- All five handles are observed through the existing weak base relay.
- Quote moves, relinks and moving-curve evaluation-date notifications propagate.
- No inputs or calculated yields are cached.
- Dividend metadata is live: reference date, day counter, calendar and settlement
  days change when the dividend handle is relinked.
- Maximum date is the minimum of all five inputs' **calendar maximum dates**.
  Maximum time uses that date and the dividend curve's reference/day counter.

## Query conventions preserved from QuantLib

All inputs receive the **same numerical time**, with internal extrapolation
explicitly enabled for both time and volatility strike. Reference dates and day
counters are not silently aligned or rebased. Differing clocks therefore retain
QuantLib's numerical result, but a meaningful financial application must supply
compatible conventions. The original source warns about common day counts.

Public range checks use the wrapper's own maximum date/time. Its extrapolation
flag starts disabled and does not inherit or synchronize input flags. Explicit
query extrapolation or `enable_extrapolation()` permits queries past its maximum;
it does not make negative/nonfinite times valid.

- `discount(0, ...)` returns one without evaluating the five inputs, after the
  public range check. With extrapolation allowed this also works on empty links.
- `zero_rate(0, ...)` follows the existing yield-curve convention: it computes a
  discount at `1e-4` years, not the formula directly at zero.
- Direct `zero_yield_impl(0)` uses input zero rates at their `1e-4` convention,
  but requests Black volatilities at zero, matching native source behavior.
- At extremely short positive times, zero rates recovered from a rounded
  discount can lose precision through `log(discount) / time`. Native fixtures
  test `1e-8` discounts, not a promised exact short-time analytic yield.

## Checked Rust boundaries

The original scalar constructor is unchecked. This additive constructor rejects
nonfinite strike/FX levels and nonfinite/out-of-range correlation. Finite zero
or negative levels remain allowed because some volatility domains support them;
the wrapper does not impose a new positive-strike domain.

Numerical queries reject missing links, nonfinite component zero rates,
nonfinite/negative component Black volatilities, a nonfinite combined zero yield,
and a nonpositive/nonfinite final discount. Component errors propagate even
when a zero correlation would make a volatility irrelevant algebraically.
Source-order arithmetic is preserved: intermediate overflow can reject an input
whose result could be finite under symbolic rearrangement. No arbitrary-precision
or catastrophic-cancellation guarantee is made.

Construction permits empty handles for later relinking. Empty dividend metadata
returns `None`/`Err`; any missing input gives the null maximum-date sentinel.
These are safe Rust fallbacks where native inspectors dereference empty handles.

## Independent reference

Fixtures use the **compiled `QuantLib==1.43` QuantoTermStructure directly**:
24 discounts and zero rates over nonflat curves, mixed reference dates/day counts,
correlation signs, out-of-domain asset strikes and extrapolated times. That wheel
is distinct from inspected source revision
`9863b578af0caa4cecabf697196533e84a8308b6`.
The generator and provenance are in
`crates/libitofin/tests/fixtures/medium2_quanto_curve/`.
