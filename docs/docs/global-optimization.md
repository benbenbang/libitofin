# Global optimization

Particle swarm and hybrid simulated annealing are not included in v0.37.0
artifacts; build from this source checkout until the next coordinated release.

Differential evolution (DE) and particle swarm (PSO) are seeded, box-bounded
population searches, available in Rust, C, Go and Python. Both solvers live in
finance-independent `itofin-optimize`;
existing binding artifacts include it without another optimizer installation.
Hybrid simulated annealing adds a single-chain search with local coordinate
polling. All three share bounds, budgets and result types, while retaining
separate update rules and controls. Node and firefly search are not implemented.

## Two entry points

| Need | API | Objective |
| --- | --- | --- |
| Standalone minimization | Rust `minimize`, Python `itofin.optimize.minimize`, Go `Minimize`, context-free C optimizer | Signed scalar `f(x)`, including negative values |
| Existing model calibration | Rust `OptimizationMethod`, Python `itofin.optimization.{DifferentialEvolution, ParticleSwarm, HybridSimulatedAnnealing}`, Go `Session.NewDifferentialEvolution` / `Session.NewParticleSwarm` / `Session.NewHybridSimulatedAnnealing` | Existing cost function's scalar value |

The standalone API rejects an out-of-box `x0`. The calibration adapter instead
clamps initially valid model parameters into the effective search box before
starting; subsequent candidates still require the model constraint check.

The standalone API does **not** turn a scalar into a residual or minimize its
absolute value. The default QuantLib-style cost function uses residual RMS;
model calibration overrides it with `sqrt(sum(weight[i] * error[i]²))`. The
adapter calls that existing scalar operation directly, preserving its semantics.
This is **root-RSS**, not the unrooted sum of squared residuals.

## Search contract

### DE/PSO population settings

| Setting | Meaning / default |
| --- | --- |
| Bounds | One finite ordered pair per coordinate; finite interval width required |
| `x0` | Finite, inside the box; never silently projected |
| `seed` | `u64`, default `0`; every value, including zero, is deterministic |
| `population_size` | Actual member count, **not** a dimension multiplier; default `max(8, 15 * free_dimensions)` |
| `initial_population` | Optional full population; its row count supplies an omitted size; conflicting size is rejected |
| `xatol` | Maximum coordinate spread divided by that coordinate's bound width; default `1e-6` |
| `fatol` | Maximum absolute objective spread; default `1e-8` |
| `maxiter` / `maxfev` | Completed generations / actual objective evaluations; defaults `1000` / `1,000,000` |

Explicit tolerances override Rust `Common.tol`, which otherwise overrides the
listed defaults. Coordinate spread uses actual physical population extrema,
divided by the finite bound width. With `xatol=0`, each physical coordinate must
have exactly zero spread; rounded normalization alone cannot imply convergence.
Both coordinate and objective spreads must satisfy their tolerances. PSO also
requires every free-coordinate absolute normalized velocity to be at most
`xatol`. These are heuristic stopping tests, not a global-optimum guarantee.

### Differential evolution

| Control | Meaning / default |
| --- | --- |
| `mutation` | Differential weight `F`, finite in `(0, 2]`; default `0.8` |
| `recombination` | Binomial crossover probability, finite in `[0, 1]`; default `0.9` |

- Strategy: serial **DE/rand/1/bin**, with three distinct donors excluding the
  target. At least one free coordinate crosses over, even with probability zero.
- Every generation reads frozen parent points and scores. Equal objective values
  accept the trial, an explicit implementation tie rule.
- Out-of-box donor coordinates are uniformly resampled inside their bounds.
  There is no implicit polishing, parallel execution or general-constraint solver.

### Particle swarm

| Control | Meaning / default |
| --- | --- |
| `inertia` | Previous-velocity coefficient, finite in `[0, 1]`; default `0.7` |
| `cognitive` | Personal-best attraction, finite in `[0, 4]`; default `1.4` |
| `social` | Global-best attraction, finite in `[0, 4]`; default `1.4` |
| `velocity_clamp` | Maximum absolute box-normalized velocity, finite in `(0, 1]`; default `0.2` |

- Serial, synchronous **global-best** topology: each generation reads frozen
  positions, personal bests and the swarm's best. All initial velocities are zero.
- Each free coordinate draws independent uniform `r1`, then `r2`, and computes
  `v = inertia*v + cognitive*r1*(pbest-x)/width + social*r2*(gbest-x)/width`.
  Velocity is clipped to `[-velocity_clamp, velocity_clamp]`; position moves
  in normalized box coordinates. These defaults and policies are ours, not a
  promise to reproduce another PSO library or the papers' experiments.
- Crossing positions are clipped to the exact bound, with outward velocity
  zeroed there (absorbing bounds). Zero velocity preserves the physical coordinate.
- Personal/global bests update only on strict improvement. Equal values retain
  the earliest evaluated best. There is no neighborhood topology, inertia
  schedule, constriction-factor mode, polishing or parallel execution.
- Explicit zero inertia, cognitive or social coefficients are supported.
  DE controls are rejected for PSO, and PSO controls are rejected for DE.

### Hybrid simulated annealing

| Control | Meaning / default |
| --- | --- |
| `initial_temperature` | Finite positive temperature in **objective units**; default `1.0` |
| `cooling_rate` | Geometric factor, finite in `(0, 1)`; default `0.95` |
| `step_size` | Uniform proposal half-width divided by box width, finite in `(0, 1]`; default `0.25` |
| `local_search_interval` | Cycles between local polls, `1..=1,000,000`; default `10` |
| `local_search_steps` | Maximum coordinate sweeps per local poll, `1..=256`; default `4` |
| `reanneal_interval` | Cycles between temperature resets, `1..=1,000,000`; default `100` |
| `xatol` / `fatol` | Normalized unsuccessful poll radius / absolute poll-cost spread; defaults `1e-6` / `1e-8` |

- One chain starts at `x0`; there is no population. Each free coordinate draws a
  uniform displacement in `[-step_size, step_size]` in normalized box space.
  Boundary-crossing proposals reflect into the box; fixed coordinates stay exact.
- Downhill and equal-cost proposals are accepted without another random draw.
  An uphill proposal is accepted with probability `exp(-(trial-current)/T)`.
  Rescaling an objective therefore changes the meaning of the temperature.
- Every local-search interval, poll the best evaluated point in coordinate order,
  positive then negative. Polls clip to the box, skip unchanged points, and retain
  strict improvements immediately. This is exploratory coordinate search, **not
  the full Hooke-Jeeves pattern-move algorithm**.
- An unsuccessful complete sweep halves the local radius. The radius starts at
  `step_size`, persists across polls and is not reset by reannealing. Local search
  returns the chain to its best point; reannealing also resets temperature and
  returns to the best, without discarding that result.
- A completed cycle contains the proposal and any scheduled local poll. Its
  callback receives the best evaluated point. Cooling follows that callback;
  periodic reannealing then resets temperature to `initial_temperature`.
  Cooling is floored at the smallest positive `f64`, not zero.
- Success requires a complete unsuccessful poll with radius `<= xatol` and finite
  observed absolute cost spread `<= fatol`. This reports **local search
  resolution**, not a global optimum or a classical asymptotic annealing guarantee.
  Explicit tolerances override `Common.tol`, then the listed defaults apply.
  Multimodal problems can successfully converge to a non-global local minimum;
  compare independently known objective values and more than one seed.
- DE/PSO population controls are rejected. The same finite box, dimension and
  budget caps apply; population-size caps do not apply to this single-chain method.

### Population initialization and shared limits

- Random initialization includes `x0` as member zero. An explicit population is
  validated completely and preserved without inserting `x0`.
- Fixed coordinates retain their exact bound. An all-fixed problem evaluates
  `x0` once, with zero iterations, after validating all options and any explicit
  population.
- Local SplitMix64 sampling needs no third-party optimizer or runtime dependency.
  Repeated seeds reproduce this implementation, not another library's trajectory.

Resource limits: at most **256 coordinates**, **4096 population members** and
**1,000,000 population coordinates**. Population size is at least four. Iteration
and evaluation budgets are capped at **1,000,000** and **10,000,000** respectively.
All three solvers require a finite box and reject general constraints.
Invalid shapes, nonfinite inputs and nonfinite interval widths are rejected before the objective runs. Python `jac` is unsupported; the
Rust solver never calls `Objective::gradient`.

## Results, callbacks and errors

The existing `OptimizeResult` reports the best evaluated point, its matching
scalar value, status, success, completed generations/cycles (`nit`), objective
calls (`nfev`) and zero gradient calls (`njev`). Exhaustion is not success.

- Initialization consumes objective evaluations, not generations/cycles.
- Partial initialization or a partial generation/cycle can exhaust the evaluation
  budget; neither increments `nit` nor invokes an iteration callback. Annealing
  proposal and local-poll evaluations share that same budget.
- Callbacks run only after a completed generation/cycle. Cancellation waits
  for that boundary; it wins over iteration exhaustion at the same boundary.
- Every nonfinite objective return terminates with the nonfinite status. No
  infinity barrier or penalty value replaces an exception or callback error.
- Python exceptions and Go errors preserve their existing propagation behavior.
  Python callback `StopIteration` requests cancellation; Go uses `context.Context`.
- The C scalar entry point is context-free. Go objectives run outside a Session
  operation, so they may update quotes and reprice using existing Session APIs.
  Callback buffers are borrowed only for the duration of the call.

## C options and ownership

The context-free `itofin_optimize_differential_evolution` and
`itofin_optimize_particle_swarm` accept separate bounds arrays and an optional
flattened row-major population. `ItofinDifferentialEvolutionOptions` and
`ItofinParticleSwarmOptions` contain `ItofinGlobalOptions`. Zero population/budget
fields select defaults;
`has_xatol`, `has_fatol`, `has_mutation` and `has_recombination` distinguish omitted
values from explicit zero. In particular, zero crossover and zero tolerances are
expressible. PSO has corresponding `has_inertia`, `has_cognitive`, `has_social`
and `has_velocity_clamp` flags; zero attraction coefficients are expressible.
Calibration uses `itofin_differential_evolution_new/result` or
`itofin_particle_swarm_new/result` on a Session. Objective release runs exactly
once, including rejected inputs and callback errors; an error return leaves the caller's result buffer untouched.

Hybrid annealing uses context-free `itofin_optimize_hybrid_simulated_annealing`
and Session `itofin_hybrid_simulated_annealing_new/result`. Bounds are passed
as separate arrays; `ItofinHybridSimulatedAnnealingOptions` carries schedule
controls, not `ItofinGlobalOptions` or a population. Presence flags distinguish
omitted schedule controls and tolerances from explicit values. An all-zero
record selects defaults. Explicit zero schedule controls are invalid; explicit
zero tolerances are valid. Zero `maxiter`/`maxfev` fields select default budgets.

## Calibration limits and diagnostics

Construct the calibration method with bounds in **free/projected parameter
order**, not full model order when parameters are fixed. Existing model and
additional constraints still apply; the effective box intersects their limits.

**The entire search box must be feasible for the model.** Every candidate is
checked before pricing. A violation aborts with an error and existing model
rollback, rather than performing rejection sampling or inventing a penalty.
Coupled domains such as GJR stationarity or the Heston Feller condition need a
carefully chosen box; simple finite bounds alone do not guarantee feasibility.

- `EndCriteria.max_iterations` also limits generations/cycles. Each solver's own
  convergence tolerances apply; the other `EndCriteria` tolerances are not substituted.
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

## Paired executable examples

### Differential evolution

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

### Particle swarm

The PSO examples check the same signed quadratic and report actual generations
and objective calls. Python uses `method="particle-swarm"`; its calibration
example uses `optimization.ParticleSwarm` with the same Hull-White setup above.
The existing root-RSS calibration cost is unchanged.

=== "Python"

    ```python
    --8<-- "example/python/particle_swarm.py"
    ```

=== "Rust"

    ```rust
    --8<-- "crates/libitofin/examples/particle_swarm.rs"
    ```

=== "Go"

    ```go
    --8<-- "sdk/go/examples/particle_swarm/main.go"
    ```

### Hybrid simulated annealing

The three examples check the same signed quadratic, coordinates within `1e-5`,
value within `1e-8`, and actual objective-call accounting. Python uses
`method="hybrid-simulated-annealing"` and also fits the same one-parameter
Hull-White model with `optimization.HybridSimulatedAnnealing`. The temperature
acts on the model's existing root-RSS cost, not on an unrooted sum of squares
or a transformed scalar.

=== "Python"

    ```python
    --8<-- "example/python/hybrid_simulated_annealing.py"
    ```

=== "Rust"

    ```rust
    --8<-- "crates/libitofin/examples/hybrid_simulated_annealing.rs"
    ```

=== "Go"

    ```go
    --8<-- "sdk/go/examples/hybrid_simulated_annealing/main.go"
    ```

## Algorithm references

Implemented independently from Storn and Price (1997),
[Differential Evolution: A Simple and Efficient Heuristic for Global Optimization
over Continuous Spaces](https://doi.org/10.1023/A:1008202821328).
The local generator follows Steele, Lea and Flood (2014),
[Fast Splittable Pseudorandom Number Generators](https://doi.org/10.1145/2660193.2660195).

PSO is implemented independently from Kennedy and Eberhart (1995),
[Particle Swarm Optimization](https://doi.org/10.1109/ICNN.1995.488968), with
[the authors' paper hosted by the University of Washington](https://staff.washington.edu/paymana/swarm/kennedy95-ijcnn.pdf),
and the inertia formulation in Shi and Eberhart (1998),
[A Modified Particle Swarm Optimizer](https://doi.org/10.1109/ICEC.1998.699146).
Our synchronous update, zero initial velocity, absorbing bounds and convergence
policies are explicit implementation choices, not unchanged paper defaults.

Hybrid annealing uses the Metropolis acceptance rule described by Kirkpatrick,
Gelatt and Vecchi (1983),
[Optimization by Simulated Annealing](https://doi.org/10.1126/science.220.4598.671),
and exploratory coordinate polling inspired by Hooke and Jeeves (1961),
[Direct Search Solution of Numerical and Statistical Problems](https://doi.org/10.1145/321062.321069).
Reflected uniform proposals, geometric cooling, periodic best-point reannealing,
local polling and finite-budget stopping are explicit implementation policies.

No upstream optimizer implementation is copied or imported. This is not a claim
of clean-room provenance or QuantLib/SciPy trajectory equivalence.
