# Seeded simulation

For asymmetric-volatility spot/variance paths, see
[GJR-GARCH process and paths](gjrgarch.md).

`itofin.simulate_gbm` generates geometric Brownian paths with a fixed,
nonzero seed. It shares the Rust simulation kernel with Go's `SimulateGBM`:
the normal stream is consumed in path, time, asset order, so the same inputs
produce bit-identical float64 values across the two bindings. `gaussian_draws`
shares Go's `GaussianDraws` stream.

```python
import itofin

paths = itofin.simulate_gbm(
    initial=[100.0, 70.0],
    drift=[0.05, -0.02],
    volatility=[0.2, 0.3],
    correlation=[1.0, 0.5, 0.5, 1.0],
    horizon=1.0,
    steps=12,
    paths=100,
    seed=42,
)
assert paths.shape == (100, 13, 2)
assert paths[0, 0].tolist() == [100.0, 70.0]

terminal = itofin.simulate_gbm(
    initial=[100.0, 70.0], drift=[0.05, -0.02],
    volatility=[0.2, 0.3], correlation=[1.0, 0.5, 0.5, 1.0],
    horizon=1.0, steps=12, paths=100, seed=42,
    terminal_only=True,
)
assert terminal.shape == (100, 2)
assert (terminal == paths[:, -1, :]).all()
```

`correlation=None` selects independent assets. Otherwise, provide a flat,
row-major, symmetric, unit-diagonal, positive-definite matrix. Singular
positive-semidefinite matrices are rejected. Volatility may be zero; horizon
may be zero. The output is a C-ordered NumPy `float64` array.

By default, output is limited to
`itofin.DEFAULT_MAX_OUTPUT_VALUES == 16_777_216` values (128 MiB). Set
`max_output_values` to a positive number to override it; zero selects the
default. `gaussian_draws(count, seed)` always uses the default limit. Both
calls reject seed zero because the core interprets it as nondeterministic.
Use `terminal_only=True` or smaller batches to reduce memory use.

The kernel moved from `libitofin-ffi` to `libitofin` so the Python facade and
the existing C/Go facade call one implementation. The C ABI and Go API stay
the same.

::: itofin.simulate_gbm

::: itofin.gaussian_draws

## Ornstein-Uhlenbeck paths

`itofin.simulate_ou` and Go's `SimulateOU` use the existing exact
`OrnsteinUhlenbeckProcess` transition. They share the same seeded normal
stream: one draw per path and step, including deterministic runs. Full output
is `[path, time]` with the initial state at time zero; terminal output is
`[path]` and matches the last full-path value exactly.

```python
ou = itofin.simulate_ou(
    initial=1.0, level=3.0, speed=0.5, volatility=0.2,
    horizon=1.0, steps=12, paths=100, seed=42,
)
assert ou.shape == (100, 13)
assert (ou[:, 0] == 1.0).all()
```

```go
paths, err := itofin.SimulateOU(itofin.OUConfig{
    Initial: 1, Level: 3, Speed: 0.5, Volatility: 0.2,
    Horizon: 1, Steps: 12, Paths: 100, Seed: 42,
})
if err != nil { return err }
fmt.Println(paths.Paths, paths.Times)
```

Speed, volatility and horizon must be finite and nonnegative; initial and
level must be finite. Steps, paths and seed must be positive. The same
128 MiB default output limit and `max_output_values` override as GBM apply.
Use terminal mode for larger batches.

The Rust, C ABI, Go and Python tests pin a three-path, four-step fixture with
`initial=1`, `level=3`, `speed=0.5`, `volatility=0.2`, `horizon=1` and
`seed=42`. It was derived with standalone Python arithmetic, without calling
the simulation kernel: initialize reference MT19937 with seed 42, map each
word `w` to `(w + 0.5) / 2**32`, apply Acklam's inverse-normal rational
approximation without refinement, then evaluate
`x_next = 3 + (x - 3) * exp(-0.125) + sqrt(0.04 * (1 - exp(-0.25))) * z`.
The stream continues across paths, with four consecutive draws per path.

| Path | MT19937 words, in step order | Normal draws, in step order |
| --- | --- | --- |
| 0 | 1608637542, 3421126067, 4083286876, 787846414 | -0.31985239197154675, 0.82933648347268585, 1.6518193787298954, -0.90235263066587656 |
| 1 | 3143890026, 3348747335, 2571218620, 2563451924 | 0.61885464008120961, 0.77115003625180822, 0.24987627978810856, 0.24520245306794447 |
| 2 | 670094950, 1914837113, 669991378, 429389014 | -1.0109564443577614, -0.13619703980908432, -1.0110572131821498, -1.2816944810144082 |

| Path | Time 0 | Time 0.25 | Time 0.5 | Time 0.75 | Time 1 |
| --- | --- | --- | --- | --- | --- |
| 0 | 1 | 1.2049197140571397 | 1.4938576175387845 | 1.8262101587088548 | 1.879255526298315 |
| 1 | 1 | 1.2932179159179402 | 1.5663072780654845 | 1.758274886469132 | 1.9272460691202662 |
| 2 | 1 | 1.139911950142801 | 1.3456668679224093 | 1.4449524117278607 | 1.50711446963386 |

Each binding checks every full-path and terminal value against these constants
with absolute tolerance `2e-15` and also requires terminal/full equality
within the same build. The tolerance allows final-bit differences in platform
math libraries; the fixture never recomputes expectations through the shared
kernel under test.

::: itofin.simulate_ou

## Heston spot and variance paths

`itofin.simulate_heston` and Go `SimulateHeston` use the existing Heston
process's Andersen quadratic-exponential (`qe`) and martingale-corrected
quadratic-exponential (`qem`) schemes. Python and Go default to `qem`; Go can
select `HestonQE` or `HestonQEM`. C uses scheme 0 for QEM and 1 for QE.
Other process schemes remain outside this facade.

```python
paths = itofin.simulate_heston(
    spot=100.0, variance=0.04,
    risk_free_rate=0.05, dividend_yield=0.02,
    kappa=1.2, theta=0.06, sigma=0.3, rho=-0.5,
    horizon=1.0, steps=12, paths=100, seed=42, scheme="qem",
)
assert paths.shape == (100, 13, 2)
assert paths[0, 0].tolist() == [100.0, 0.04]
terminal = itofin.simulate_heston(
    spot=100.0, variance=0.04,
    risk_free_rate=0.05, dividend_yield=0.02,
    kappa=1.2, theta=0.06, sigma=0.3, rho=-0.5,
    horizon=1.0, steps=12, paths=100, seed=42,
    terminal_only=True,
)
assert (terminal == paths[:, -1, :]).all()
```

```go
paths, err := itofin.SimulateHeston(itofin.HestonConfig{
    Spot: 100, Variance: 0.04, RiskFreeRate: 0.05, DividendYield: 0.02,
    Kappa: 1.2, Theta: 0.06, Sigma: 0.3, Rho: -0.5,
    Horizon: 1, Steps: 12, Paths: 100, Seed: 42, Scheme: itofin.HestonQEM,
})
if err != nil { return err }
fmt.Println(paths.Paths, paths.Times, paths.Assets)
```

Full output is C-ordered `[path, time, component]`, including time zero;
terminal output is `[path, component]`. Component 0 is spot and component 1
is variance. Two independent standard normals per path and step are consumed
in spot-factor then variance-factor order. The process applies `rho` internally.
A nonzero seed restarts the same MT19937/inverse-normal stream on every call.
Zero horizon repeats the initial state exactly. Flat risk-free and dividend
rates use the horizon's time unit.

Spot must be positive, initial variance nonnegative, `kappa`, `theta`, and
`sigma` positive, `rho` between -1 and 1, and horizon nonnegative. All scalar
inputs must be finite. Steps, paths, and seed must be positive. The same
16,777,216-value default allocation limit and terminal-mode option apply as
for GBM; an explicit positive `max_output_values` overrides the limit.

Tests pin independent one-step Andersen QE calculations with seed 42's first
normal pair `(-0.31985239197154675, 0.8293364834726858)`, in spot/variance
order. Both cases use spot 100, rates 0.05/0.02 and rho -0.5:

| Scheme | Initial variance | kappa | theta | sigma | Time step | Spot | Variance |
| --- | --- | --- | --- | --- | --- | --- | --- |
| QEM | 0.04 | 1.2 | 0.06 | 0.3 | 0.25 | 93.21873664131503 | 0.06567515852602028 |
| QE, high-psi branch | 0.01 | 0.5 | 0.01 | 0.2 | 1 | 96.55317400157244 | 0.018076059597846472 |

Rust, C, Go and Python compare spot at absolute tolerance `1e-11` and variance
at `1e-13`. Same-build full/terminal values must match exactly. Separate seeded
Monte Carlo tests check analytic variance moments and the QEM discounted-spot
expectation with statistical tolerances.

::: itofin.simulate_heston

## Merton jump paths

`itofin.simulate_merton` and Go's `SimulateMerton` generate scalar,
constant-parameter lognormal jump-diffusion paths through the shared Rust
kernel. Each grid transition is exact for this model. Outputs contain grid
spots; individual jump times and sizes within an interval are not returned.
The [Merton pricing process](jump-diffusion.md) remains a live market-input
carrier: simulation takes explicit scalar assumptions rather than reading
or changing its quotes and curves.

```python
import itofin

parameters = dict(
    spot=100.0, drift=0.05, volatility=0.2,
    jump_intensity=1.0, log_mean_jump=-0.1, log_jump_volatility=0.3,
    horizon=1.0, steps=12, paths=100, seed=42,
)
full = itofin.simulate_merton(**parameters)
terminal = itofin.simulate_merton(**parameters, terminal_only=True)
assert full.shape == (100, 13)
assert terminal.shape == (100,)
assert (full[:, 0] == 100.0).all()
assert (terminal == full[:, -1]).all()
```

```go
config := itofin.MertonConfig{
    Spot: 100, Drift: 0.05, Volatility: 0.2,
    JumpIntensity: 1, LogMeanJump: -0.1, LogJumpVolatility: 0.3,
    Horizon: 1, Steps: 12, Paths: 100, Seed: 42,
}
full, err := itofin.SimulateMerton(config)
if err != nil {
    return err
}
config.TerminalOnly = true
terminal, err := itofin.SimulateMerton(config)
if err != nil {
    return err
}
fmt.Println(full.Paths, full.Times, full.Assets)
fmt.Println(terminal.Values[0] == full.Values[12])
```

### Drift and jump conventions

All rates use matching annual time units. `drift` means total expected
arithmetic spot growth, including jumps. Supply `r-q` for constant-rate
risk-neutral simulation; for a real-world forecast, supply your own drift
assumption. Option calibration does not determine a real-world forecast drift.
`jump_intensity` is the original Poisson event intensity, rather than the
asset-weighted intensity used inside the European pricing series.
`log_mean_jump` and `log_jump_volatility` are the mean and standard deviation
of a jump's logarithmic multiplier.

With `dt=horizon/steps`, diffusion normal `Zd`, independent jump normal `Zj`
and `N ~ Poisson(jump_intensity*dt)`, the transition is:

```text
kappa = expm1(log_mean_jump + log_jump_volatility^2/2)
S_next = S * exp((drift - jump_intensity*kappa - volatility^2/2)*dt
                + volatility*sqrt(dt)*Zd
                + N*log_mean_jump + sqrt(N)*log_jump_volatility*Zj)
```

The kernel subtracts the compensator internally, giving
`E[S(T)] = spot*exp(drift*T)`. Do not subtract it again in the input drift.

### Seed, layout and limits

Three MT19937 streams use the nonzero 32-bit input seed for diffusion and
`1 + ((seed-1+offset) % 4294967295)` for counts and aggregate jumps, with
respective offsets `0x9E3779B9` and `0xBB67AE85`. Uniforms use
`(word+0.5)/2^32`; normals use the existing Acklam inverse without refinement.
Every step consumes one diffusion draw, one count draw and one jump draw,
including zero horizon, zero volatility and zero count. Streams advance in
path then time order and continue across paths.

Python returns an owned, C-contiguous NumPy `float64` array. Go returns owned
`Simulation.Values` in row-major `[path,time]` order, with `Assets=1`.
Full output includes the initial spot; terminal output matches each last full
value bit for bit. Zero jump intensity matches scalar GBM with the same seed,
including its zero-volatility deterministic convention. Zero horizon returns
the initial spot. Calls do not require a session and can run independently.

Spot must be finite and positive. Drift and log-jump mean must be finite;
volatilities, intensity and horizon must be finite and nonnegative. Steps,
paths and seed must be positive. Invalid or overflowing dimensions,
unrepresentable coefficients, positive time-step/count-mean underflow and
nonfinite or nonpositive simulated spots return errors.
The existing inverse-Poisson recurrence requires its initial mass to stay
normal, restricting the mean per step to approximately 708.396. Use more
steps when needed; no large-mean approximation is substituted.

Both bindings default to 16,777,216 output values (128 MiB). Zero
`max_output_values`/`MaxOutputValues` selects that default; a positive value
overrides it and a negative value fails. The limit bounds returned values,
not total peak memory. Terminal mode or smaller batches reduce output size.
C accepts caller-owned capacity through `itofin_merton_paths` and preserves
its output buffer on every error, including failures after earlier steps.

The [independent fixture notes](https://github.com/benbenbang/libitofin/blob/main/sdk/go/testdata/merton-paths-oracle.md)
include a dependency-free generator, seed/draw records, twelve numerical
cases and analytical moment checks. Rust, C, Go and Python share these gates.

::: itofin.simulate_merton
