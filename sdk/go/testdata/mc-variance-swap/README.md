# Monte Carlo variance-swap oracle

`generate.py` compiles actual QuantLib `MCVarianceSwapEngine<PseudoRandom>`,
`VariancePathPricer`, `GeneralizedBlackScholesProcess`, `BlackVarianceCurve`,
`TimeGrid`, `SegmentIntegral`, MT19937 and native statistics from commit
`9863b578af0caa4cecabf697196533e84a8308b6` (`1.43-dev`). Expected measurements
come from that binary, not itofin or a differently versioned Python wheel.

```sh
python sdk/go/testdata/mc-variance-swap/generate.py \
  --quantlib /path/to/pinned/QuantLib \
  --boost-include /path/to/boost/include \
  --build-dir /path/to/task-owned/native-build
```

The helper extracts the exact Git archive, compiles the support closure and
records compiler identity, flags, translation units and compiler-discovered
upstream source/header SHA256s. Current macOS builds dead-strip unused support
functions. Two clean builds reproduce every JSON byte; linkage uses system C++
libraries only. No installed QuantLib headers or upstream notices are replaced.

## Fixtures and independent replay

- `cases.json`: nine native cases with fixed dates, Actual365Fixed and seed42.
  The original `testMCVarianceSwap` uses 36/365 and 90/365 variance-curve knots,
  Black vols .1/.2, 250 steps/year and 1023 samples. Annualized variance .04 is
  checked with its unchanged `3e-4` literature tolerance. Native full precision
  is retained; time-only local-vol integration is independently replayed.
- Constant-vol, short position, zero-vol, negative rate/dividend, two-sample,
  minimum-grid, explicit-grid and absolute-tolerance modes are covered.
  For `T=3/365` and seven path steps, floating arithmetic creates six integration
  intervals. The independent replay preserves that separate interval count.
- `live_updates.json` retains one engine/swap, changes spot/rate/dividend/vol,
  and explicitly recalculates. Pinned native does not observe its process;
  itofin intentionally invalidates automatically and matches recalculated values.
- `external_cases.json` is Rust-only explicit external local-vol coverage:
  `sigma(t,S)=.15+.1*S/(S+100)`. This bounded adapter is fixture-owned, not an
  upstream production engine change. Fixed/short cases have nonzero sampling
  error; tolerance mode exercises actual adaptive batches, not guessed counts.
- `replay_external.py` independently seeds Python's MT state with the standard
  integer recurrence, uses `statistics.NormalDist.inv_cdf`, evolves log states,
  integrates local variance and evaluates two-pass sampling error. It does not
  call native or itofin pricing. Independent adaptive replay matches sample count.
- Finite comparisons retain canonical tolerances: relative `3e-12`, variance
  and variance-error absolute `2e-14`, NPV and NPV-error absolute `2e-9`.
  Small time-only sampling errors are roundoff, not stochastic accuracy evidence.

## Semantics and limits

The estimator integrates squared log-price local volatility over each generated
path, then divides by its floating terminal time. It does not square price
returns. Native trapezoid state indexing uses `floor(t/path_dt)` and preserves
repeated floating additions. No bridge, antithetic or control variate is enabled.

Annualized variance-error tolerance is not monetary NPV tolerance. Native NPV
error is `position_sign*discount*notional*variance_error`, therefore negative for
shorts. Actual sample count is reported. Sampling error excludes integration,
time-grid and model bias; constant volatility can report zero error on any grid.
These spot-start fixtures do not validate historical fixings or discrete accrued
variance. Upstream permissive inputs do not override itofin safety validation.

The engine credits Warren Chou. Upstream sources retain their notices and
[QuantLib license](https://www.quantlib.org/license.shtml).
