# Correlated GBM simulation

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
