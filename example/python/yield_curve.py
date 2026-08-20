"""Bootstrap a yield curve from deposits and swaps, then query it.

`PiecewiseYieldCurve` takes a bundle of rate helpers (each pinning one market
quote) and solves for the discount factors that reprice all of them at once.
The curve is lazy: nothing is solved until the first query.

The deposit and swap quotes are transcribed from QuantLib's
`piecewiseyieldcurve.cpp`. The pedagogical payoff and the self-check are the
same thing: a FRESH Euribor index laid on the bootstrapped curve must forecast
back each deposit's own input rate. If it does, the bootstrap is consistent.

Run it with:

    python example/python/yield_curve.py
"""

# plugins
# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, PiecewiseYieldCurve, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency
from itofin.time import Period as P

# (n, unit, rate-in-percent) short-end deposits.
DEPOSIT_DATA = [
    (1, "Weeks", 4.559),
    (1, "Months", 4.581),
    (3, "Months", 4.557),
    (6, "Months", 4.496),
    (9, "Months", 4.490),
]

# (years, rate-in-percent) par swap rates for the long end.
SWAP_DATA = [
    (1, 4.54),
    (2, 4.63),
    (3, 4.75),
    (5, 4.99),
    (10, 5.47),
    (20, 5.89),
    (30, 5.96),
]


def main() -> None:
    settings = Settings()
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), BusinessDayConvention.Following)
    settings.set_evaluation_date(today)

    # Settlement is spot: today advanced two business days.
    settlement = calendar.advance(today, 2, "Days", BusinessDayConvention.Following, False)

    # Deposit helpers: each quotes a Euribor rate over an (as yet unlinked)
    # forwarding index. The SimpleQuote holds the market rate.
    deposits = []
    for n, unit, rate in DEPOSIT_DATA:
        index = Euribor(P(n, unit), None, settings)
        deposits.append(DepositRateHelper(SimpleQuote(rate / 100.0), index))

    # Swap helpers: a par swap rate, an annual fixed leg on 30/360 bond basis,
    # floating off 6M Euribor.
    swaps = []
    for n, rate in SWAP_DATA:
        euribor6m = Euribor(P(6, "Months"), None, settings)
        swaps.append(
            SwapRateHelper(
                SimpleQuote(rate / 100.0),
                P(n, "Years"),
                calendar,
                Frequency.Annual,
                BusinessDayConvention.Unadjusted,
                DayCounter.thirty360_bond_basis(),
                euribor6m,
            )
        )

    # Log-linear-on-discount-factors interpolation (the market default).
    curve = PiecewiseYieldCurve(settlement, deposits + swaps, DayCounter.actual360(), "LogLinear")

    # First query forces the (lazy) bootstrap. Query strictly inside the curve
    # span; t=0 and points past the last pillar are range-rejected.
    print("Discount factors and zero rates off the bootstrapped curve:")
    for t in [0.5, 1.0, 2.0, 5.0, 10.0, 20.0]:
        df = curve.discount(t)
        zero = curve.zero_rate(t)
        print(f"  t={t:5.1f}y   discount={df:.6f}   zero={zero * 100:6.3f}%")

    # Consistency check: a fresh index on the solved curve reforecasts each
    # deposit's own input rate. This is the bootstrap contract made visible.
    print("\nDeposit reprice check (fresh index on the curve vs input quote):")
    for n, unit, rate in DEPOSIT_DATA:
        index = Euribor(P(n, unit), curve, settings)
        forecast = index.fixing(today, False)
        print(
            f"  {n:2d} {unit:<7} forecast={forecast * 100:6.3f}%   "
            f"input={rate:6.3f}%   |diff|={abs(forecast - rate / 100.0):.2e}"
        )


if __name__ == "__main__":
    main()
