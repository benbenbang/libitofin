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
Quanto and local-volatility settings are outside this API.

## Cash dividends

Pass `dividend_dates` and `dividend_amounts` together to pay fixed cash
dividends under the Spot model. Dividends outside the option life are dropped,
and a dividend on the evaluation date leaves `option.theta()` unavailable. The
Escrowed cash-dividend model is not offered; it is tracked on #828.

```python
dividends = FdBlackScholesVanillaEngine(
    process, 100, 400,
    dividend_dates=[Date(15, 7, 2025)], dividend_amounts=[2.0],
)
american_value = option.price_fd(dividends)

from itofin.pricingengines import AnalyticDividendEuropeanEngine

analytic = AnalyticDividendEuropeanEngine(process, [Date(15, 7, 2025)], [2.0])
european = VanillaOption(OptionType.Put, 100.0, expiry, settings)
european_value = european.price_analytic_dividend(analytic)
```

`AnalyticDividendEuropeanEngine` reduces the spot by the present value of the
dividends and raises `ItofinError` for a non-European exercise. Go exposes
`Session.NewFdBlackScholesVanillaEngineWithCashDividends` and
`Session.NewAnalyticDividendEuropeanEngine`; C exposes
`itofin_fd_black_scholes_engine_with_cash_dividends` and
`itofin_analytic_dividend_european_engine_new`, both taking serial dates.

For the equivalent Go constructor and configuration, see the [Go SDK FD example](go.md#finite-difference-vanilla-options).
