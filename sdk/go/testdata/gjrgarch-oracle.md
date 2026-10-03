# GJR-GARCH process and seeded-path oracle

Issue [#1164](https://github.com/benbenbang/libitofin/issues/1164) covers processes
and simulation only. Calibration/models/engines remain #1165.

## Independent provenance

- Transition and path values come from native **QuantLib-Python 1.43**, not Rust,
  C, Go or Python itofin. `apply` is an independent Python formula because SWIG
  does not expose it. Analytic moments use Gaussian/lognormal identities.
- Contract source: QuantLib commit `9863b578af0caa4cecabf697196533e84a8308b6`,
  `ql/processes/gjrgarchprocess.cpp`; SHA256
  `63044072abb7df8449e9e76273f752386976003659fa66fb407c6af9d5b71da0`.
- The published 1.43 executable is a version-pinned independent oracle, not a
  claimed binary build of the repository commit. Its selected coefficients are
  checked against independent `math.erfc` formulas from the pinned C++ source.
- Regenerate with `python sdk/go/testdata/generate_gjrgarch_oracle.py` in an
  environment containing QuantLib 1.43. No itofin import is used.

## Units and transitions

| Contract | Behavior |
| --- | --- |
| Constructor variance and omega | Daily constants; `v_initial = days_per_year * daily_variance` |
| State variance | Annual variance, including raw negative intermediate values |
| Time and dt | Years; independent standard-normal inputs are scaled by `sqrt(dt)` |
| Variance drift | `D² omega + D (beta + alpha q2 + gamma q3 - 1) v_effective` |
| Partial truncation | Raw variance for drift/base; max variance for volatility |
| Full truncation | Max variance for drift/volatility; raw variance for base |
| Reflection evolve | Positive `sqrt(abs(v))`; variance base is `abs(v)` |
| Reflection drift/diffusion | Negative `sqrt(-v)` for negative variance |
| Truncation diffusion at nonpositive variance | Volatility floor `1e-8`; evolve uses exact zero |
| Zero-time reflection from negative variance | Returns absolute variance, not the original negative value |
| Apply | `spot * exp(dx0)`, `variance + dx1` |

The executable uses `sigma2 = 2 + 4 lambda²`; the header's `lambda⁴` is a typo.
Reflection's query/transition sign difference is intentional upstream behavior.
Stationarity is **not** required for finite-horizon simulation. QuantLib's process
constructor has no parameter checks. Our economic nonnegative daily-variance
policy is distinct from QuantLib calibration constraints: that model restricts
alpha/beta to [0,1], gamma to [-1,1], positive omega/v0, and `beta + gamma >= 0`.

## Fixture checks

- 27 transitions: all schemes; positive/zero/negative variance; both spot shocks;
  varied lambda/days; negative gamma; zero omega/v0; nonstationarity; zero dt.
- Six seeded cases use one MT19937 stream, uniform `(word + 0.5) / 2³²`, Acklam
  inverse normal without refinement, in path/step/factor0/factor1 order.
- Full layout is `[path, time, state]`, state `[spot, annual_variance]`; terminal
  output contains pairs. High-leverage cases cross below zero in all schemes.
- Twelve recorded word/normal/state rows isolate stream-consumption changes.
- A 40,000-draw one-step native oracle checks spot mean/second moment, variance
  mean/variance and log-spot/variance covariance within six standard errors.
- Fixture comparison uses relative `2e-13`, absolute `1e-12`. The latter accounts
  for QuantLib's instantaneous flat-forward rate rounding, about `3.5e-13` in
  the baseline spot drift. Existing oracle bounds remain unchanged.
