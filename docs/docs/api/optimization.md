# Optimization

QuantLib-style methods for fitting Heston and Hull-White models. Pass any of
these methods to `HestonModel.calibrate*` or `HullWhite.calibrate*`, with an
`EndCriteria` to set the stopping thresholds.

| Method | Choose it when |
| --- | --- |
| `LevenbergMarquardt` | Fitting least-squares errors with the usual calibration default. |
| `Simplex(lambda_)` | You want a derivative-free search and can choose a positive starting scale. |
| `ConjugateGradient` | You want a gradient-based search using the core Armijo line search. |
| `SteepestDescent` | You want a basic gradient-based search using the same line search. |

These methods belong to `itofin.optimization`. The separate `itofin.optimize`
API for standalone scalar minimization is tracked in
[EPIC-OPT](https://github.com/benbenbang/libitofin/issues/1078).

Every `HestonModel.calibrate*` and `HullWhite.calibrate*` method also accepts
keyword-only `constraint=`, `weights=`, and `fix_parameters=` arguments. Use
`NoConstraint()`, `PositiveConstraint()`, `BoundaryConstraint(low, high)`, or
`CompositeConstraint(a, b)` for an additional constraint intersected with the
model's own parameter limits. A composite copies its children.

`weights` has one entry per helper. `fix_parameters` has one Boolean per model
parameter: Heston uses `(theta, kappa, sigma, rho, v0)` and Hull-White uses
`(a, sigma)`. Omit either argument for the core default. Hull-White's existing
`fix_reversion=True` fixes `a`; it cannot be combined with a nonempty
`fix_parameters` mask.

::: itofin.optimization
