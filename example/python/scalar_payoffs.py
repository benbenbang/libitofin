"""Signed gap payout and a dimensionless percentage-strike scalar payoff."""

# itofin library
from itofin.instruments import OptionType, ScalarPayoff

payoff = ScalarPayoff.gap(OptionType.Call, 100, 110)
print(f"trigger={payoff.strike():.2f} payment={payoff.second_strike():.2f} value={payoff.value(100):.2f}")
ratio = ScalarPayoff.percentage_strike(OptionType.Call, 0.8)
print(f"moneyness={ratio.strike():.2f} payoff at -100={ratio.value(-100):.2f}")
