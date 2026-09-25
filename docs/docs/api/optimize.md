# Optimize

SciPy-style `minimize` over the finance-independent `itofin-optimize` crate.
Nelder-Mead is the first solver; BFGS, L-BFGS-B and SLSQP follow.

This is distinct from [Optimization](optimization.md), the QuantLib
calibration port. The objective runs outside any bootstrap callback, so it may
set a `SimpleQuote` and reprice an instrument. An exception raised by `fun` or
`callback` is re-raised as the same object; `StopIteration` from `callback`
stops the run with `Status.Cancelled`.

```python
from itofin.optimize import minimize

result = minimize(lambda x: (x[0] - 3.0) ** 2, [0.0], options={"xatol": 1e-8})
print(result.x, result.status, result.message)
```

The integer values of `Status` match the C `ItofinOptimizeStatus` and the Go
`OptimizeStatus`.

::: itofin.optimize
