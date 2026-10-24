# European asset-or-nothing options

This additive factory returns the existing option facade. It exposes the existing
asset-or-nothing payoff through the existing finite-difference Black-Scholes
engine, not a new analytic formula or a complete payoff-object family. Existing
plain-vanilla and cash-digital constructors are unchanged.

| Language | Factory | FD pricing |
| --- | --- | --- |
| Python | `VanillaOption.asset_or_nothing(option_type, strike, expiry, settings)` | `option.set_fd_engine(engine)` or `option.price_fd(engine)` |
| Go | `session.NewAssetOrNothingOption(kind, strike, expiry, settings)` | `option.SetFdEngine(engine)` or `option.PriceFd(engine)` |
| C | `itofin_option_asset_or_nothing_new` | Existing `itofin_option_set_engine`, kind2, with an FD engine handle |

## Bounded behavior

- A European call pays the terminal asset price strictly above strike; a put
  pays that price strictly below strike. The point payoff is zero at equality.
  There is no cash amount, notional multiplier or physical-settlement API.
- Strike must be finite and nonnegative. The existing stock FD process requires
  positive stock levels. Constructibility does not promise a finite result for
  every grid, volatility or extreme market input.
- Create `FdBlackScholesVanillaEngine` through the existing facade. The numerical
  reference matrix uses Douglas, zero damping and equal time/space grids.
  Defaults remain 100 by 100 with zero damping. A discontinuity needs a convergence
  check; a default-grid number is not a precision guarantee.
- The existing FD engine supplies NPV, delta, gamma and theta where available.
  Unexpired FD vega, rho, dividend rho, error estimate and exercise probability
  remain unavailable and raise native errors. Expired instruments retain the
  ordinary option's zero-result handling.
- The analytic Black calculator does not support this payoff. `price(process)`
  or analytic engine attachment followed by a query raises an error rather than
  pricing a vanilla call/put. Other plain-payoff-only analytic/MC paths retain
  their existing payoff/exercise checks. No new visitor or engine compatibility
  is implied by this factory.

## Numerical qualifications

The strict point payoff is distinct from the FD engine's terminal cell averages,
from a positive-time option value, and from an expired instrument's result.
Neither NPV at spot equal to strike nor a grid node close to strike is a direct
point-payoff query.

- Same-grid comparisons use a separately compiled QuantLib 1.43 wheel. The local
  source oracle is revision `9863b578af0caa4cecabf697196533e84a8308b6`, a
  1.43-dev checkout; the wheel is not claimed to have been built from that checkout.
- The tracked 64-row reference has 40 ordinary positive-volatility/nonexpired rows
  and 24 separately classified finite boundary observations. Native errors are
  preserved separately, not represented as successful finite comparisons.
- For the eight one-year call/put strike 80/100/105/120 cases, the independent
  native maximum price error versus the truncated-lognormal moment at grids
  100/200/400/800 is approximately 0.40468/0.00669/0.00175/0.000747. Individual
  cases are not monotonically improving: strike 80 can worsen 400 to 800, and
  strike 105 can worsen 100 to 200. Do not infer precision from grid size alone.
- Zero volatility with flat equal risk-free/dividend rates collapses the
  existing log-FD grid and raises a native mesher error. This factory adds no
  deterministic or analytic fallback. Nonzero-drift zero-volatility, zero-strike
  and expiry-day results are boundary-policy observations, not ordinary
  continuum-convergence promises.
- At expiry the existing settings/expired-instrument path can return zero before
  asking the engine. Non-notifying settings flags keep their ordinary cached
  versus fresh behavior. This factory adds no silent-policy synchronization or
  new freeze/unfreeze surface.
- Ordinary tested outputs must be finite before comparison. No global finite
  output guard was added to existing option accessors, and inherited unsupported
  or extreme-input nonfinite outcomes are not silently converted into exceptions.

## Lifetime and errors

Settings are retained by the option. The attached FD engine retains its process,
curves and observable quote graph. Closing input wrappers or Python garbage
collection does not remove those retained dependencies. Observable quote and
evaluation-date notifications invalidate the ordinary lazy cache; result snapshots
remain frozen copies.

C keeps typed context-scoped handles and creating-thread confinement. Failed
constructor/query calls leave caller outputs unchanged under the C pointer
contract. Go rejects foreign-session, nil and closed inputs, serializes native
calls on its locked worker, and requires explicit Close. Python retains its
unsendable native ownership. A contained C context panic still poisons that context;
this factory does not change the general boundary contract.

## Examples and reference data

- Python: `example/python/asset_digital.py`.
- Go: `sdk/go/examples/asset_digital/main.go`.
- Independent CSV: `crates/itofin-py/tests/fixtures/asset-digital/asset_fd_reference.csv`,
  with a byte-identical SDK testdata copy. Tracked copies normalize only original
  CRLF line endings to LF; every numeric/reference cell string is unchanged.
- Original independent generator, moment-law reference and provenance are retained
  under `crates/libitofin-ffi/tests/fixtures/asset-digital-oracle/`. The executed generator is preserved byte-exact with its original
  absolute local evidence/oracle paths, explicitly not advertised as portable.

This slice does not expose signed stock domains, American asset digitals, gap or
percentage-strike products, or analytic Black asset-digital pricing. Original core
numerics and the wider deferred engine/payoff scope remain separate.
