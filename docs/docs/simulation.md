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
