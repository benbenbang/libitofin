# European percentage-strike options

This additive factory returns the existing option facade and reuses the existing
finite-difference Black-Scholes engine. No core payoff or pricing algorithm is
changed, and existing constructors remain unchanged.

| Language | Factory | Pricing |
| --- | --- | --- |
| Python | `VanillaOption.percentage_strike(option_type, moneyness, expiry, settings)` | `set_fd_engine(engine)` or `price_fd(engine)` |
| Go | `session.NewPercentageStrikeOption(kind, moneyness, expiry, settings)` | `SetFdEngine(engine)` or `PriceFd(engine)` |
| C | `itofin_option_percentage_strike_new` | Existing `itofin_option_set_engine` with kind 2 |

## Ratio and engine contract

- Moneyness `m` is a dimensionless ratio. For example `0.8` means a call pays
  `0.2 * terminal_asset_price`, regardless of where that price finishes.
- Calls pay `S * max(1-m, 0)` and puts pay `S * max(m-1, 0)`. Both pay zero
  when `m=1`. This is not an ordinary absolute-strike option, a percentage
  scaled by 100, a forward-start option or a reset-strike product.
- Ratios must be finite. Zero and signed ratios are accepted, matching the
  underlying payoff's domain. Constructor acceptance is not a guarantee that
  every process, volatility surface or grid can price that ratio.
- Attach the existing European FD engine. The analytic Black engine rejects this
  payoff rather than silently treating it as an ordinary vanilla option. No
  additional MC, analytic or American payoff compatibility is introduced.
- The native FD engine passes `m` unchanged as the strike to volatility lookup.
  It does not pass `spot*m`. A strike-smile surface defined on currency strikes
  such as 80 to 120 can therefore reject `m=0.8`; that domain error is preserved.
  Constant-volatility processes are the validated path.

## Pricing and numerical limits

The existing FD engine provides NPV, delta, gamma and theta where available.
Unexpired FD vega, rho, dividend rho and error estimate remain unavailable and
raise errors. Expired instruments retain their ordinary zero-result behavior.
No global finite-output guard or deterministic fallback was added to the existing
option accessors. Extreme inputs and the existing log-FD domain limits remain.

For a constant dividend yield `q`, time `T` and factor `f=max(1-m,0)` for calls
or `max(m-1,0)` for puts, the continuum value is `S*f*exp(-q*T)`. Delta is
`f*exp(-q*T)`, gamma is zero and theta is `q*S*f*exp(-q*T)`. FD discretization
still introduces error even for this linear payoff. Independent observations
at grids 100 and 400 have maximum absolute price errors approximately 0.002762
and 0.000171 over the tracked two-market matrix. Same-grid agreement is not
an exact continuum-price guarantee; defaults remain unchanged.

The independent 48-row JSONL fixture covers calls and puts, signed/zero/unit ratios,
two markets, two maturities and grids 100/400 with Douglas and zero damping.
It was produced by a separate QuantLib 1.43 wheel, not libitofin. The inspected
source reference is revision `9863b578af0caa4cecabf697196533e84a8308b6`
(1.43-dev), not a claim about that wheel's build source. The generator and
provenance are in `crates/libitofin-ffi/tests/fixtures/percentage-strike-oracle/`.
Byte-identical JSONL files are retained in the Python fixtures and Go testdata.

## Lifetime and errors

Settings are retained by the option. The attached engine retains the process,
curves and quote graph. Closing input wrappers or Python garbage collection does
not remove these dependencies. Observable quote and evaluation-date updates
invalidate the ordinary lazy cache; existing result snapshots remain unchanged.

C preserves typed context handles, creating-thread confinement and unchanged
caller outputs on failure under its pointer contract. Go rejects nil, foreign
and closed dependencies, serializes calls on its session worker and requires
explicit Close. Python retains its unsendable native ownership model.

## Examples

- Python: `example/python/percentage_strike.py`.
- Go: `sdk/go/examples/percentage_strike/main.go`.

This slice is not a complete payoff-object family or CPI pricing implementation.
