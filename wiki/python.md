# Python guide

[README](../README.md) | Python | [Rust](rust.md) | [Go](go.md)

The same engine is reachable from Python via [`itofin`](https://pypi.org/project/itofin/)
(Python 3.10+: 3.13 is the primary tested target, and 3.10-3.12 are supported via
the abi3 wheel). The API mirrors QuantLib's `ql/` layout: types live in submodules
(`itofin.time`, `itofin.instruments`, `itofin.processes`, ...), while `Settings`
and `ItofinError` stay at the top level. Real market data is first-class - yield
curves (bootstrapped from a deposit/swap strip via `PiecewiseYieldCurve`, or
interpolated), Black-vol surfaces, swaption vol cubes (matrix, interpolated, or
SABR-calibrated), and cap/floor optionlet-vol stripping - so you price against a
market snapshot, not just flat inputs. Type stubs ship in the wheel, so editors
and `mypy` see the full API.

For installation, see the [repository index](../README.md#install).

## Price a European option

```python
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

s = Settings()
s.set_evaluation_date(Date(15, 6, 2026))
dc = DayCounter.actual360()

process = BlackScholesProcess(60.0, 0.08, 0.0, 0.30, Date(15, 6, 2026), dc)
option = VanillaOption(OptionType.Call, 65.0, Date(15, 6, 2026) + 90, s)
option.set_engine(process)

print(f"NPV   {option.npv():.10f}")   # 2.1333684449
print(f"delta {option.delta():.10f}")  # 0.3724827980
```

## Black-vol surface

Build a volatility surface (strike x expiry) and query it:

```python
from itofin.termstructures import BlackVarianceSurface
from itofin.time import Date, DayCounter

ref = Date(15, 6, 2026)
vols = BlackVarianceSurface(
    ref,
    [ref + 365, ref + 730],   # expiries
    [90.0, 100.0, 110.0],     # strikes
    [[0.20, 0.25],            # one row per strike,
     [0.18, 0.22],            # one column per expiry
     [0.16, 0.20]],
    DayCounter.actual365_fixed(),
)
print(f"{vols.black_vol(1.0, 100.0):.2f}")  # 0.18
```

## Swaption-vol surface

Query an ATM swaption volatility matrix; `InterpolatedSwaptionVolatilityCube`
and the SABR-calibrated `SabrSwaptionVolatilityCube` add the strike dimension, and
`pricingengines.BlackSwaptionEngine` (via `Swaption.set_black_engine`) prices a
swaption straight off any of them:

```python
from itofin import Settings
from itofin.termstructures import SwaptionVolatilityMatrix, VolatilityType
from itofin.time import Date, Period, Calendar, DayCounter, BusinessDayConvention

s = Settings()
s.set_evaluation_date(Date(15, 6, 2026))
opt = [Period(1, "Years"), Period(5, "Years")]   # option tenors (rows)
swp = [Period(1, "Years"), Period(5, "Years")]   # swap tenors (columns)
vols = [[0.20, 0.18],
        [0.17, 0.16]]

svol = SwaptionVolatilityMatrix(
    Date(15, 6, 2026), Calendar.target(), BusinessDayConvention.Following,
    opt, swp, vols, DayCounter.actual365_fixed(), VolatilityType.ShiftedLognormal,
)
print(f"{svol.volatility(Period(1, 'Years'), Period(5, 'Years'), 0.03):.4f}")  # 0.1800 (node)
print(f"{svol.volatility(Period(3, 'Years'), Period(3, 'Years'), 0.03):.4f}")  # 0.1775 (bilinear)
```

## More examples and reference

- [Swaption example](../example/python/swaption.py) (requires current `main`): price vanilla payer and Eonia OIS swaptions, then reprice after a volatility update.

- [Getting started](https://benbenbang.github.io/libitofin/getting-started/): runnable Python, Rust, and Go examples.
- [Python example sources](../example/python/): yield curves, swaps, Monte Carlo, credit, and inflation.
- [Python API reference](https://benbenbang.github.io/libitofin/api/core/): module-by-module signatures and documentation.
- [Python bindings source](../crates/itofin-py/): implementation and tests.
