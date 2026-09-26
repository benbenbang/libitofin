# Optimize

SciPy-style `minimize` over the finance-independent `itofin-optimize` crate.
Python, Go, C and Rust expose Nelder-Mead, BFGS, L-BFGS-B, and SLSQP.

This is distinct from [Optimization](optimization.md), the QuantLib
calibration port. The objective runs outside any bootstrap callback, so it may
set a `SimpleQuote` and reprice an instrument. An exception raised by `fun`,
`jac`, a constraint callback, or `callback` is re-raised as the same object;
`StopIteration` from `callback` stops the run with `Status.Cancelled`.

```python
from itofin.optimize import minimize

result = minimize(lambda x: (x[0] - 3.0) ** 2, [0.0], options={"xatol": 1e-8})
print(result.x, result.status, result.message)
```

| Method | Supported options | Gradient | Bounds | General constraints |
| --- | --- | --- | --- | --- |
| Nelder-Mead | `maxiter`, `maxfev`, `xatol`, `fatol`, `adaptive` | None | Rejected | Rejected |
| BFGS | `maxiter`, `gtol`, `eps` | `jac(x)` or finite differences | Rejected | Rejected |
| L-BFGS-B | `maxiter`, `maxfev`, `maxcor`, `ftol`, `gtol`, `eps` | `jac(x)` or bounded finite differences | `(lower, upper)` pairs; `None` opens a side | Rejected |
| SLSQP | `maxiter`, `maxfev`, `ftol` | `jac(x)` or finite differences | `(lower, upper)` pairs; `None` opens a side | `constraints` dictionaries |

`eps` is an absolute finite-difference step; it has no effect when `jac` is
provided. An unknown method, invalid numeric input, or unsupported Nelder-Mead
option raises `ItofinError`. Unsupported BFGS options, `jac` with Nelder-Mead,
and bounds on Nelder-Mead or BFGS raise `ValueError`. L-BFGS-B and SLSQP reject
an incorrect number of bound pairs before evaluating the objective. Only SLSQP
accepts `constraints`; unsupported options and malformed constraint descriptors
are rejected before the objective runs. Callback output dimensions are checked
on every evaluation.

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

SLSQP interprets equality constraints as `fun(x) == 0` and inequality
constraints as `fun(x) >= 0`. Each `fun` returns a scalar or one-dimensional
vector. Its optional `jac` returns a gradient for a scalar constraint or one
gradient row per vector component. A missing Jacobian uses finite differences.
The dimension is fixed at the initial point; `Status.Infeasible` (integer 8)
means the constraints could not be satisfied.

```python
result = minimize(
    lambda x: (x[0] - 3.0) ** 2 + (x[1] + 1.0) ** 2,
    [0.2, 0.3],
    method="SLSQP",
    bounds=[(0.0, 1.0), (0.0, 1.0)],
    constraints=[{
        "type": "ineq",
        "fun": lambda x: x[0] + x[1] - 2.0,
        "jac": lambda x: [1.0, 1.0],
    }],
    options={"ftol": 1e-10},
)
```

The integer values of `Status` match the C `ItofinOptimizeStatus` and the Go
`OptimizeStatus`.

::: itofin.optimize
