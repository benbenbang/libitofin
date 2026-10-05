# Variance-swap replication oracle

`generate.py` compiles and executes actual QuantLib `VarianceSwap`,
`ReplicatingVarianceSwapEngine` and `AnalyticEuropeanEngine` from commit
`9863b578af0caa4cecabf697196533e84a8308b6` (`1.43-dev`). No expected result is
computed by itofin. The native engine is header-defined; support implementations
are source-compiled, not linked from a differently versioned Python wheel.

```sh
python sdk/go/testdata/variance-swap/generate.py \
  --quantlib /path/to/pinned/QuantLib \
  --boost-include /path/to/boost/include \
  --build-dir /path/to/task-owned/native-build
```

The current build helper uses a reduced source closure and macOS dead stripping
to discard unrelated implied-volatility/FDM implementation paths. It extracts an
exact Git archive into the build directory, never modifying the source checkout.
Compiler identity, flags, actual compiled sources and compiler-discovered header
hashes are recorded in `oracle.json`. Every fixture uses fixed dates and
Actual365Fixed on all term structures. Rates are continuously compounded;
volatility and variance are annualized, not percentage points. Notional multiplies
variance difference directly, not volatility difference.

## Cases and cross-checks

- Native literature smile reproduces `test-suite/varianceswaps.cpp`, attributed
  there to Demeterfi, Derman, Kamal and Zou, *A Guide to Volatility and Variance
  Swaps* (1999). Its corrected maturity is exactly 90/365. The literature rounded
  variance `0.04189` has the original `1e-4` tolerance; JSON retains full precision.
- Small cases retain every option's type, strike, replication weight, price and
  weighted price. Flat strips exercise long/short, strike/notional changes,
  boundary choice, unsorted duplicates, negative rates/dividends, short maturity
  and zero volatility. Equal native variance strike gives exact zero NPV.
- The generator separately evaluates secant weights and Black prices with Python
  `math.erfc`, checks variance/NPV, then stores only the actual native outputs.
  Declared numeric tolerances are in `oracle.json`; finite checks precede them.
  Native deep-OTM puts at seven days have tiny negative roundoff prices
  (magnitude below `6e-15`), retained verbatim rather than silently clamped.
- Dense flat-vol refinement stores deterministic grid recipes, input hashes and
  native scalar values, not large weight tables. With zero dividends, the
  continuous infinite-strip limit is sigma squared. Finite strips have truncation
  and discretization bias; these fixtures are not an accuracy guarantee.

## Intentional native differences and limits

Pinned native `ReplicatingVarianceSwapEngine` does not observe its process.
`live_updates.json` keeps a retained engine/swap and shows cached values after
spot/rate/dividend/volatility changes, then explicitly calls `recalculate()`.
Live itofin observers should match the recalculated values, not the stale cache.
Replication weights depend on the strip and residual time, not market quotes.

The native drift convention has continuous flat-vol limit
`sigma^2 + 2*q + 2*S/(T*f)*(exp((r-q)*T)-exp(r*T))`, with common boundary `f`.
It is not generally sigma squared for nonzero dividend yield. The paired facade
preserves this convention; it is not an unbiased realized-variance expectation.

Native synthetic put tail zero can produce nonfinite log-payoff weights. The
paired facade intentionally rejects nonpositive/unrepresentable tails, invalid
market inputs and oversized strips before pricing. Native ignores startDate in
replication; supported facade contracts are spot-start only, not accrued or
forward-start swaps. No Monte Carlo variance engine is covered here.

## Attribution

Upstream sources remain external and retain their original notices and
[QuantLib license](https://www.quantlib.org/license.shtml). The replication engine
credits Warren Chou and StatPro Italia; the European engine credits Ferdinando
Ametrano and StatPro Italia. The generated native measurements and independent
fixture adapter do not remove or replace those notices.
