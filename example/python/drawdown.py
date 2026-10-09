"""Measure the maximum running-peak loss in an ordered positive NAV path."""

# itofin library
from itofin.statistics import maximum_drawdown

result = maximum_drawdown([100.0, 120.0, 90.0, 130.0])
assert (result.drawdown, result.peak_index, result.trough_index) == (0.25, 1, 2)
print(f"maximum drawdown {result.drawdown:.0%}, peak {result.peak_index}, trough {result.trough_index}")
