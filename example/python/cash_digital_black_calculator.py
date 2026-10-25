"""Standalone cash-digital Black value and Greeks, without an instrument graph."""
# itofin library
from itofin.instruments import OptionType
from itofin.pricingengines import BlackCalculator

calculator = BlackCalculator.cash_digital(OptionType.Call, 100., 7., 110., .3, .94)
print(f"value: {calculator.value():.8f}")
print(f"forward delta: {calculator.delta_forward():.8f}")
print(f"vega (two-year expiry): {calculator.vega(2.):.8f}")
