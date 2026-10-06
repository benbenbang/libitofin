# Global optimization

Differential evolution is a seeded, box-bounded population search, available in
Rust, C, Go and Python. The solver lives in finance-independent `itofin-optimize`;
existing binding artifacts include it without another optimizer installation.
Node is not implemented. This is the first shared contract for the remaining
global-solver requests, not an implementation of particle swarm, simulated
annealing or firefly search.

## Two entry points, one solver

| Need | API | Objective |
| --- | --- | --- |
| Standalone minimization | Rust `minimize`, Python `itofin.optimize.minimize`, Go `Minimize`, context-free C optimizer | Signed scalar `f(x)`, including negative values |
| Existing model calibration | Rust `OptimizationMethod`, Python `itofin.optimization.DifferentialEvolution`, Go `Session.NewDifferentialEvolution` | Existing cost function's scalar value |

The standalone API rejects an out-of-box `x0`. The calibration adapter instead
clamps initially valid model parameters into the effective search box before
starting; subsequent candidates still require the model constraint check.

The standalone API does **not** turn a scalar into a residual or minimize its
absolute value. The default QuantLib-style cost function uses residual RMS;
model calibration overrides it with `sqrt(sum(weight[i] * error[i]²))`. The
adapter calls that existing scalar operation directly, preserving its semantics.

## Search contract

| Setting | Meaning / default |
| --- | --- |
| Bounds | One finite ordered pair per coordinate; finite interval width required |
| `x0` | Finite, inside the box; never silently projected |
| `seed` | `u64`, default `0`; every value, including zero, is deterministic |
| `population_size` | Actual member count, **not** a dimension multiplier; default `max(8, 15 * free_dimensions)` |
| `initial_population` | Optional full population; its row count supplies an omitted size; conflicting size is rejected |
| `xatol` | Maximum coordinate spread divided by that coordinate's bound width; default `1e-6` |
| `fatol` | Maximum absolute objective spread; default `1e-8` |
| `mutation` | Differential weight `F`, finite in `(0, 2]`; default `0.8` |
| `recombination` | Binomial crossover probability, finite in `[0, 1]`; default `0.9` |
| `maxiter` / `maxfev` | Completed generations / actual objective evaluations; defaults `1000` / `1,000,000` |

Explicit tolerances override Rust `Common.tol`, which otherwise overrides the
listed defaults. Coordinate spread uses actual physical population extrema,
divided by the finite bound width. With `xatol=0`, each physical coordinate must
have exactly zero spread; rounded normalization alone cannot imply convergence.
Both coordinate and objective spreads must satisfy their tolerances.
Neither convergence nor a seeded run proves a global optimum.

- Strategy: serial **DE/rand/1/bin**, with three distinct donors excluding the
  target. At least one free coordinate crosses over, even with probability zero.
- Every generation reads frozen parent points and scores. Equal objective values
  accept the trial, an explicit implementation tie rule.
- Out-of-box donor coordinates are uniformly resampled inside their bounds.
  There is no implicit polishing, parallel execution or general-constraint solver.
- Random initialization includes `x0` as member zero. An explicit population is
  validated completely and preserved without inserting `x0`.
- Fixed coordinates retain their exact bound. An all-fixed problem evaluates
  `x0` once, with zero generations, after validating any explicit population.
- Local SplitMix64 sampling needs no third-party optimizer or runtime dependency.
  Repeated seeds reproduce this implementation, not another library's trajectory.

Resource limits: at most **256 coordinates**, **4096 population members** and
**1,000,000 population coordinates**. Population size is at least four. Iteration
and evaluation budgets are capped at **1,000,000** and **10,000,000** respectively.
Invalid shapes, general constraints, nonfinite inputs and nonfinite interval
widths are rejected before the objective runs. Python `jac` is unsupported; the
Rust solver never calls `Objective::gradient`.

## Results, callbacks and errors

The existing `OptimizeResult` reports the best evaluated point, its matching
scalar value, status, success, generations (`nit`), objective calls (`nfev`) and
zero gradient calls (`njev`). Exhaustion is not success.

- Initialization consumes objective evaluations, not generations.
- Partial initialization or a partial generation can exhaust the evaluation
  budget; neither increments `nit` nor invokes an iteration callback.
- Callbacks run only after a completed generation. Cancellation therefore waits
  for that boundary; it wins over iteration exhaustion at the same boundary.
- Every nonfinite objective return terminates with the nonfinite status. No
  infinity barrier or penalty value replaces an exception or callback error.
- Python exceptions and Go errors preserve their existing propagation behavior.
  Python callback `StopIteration` requests cancellation; Go uses `context.Context`.
- The C scalar entry point is context-free. Go objectives run outside a Session
  operation, so they may update quotes and reprice using existing Session APIs.
  Callback buffers are borrowed only for the duration of the call.

## C options and ownership

`itofin_optimize_differential_evolution` accepts separate bounds arrays and an
optional flattened row-major population. `ItofinDifferentialEvolutionOptions`
contains `ItofinGlobalOptions`. Zero population/budget fields select defaults;
`has_xatol`, `has_fatol`, `has_mutation` and `has_recombination` distinguish omitted
values from explicit zero. In particular, zero crossover and zero tolerances are
expressible. Objective release runs exactly once, including rejected inputs and
callback errors; an error return leaves the caller's result buffer untouched.

## Calibration limits and diagnostics

Construct the calibration method with bounds in **free/projected parameter
order**, not full model order when parameters are fixed. Existing model and
additional constraints still apply; the effective box intersects their limits.

**The entire search box must be feasible for the model.** Every candidate is
checked before pricing. A violation aborts with an error and existing model
rollback, rather than performing rejection sampling or inventing a penalty.
Coupled domains such as GJR stationarity or the Heston Feller condition need a
carefully chosen box; simple finite bounds alone do not guarantee feasibility.

- `EndCriteria.max_iterations` also limits generations. The global method's own
  spread tolerances apply; the other `EndCriteria` tolerances are not substituted.
- Rust `last_result()`, Python `last_result()` and Go `LastResult()` expose exact
  diagnostics from the last solver invocation. Entering the adapter clears its
  previous result. Model/binding preflight failures before the adapter runs leave
  the previous solver diagnostics unchanged; they do not describe the failed fit.
- Legacy `EndCriteriaType` has no evaluation-budget status: that outcome maps to
  `Unknown`, while the method's result retains `MaxEvaluations`.
- Solver `nfev` excludes the model's final residual evaluation. After a completed
  fit, the model's reported evaluation count normally equals solver `nfev + 1`.

See the [calibration API](api/optimization.md) for method dispatch and existing
weights, fixed-parameter masks and constraint arguments.

## Paired executable example

The signed quadratic has analytic optimum `(1.25, -0.75)` and value `-3`.
Each example checks coordinates within `1e-5` and objective value within `1e-8`.
The Python example also fits Hull-White volatility to one swaption, fixing mean
reversion. Its one-dimensional search bound is for free `sigma` only, and it
checks the calibration cost below `1e-6`, rather than inventing a reference sigma.

=== "Python"

    ```python
    --8<-- "example/python/differential_evolution.py"
    ```

=== "Rust"

    ```rust
    --8<-- "crates/libitofin/examples/differential_evolution.rs"
    ```

=== "Go"

    ```go
    --8<-- "sdk/go/examples/differential_evolution/main.go"
    ```

## Algorithm reference

Implemented independently from Storn and Price (1997),
[Differential Evolution: A Simple and Efficient Heuristic for Global Optimization
over Continuous Spaces](https://doi.org/10.1023/A:1008202821328).
The local generator follows Steele, Lea and Flood (2014),
[Fast Splittable Pseudorandom Number Generators](https://doi.org/10.1145/2660193.2660195).

No upstream optimizer implementation is copied or imported. This is not a claim
of clean-room provenance or QuantLib/SciPy trajectory equivalence.
