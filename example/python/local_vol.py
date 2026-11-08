"""Price a European call by Monte Carlo under a Dupire local volatility surface.

The local volatility is derived from a skewed Black surface over a shared spot
quote, so bumping the quote reprices the option.

Run it with:

    python example/python/local_vol.py
"""

# standard library
import math

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import MCEuropeanEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackVarianceSurface, FlatForward, LocalVolSurface
from itofin.time import Date, DayCounter

today = Date(15, 6, 2026)
dc = DayCounter.actual360()
settings = Settings()
settings.set_evaluation_date(today)
spot = SimpleQuote(100.0)
risk_free = FlatForward(today, 0.05, dc)
dividend = FlatForward(today, 0.02, dc)

strikes = [100.0 * math.exp(0.2 * k) for k in range(-7, 8)]
vols = [[0.2 - 0.1 * math.log(k / 100) + 0.05 * math.log(k / 100) ** 2] * 3 for k in strikes]
black = BlackVarianceSurface(today, [today + 60, today + 120, today + 240], strikes, vols, dc)
local = LocalVolSurface(black, risk_free, dividend, spot)
process = BlackScholesProcess.with_local_vol(spot, risk_free, dividend, black, local)

option = VanillaOption(OptionType.Call, 115.0, today + 180, settings)
option.set_mc_engine(MCEuropeanEngine(process, steps_per_year=12, samples=20000, seed=42))
print(f"local vol at t=0.25, S=110: {local.local_vol(0.25, 110.0):.6f}")
print(f"MC call K=115: {option.npv():.6f} +/- {option.error_estimate():.6f}")
spot.set_value(105.0)
print(f"after spot bump to 105: {option.npv():.6f}")
