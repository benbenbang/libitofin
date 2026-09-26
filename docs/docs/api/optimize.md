# Optimize

SciPy-style `minimize` over the finance-independent `itofin-optimize` crate.
Python exposes Nelder-Mead and BFGS. The Rust core also implements SLSQP;
L-BFGS-B remains planned.

This is distinct from [Optimization](optimization.md), the QuantLib
calibration port. The objective runs outside any bootstrap callback, so it may
set a `SimpleQuote` and reprice an instrument. An exception raised by `fun`,
`jac`, or `callback` is re-raised as the same object; `StopIteration` from `callback`
stops the run with `Status.Cancelled`.

```python
from itofin.optimize import minimize

result = minimize(lambda x: (x[0] - 3.0) ** 2, [0.0], options={"xatol": 1e-8})
print(result.x, result.status, result.message)
```

| Method | Supported options | Gradient | Bounds |
| --- | --- | --- | --- |
| Nelder-Mead | `maxiter`, `maxfev`, `xatol`, `fatol`, `adaptive` | None | Rejected |
| BFGS | `maxiter`, `gtol`, `eps` | `jac(x)` or finite differences | Rejected |

`eps` is an absolute finite-difference step; it has no effect when `jac` is
provided. An unknown method, invalid numeric input, or unsupported Nelder-Mead
option raises `ItofinError`. Unsupported BFGS options, `jac` with Nelder-Mead,
and bounds raise `ValueError`.

```python
result = minimize(
    lambda x: (x[0] - 3.0) ** 2,
    [0.0],
    method="BFGS",
    jac=lambda x: [2.0 * (x[0] - 3.0)],
    options={"gtol": 1e-8},
)
```

The integer values of `Status` match the C `ItofinOptimizeStatus` and the Go
`OptimizeStatus`.

::: itofin.optimize
