# Optimize

SciPy-style `minimize` over the finance-independent `itofin-optimize` crate.
Python exposes Nelder-Mead, BFGS, and L-BFGS-B. The Rust core also implements
SLSQP.

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
| L-BFGS-B | `maxiter`, `maxfev`, `maxcor`, `ftol`, `gtol`, `eps` | `jac(x)` or bounded finite differences | `(lower, upper)` pairs; `None` opens a side |

`eps` is an absolute finite-difference step; it has no effect when `jac` is
provided. An unknown method, invalid numeric input, or unsupported Nelder-Mead
option raises `ItofinError`. Unsupported BFGS options, `jac` with Nelder-Mead,
and bounds on Nelder-Mead or BFGS raise `ValueError`. L-BFGS-B rejects an
incorrect number of bound pairs before evaluating the objective.

```python
result = minimize(
    lambda x: (x[0] - 3.0) ** 2,
    [0.0],
    method="BFGS",
    jac=lambda x: [2.0 * (x[0] - 3.0)],
    options={"gtol": 1e-8},
)
```

```python
result = minimize(
    lambda x: (x[0] - 3.0) ** 2 + (x[1] + 2.0) ** 2,
    [0.0, 0.0],
    method="L-BFGS-B",
    bounds=[(0.0, 1.0), (None, None)],
)
```

The integer values of `Status` match the C `ItofinOptimizeStatus` and the Go
`OptimizeStatus`.

::: itofin.optimize
