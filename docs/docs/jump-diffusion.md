# Merton jump-diffusion pricing

`Merton76Process` combines a live spot quote, risk-free and dividend curves,
Black volatility, and three observable jump quotes. `JumpDiffusionEngine`
prices European plain-vanilla calls and puts and reports delta, gamma, theta,
vega, rho and dividend rho on the option.

Jump intensity is the expected number of jumps per year. `log_mean_jump` and
`log_jump_volatility` are the mean and standard deviation of the log jump
multiplier. The mean proportional jump is
`exp(log_mean_jump + log_jump_volatility**2 / 2) - 1`; the model compensates the
drift by subtracting intensity times that mean jump. Diffusion volatility
excludes the jump contribution.

## Python

```python
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import JumpDiffusionEngine
from itofin.processes import Merton76Process
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter

today = Date(15, 1, 2025)
expiry = Date(15, 1, 2026)
dc = DayCounter.actual365_fixed()
settings = Settings()
settings.set_evaluation_date(today)
spot = SimpleQuote(100.0)
rate = SimpleQuote(0.05)
dividend = SimpleQuote(0.02)
diffusion_vol = SimpleQuote(0.20)
intensity = SimpleQuote(0.75)
log_mean = SimpleQuote(-0.10)
log_vol = SimpleQuote(0.25)
process = Merton76Process(
    spot,
    FlatForward.from_quote(today, rate, dc),
    FlatForward.from_quote(today, dividend, dc),
    BlackConstantVol.from_quote(today, diffusion_vol, dc),
    intensity, log_mean, log_vol,
)
engine = JumpDiffusionEngine(process, relative_accuracy=1e-10, max_iterations=1000)
option = VanillaOption(OptionType.Call, 100.0, expiry, settings)
option.set_jump_diffusion_engine(engine)
print(option.npv(), option.delta(), option.vega())
spot.set_value(105.0)
print(option.npv())
```

The process retains its market inputs, the engine retains the process, and the
option retains its engine. Quote updates invalidate the cached price; a later
`npv()` recomputes it. `spot()`, `jump_intensity()`, `log_mean_jump()`,
`log_jump_volatility()` and `time(date)` inspect the process.

## Go

Use `session.NewMerton76Process(spot, riskFree, dividend, vol, intensity,
logMean, logVol)` with quotes and curves from the same session.
`session.NewBlackConstantVolFromQuote(today, diffusionVol, dc, nil)` supplies
observable diffusion volatility. Then attach the engine:

```go
engine, err := session.NewJumpDiffusionEngine(process, 1e-10, 1000)
if err != nil {
    return err
}
if err = option.SetJumpDiffusionEngine(engine); err != nil {
    return err
}
value, err := option.NPV()
if err != nil {
    return err
}
fmt.Println(value)
```

The process exposes `Spot`, `JumpIntensity`, `LogMeanJump`,
`LogJumpVolatility` and `Time`. Close Go handles when finished; dependents
retain the underlying inputs. Closing a session ends access to its handles.

## Pricing limits

Intensity and log jump volatility must be finite and nonnegative; log jump
mean must be finite. Spot must be finite and positive. These conditions are
checked again after market updates. Zero intensity recovers Black-Scholes.

Python defaults are `relative_accuracy=1e-4` and `max_iterations=100`;
Go and C accept these controls explicitly. The accuracy must be finite and
positive, and the iteration limit is a hard budget between 1 and 100,000.
Insufficient budget or
failure to converge raises an error instead of returning a truncated price.
American/Bermudan exercise and non-plain-vanilla payoffs are rejected.

The series checks both the latest contribution and a bound on the remaining
payoff tail. This avoids stopping at zero early terms when later jumps can
produce a payoff. Zero diffusion volatility is handled without NaNs.
These behaviors intentionally correct upstream edge cases. Independent
[QuantLib fixture notes](https://github.com/benbenbang/libitofin/blob/main/sdk/go/testdata/merton76-oracle.md)
record the source, curve clocks and tolerances.

This process provides pricing inputs and date conversion. Like QuantLib's
`Merton76Process`, it does not provide drift, diffusion or apply operations
for path generation. The separate [seeded Merton kernel](simulation.md#merton-jump-paths)
generates constant-parameter paths from explicit scalar assumptions.
