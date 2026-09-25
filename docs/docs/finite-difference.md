# Finite-difference vanilla options

The Black-Scholes finite-difference engine prices European, American and Bermudan
vanilla options. It reports NPV, delta, gamma and theta on the same option object.

```python
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

today = Date(15, 1, 2025)
expiry = Date(15, 1, 2026)
settings = Settings()
settings.set_evaluation_date(today)
process = BlackScholesProcess(
    80.0, 0.05, 0.0, 0.25, today, DayCounter.actual365_fixed()
)
option = VanillaOption.american(OptionType.Put, 100.0, today, expiry, settings)
engine = FdBlackScholesVanillaEngine(process, t_grid=200, x_grid=200)
value = option.price_fd(engine)
print(value, option.delta(), option.gamma(), option.theta())
```

The example returns approximately `20.357667204554883` for the American put.
`option.set_fd_engine(engine)` also attaches the engine for later `option.npv()`
calls. The option retains its engine, and the engine retains the process.

The default grid is 100 time steps by 100 spatial nodes, with zero damping
steps and `FdScheme.Douglas`. `FdScheme.ImplicitEuler` is the other supported
rollback scheme. Time steps must be positive and spatial nodes at least three.
Cash dividends, quanto and local-volatility settings are outside this API.

For the equivalent Go constructor and configuration, see the [Go SDK FD example](go.md#finite-difference-vanilla-options).
