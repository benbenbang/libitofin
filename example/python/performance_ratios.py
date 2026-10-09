"""Explicit monthly targets and square-root frequency scaling, not compounding."""

# standard library
import math

# itofin library
from itofin import statistics

returns = [0.02, -0.01, 0.03, -0.02]
downside = statistics.target_downside_deviation(returns, 0.0)
sharpe = statistics.sharpe_ratio(returns, 0.0, periods_per_year=12.0)
sortino = statistics.sortino_ratio(returns, 0.0, periods_per_year=12.0)
assert math.isclose(downside, math.sqrt(0.0005 / 4), abs_tol=1e-14)
assert math.isclose(sortino, math.sqrt(12 / 5), abs_tol=1e-13)
print(f"Monthly downside: {downside:.8f}; annualized Sharpe: {sharpe:.8f}; Sortino: {sortino:.8f}")
