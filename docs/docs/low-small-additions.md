# Small LOW Rust additions

This slice reviews standalone small entries from the fork's LOW inventory.
The initial screen used fork files under 200 total lines and the listed tiny
helpers in larger files. Tests, documentation and numerical hardening are not
constrained to 200 changed lines. This is not completion of all 47 LOW rows.

## Availability and compatibility

These are additive Rust-core APIs and generic numerical components. Existing
constructors, signatures, pricing engines and language-binding contracts remain
unchanged. No new C, Go or Python facade or release is introduced. Raw aliases
that would misstate QuantLib process-construction conventions are not added.

| LOW row | Addition | Contract |
| --- | --- | --- |
| 2 | `UsdLibor::six_months` | Delegates the existing tenor constructor, retaining forwarding-handle and settings state. |
| 4 | `CapFloor::last_overnight_coupon` | Borrows the final retained overnight coupon; returns `None` for Ibor/empty stores. The existing Ibor getter is unchanged. |
| 6, partial | `HullWhite::sigma` | Reads the current calibrated volatility parameter. |
| 7, partial | `BatesProcess::m` | Exposes the already-validated mean proportional jump using accurate `exp_m1`. |
| 10 | `math::matrix::{det3, inverse_3x3}` | Checked, exactly 3 by 3 matrix helpers with row scaling and pivoting. |
| 11 | `LmExponentialCorrelationModel` | Immutable exponential forward-row correlation and cached full-factor spectral square root. |
| 12 | `LmLinearExponentialVolatilityModel` | Immutable instantaneous volatility, preserving fixing-row order and zero at/after fixing. |
| 13 | `LfmCovarianceProxy` | Checked instantaneous covariance and diffusion for the two immutable legacy components. |
| 30 | `NullPayoff` | Dummy name/description contract, not a zero payoff or a new pricing engine. |
| 47 | `default_theta_per_day` | Exactly `theta / 365.0`, independent of curve day count; IEEE values propagate. |

## Numerical limits

- Matrix helpers return `QlResult` for malformed/nonfinite input, unresolved
  scaling and unrepresentable arithmetic. Numerically zero pivots produce zero
  determinant; singular or unresolved inverses error. No absolute determinant
  tolerance is imposed. Both inverse identity-product residuals must be at most
  `256 * Real::EPSILON`; conditioning can cause a nonsingular inverse to error.
- Inversion does not require a representable determinant: uniformly `1e200`
  and `1e-200` diagonal matrices can have finite inverses. A determinant outside
  the finite nonzero representable range errors instead of silently saturating.
- These are fixed-size floating-point helpers, not generic decomposition parity.
  Conditioning still governs accuracy; no condition estimate is supplied.
- Legacy inputs are finite with positive coefficients. Queries reject nonfinite
  time, invalid indices, incompatible dimensions and unrepresentable outputs.
  Volatility has a log-domain fallback for representable extreme-scale results.
- Correlation factors are compared through their products, not their orientation.
  The reused spectral backend retains its documented convergence panic limit.
- Legacy components provide no mutable calibration, LIBOR process, observable
  curves, pricing engines or integrated variance/covariance. Generic numerical
  building blocks remain Rust-only.
- `NullPayoff::value` deliberately panics with `dummy payoff given`, matching
  QuantLib's evaluation failure through this infallible payoff trait. Existing
  engines are not rewired to use it.

## Reconciled and withheld scope

| Small inventory entry | Disposition |
| --- | --- |
| Coupon getters and Merton handle getters | Already delivered; not duplicated. |
| Latest-date American constructor | Existing checked `AmericanExercise::until` already covers the valid-date use case. The fork's unchecked `from_latest` is not copied. |
| Hull-White `a`, `r0`, curve, `phi`, dynamics | `r0`/curve are delivered. An inherent zero-argument `a` could shadow the existing affine `a(t, maturity)` trait call. Fitting/dynamics remain owned by [#466](https://github.com/benbenbang/libitofin/issues/466). |
| Three Black-Scholes process aliases | Withheld: unrestricted generalized aliases do not enforce QuantLib's zero-dividend, forward-carry or named FX constructor conventions. |
| AUD LIBOR | Additional currency-Libor family owned by [#306](https://github.com/benbenbang/libitofin/issues/306); skipped. |
| Swing condition and FD swing engine | Depend on deferred multidimensional machinery under [#636](https://github.com/benbenbang/libitofin/issues/636); skipped. |
| Small exotic instrument wrappers | A small file is not a complete priced product. Matching engines, payoff/product prerequisites and binding consumers need larger verticals. No wrapper-only pricing support is claimed. |
| Writer-extensible pair | Its 122-line wrapper plus 189-line engine form a 311-line pricing vertical before bindings and consumer validation, outside this standalone-helper slice. |

General instrument/model/pricing epics are not treated as blanket ownership of
unassigned tiny helpers. No existing board issue is closed by these additions.
Black-Karasinski and other larger files were not selected by the original
under-200-total-file screen; narrower future slices need a separate review.

## Independent evidence

[Reference generation instructions](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/tests/fixtures/low_small)
separate inspected QuantLib source `9863b578af0caa4cecabf697196533e84a8308b6`
from the independent PyPI QuantLib 1.43 compiled wheel.

- Nine exact rational matrix references include pivots, singular rejection and
  uniform/mixed extreme scales; nonsingular inverses are checked against the
  compiled wheel.
- Four legacy observations use independently evaluated 80-digit source-law
  formulas. The wheel lacks these legacy model classes, so no compiled model
  invocation is claimed. Its spectral factor products provide a separate check.
- Rust tests bake the reference values and require no installed QuantLib.
  Existing compatibility and numerical tolerances are not relaxed.
