# itofin

Idiomatic Python bindings over the [`libitofin`](https://github.com/benbenbang/libitofin) Rust core, a ground-up port of [QuantLib](https://github.com/lballabio/QuantLib) for pricing, risk, and calibration. The heavy numerics run in Rust; Python drives them. The pricing snippets below reproduce the cached Rust test oracles exactly; the abbreviated calibration snippets converge to the same targets within the tests' calibration tolerance (the full matrices live in `tests/`).

## Install

```bash
pip install itofin
```

Requires Python >= 3.13.

## Price a European option and its greeks

```python
import itofin

s = itofin.Settings()
s.set_evaluation_date(itofin.Date(15, 6, 2026))
dc = itofin.DayCounter.actual360()

process = itofin.BlackScholesProcess(60.0, 0.08, 0.0, 0.30, itofin.Date(15, 6, 2026), dc)
option = itofin.VanillaOption(itofin.OptionType.Call, 65.0, itofin.Date(15, 6, 2026) + 90, s)
option.set_engine(process)

print(f"NPV   {option.npv():.10f}")
print(f"delta {option.delta():.10f}")
print(f"vega  {option.vega():.10f}")
print(f"rho   {option.rho():.10f}")
```

```
NPV   2.1333684449
delta 0.3724827980
vega  11.3515440535
rho   5.0538998582
```

## Price under Heston and calibrate to a flat-vol surface

Semi-analytic Heston pricing:

```python
import itofin

s = itofin.Settings()
s.set_evaluation_date(itofin.Date(27, 12, 2004))
dc = itofin.DayCounter.actual_actual_isda()

# HestonProcess(risk_free, dividend, spot, v0, kappa, theta, sigma, rho, reference_date, day_counter)
process = itofin.HestonProcess(
    0.0225, 0.02, 1.0, 0.1, 3.16, 0.09, 0.4, -0.2, itofin.Date(27, 12, 2004), dc
)
model = itofin.HestonModel(process)
option = itofin.VanillaOption(itofin.OptionType.Call, 1.05, itofin.Date(28, 3, 2005), s)
option.set_heston_engine(model, 64)

print(f"Heston NPV {option.npv():.10f}")
```

```
Heston NPV 0.0404774515
```

Calibrating the model to a flat 10% vol surface drives the vol-of-vol `sigma` toward zero and both `v0` and `theta` toward the flat variance (0.10 squared = 0.01). The snippet below fits a 3-maturity by 3-strike grid; the full 21-helper matrix and its tighter tolerances live in `tests/test_heston_calibration.py`.

```python
import math
import itofin

s = itofin.Settings()
s.set_evaluation_date(itofin.Date(15, 1, 2026))
dc = itofin.DayCounter.actual360()
ref = itofin.Date(15, 1, 2026)
risk_free = itofin.FlatForward(ref, 0.04, dc)
dividend = itofin.FlatForward(ref, 0.50, dc)
VOL = 0.10

helpers = []
for n in (1, 2, 3):
    tau = float(n)
    forward = dividend.discount(tau) / risk_free.discount(tau)  # spot = 1.0
    for moneyness in (-1.0, 0.0, 1.0):
        strike = forward * math.exp(-moneyness * VOL * math.sqrt(tau))
        helpers.append(itofin.HestonModelHelper(
            itofin.Period(n, "Years"),
            itofin.Calendar.null_calendar(),
            1.0,        # spot
            strike,
            VOL,        # 10% flat vol
            0.04,       # risk-free
            0.50,       # dividend yield
            itofin.CalibrationErrorType.RelativePriceError,
            ref,
            dc,
            s,
        ))

process = itofin.HestonProcess(0.04, 0.50, 1.0, 0.01, 0.2, 0.02, 0.5, -0.75, ref, dc)
model = itofin.HestonModel(process)
method = itofin.LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
end_criteria = itofin.EndCriteria(400, 40, 1e-8, 1e-8, 1e-8)
model.calibrate(helpers, method, end_criteria, 96)

print(f"sigma {model.sigma():.6f}")
print(f"v0    {model.v0():.6f}")
print(f"theta {model.theta():.6f}")
```

```
sigma 0.000216
v0    0.010000
theta 0.010000
```

## Calibrate Hull-White to a swaption matrix

```python
import itofin

s = itofin.Settings()
s.set_evaluation_date(itofin.Date(15, 2, 2002))
curve = itofin.FlatForward(itofin.Date(19, 2, 2002), 0.04875825, itofin.DayCounter.actual365_fixed())

model = itofin.HullWhite(curve, 0.1, 0.01)
index = itofin.Euribor.six_months(curve, s)

# (option tenor, swap tenor, Black vol); the full co-terminal matrix is in the tests.
swaptions = [(1, 5, 0.1148), (2, 4, 0.1108), (3, 3, 0.1070), (4, 2, 0.1021), (5, 1, 0.1000)]
helpers = [
    itofin.SwaptionHelper(
        itofin.Period(maturity, "Years"),
        itofin.Period(length, "Years"),
        vol,
        index,
        itofin.Period(1, "Years"),
        itofin.DayCounter.thirty360_bond_basis(),
        itofin.DayCounter.actual360(),
        curve,
        itofin.CalibrationErrorType.RelativePriceError,
        1.0,
    )
    for maturity, length, vol in swaptions
]

method = itofin.LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
end_criteria = itofin.EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8)
model.calibrate(helpers, method, end_criteria, False)  # fix_reversion=False, fit a and sigma

print(f"a     {model.a():.7f}")
print(f"sigma {model.sigma():.7f}")
```

```
a     0.0464113
sigma 0.0057992
```

## Price a European swaption with the Jamshidian engine

```python
import itofin

s = itofin.Settings()
s.set_evaluation_date(itofin.Date(15, 1, 2026))
curve = itofin.FlatForward(itofin.Date(15, 1, 2026), 0.03, itofin.DayCounter.actual365_fixed())

fixed = itofin.Schedule(
    itofin.Date(15, 1, 2028), itofin.Date(15, 1, 2033),
    itofin.Frequency.Annual, itofin.Calendar.target(),
    itofin.BusinessDayConvention.Unadjusted,
)
floating = itofin.Schedule(
    itofin.Date(15, 1, 2028), itofin.Date(15, 1, 2033),
    itofin.Frequency.Semiannual, itofin.Calendar.target(),
    itofin.BusinessDayConvention.Unadjusted,
)
index = itofin.Euribor.six_months(curve, s)

swap = itofin.VanillaSwap(
    itofin.SwapType.Payer, 100.0,
    fixed, 0.03, itofin.DayCounter.thirty360_bond_basis(),
    floating, index, 0.0, itofin.DayCounter.actual360(), s,
)
swaption = itofin.Swaption(
    swap, itofin.EuropeanExercise(itofin.Date(15, 1, 2027)),
    itofin.SettlementType.Physical, itofin.SettlementMethod.PhysicalOTC, s,
)
swaption.set_jamshidian_engine(itofin.HullWhite(curve, 0.05, 0.01))

print(f"payer swaption NPV {swaption.npv():.10f}")
```

```
payer swaption NPV 1.5666103956
```

## License

BSD-3-Clause, matching the `libitofin` core. See the [repository](https://github.com/benbenbang/libitofin) for the layer status table, the divergences-from-QuantLib catalogue, and the Rust test oracles behind the numbers above.
