"""Price a European option and read its greeks.

This is the "hello world" of itofin: build a Black-Scholes market, wrap it in a
`VanillaOption`, attach the analytic engine and read the value plus every greek.

The numbers below reproduce the pricing gate in
`crates/itofin-py/tests/test_european_option.py` (row 1), so the printed NPV of
2.1333684449161985 is a self-check: if it drifts, the market was wired wrong.

Run it with:

    python example/python/european_option.py
"""

# plugins
# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def main() -> None:
    # D5: one Settings object holds the evaluation date and is threaded into
    # every instrument. There is no global "today"; an unset date is an error,
    # not a silent fall back to the system clock.
    settings = Settings()
    today = Date(15, 6, 2026)
    settings.set_evaluation_date(today)

    # A generalized Black-Scholes process from scalar market data:
    #   spot 60, risk-free 8%, dividend yield 0%, volatility 30%,
    # all quoted on an Actual/360 day count as of `today`.
    day_counter = DayCounter.actual360()
    process = BlackScholesProcess(
        60.0,  # spot
        0.08,  # risk-free rate
        0.0,  # dividend yield
        0.30,  # volatility
        today,  # reference date
        day_counter,
    )

    # A European call struck at 65, expiring 90 calendar days out.
    # `Date + int` advances by days (not by a Period).
    option = VanillaOption(OptionType.Call, 65.0, today + 90, settings)

    # Attaching the process installs the analytic European engine.
    option.set_engine(process)

    # NPV plus the full greek set. Each read reprices lazily off the process.
    print("European call, K=65, 90d, spot=60, vol=30%, r=8%")
    print(f"  NPV          = {option.npv():.10f}")
    print(f"  delta        = {option.delta():.10f}")
    print(f"  gamma        = {option.gamma():.10f}")
    print(f"  theta        = {option.theta():.10f}")
    print(f"  vega         = {option.vega():.10f}")
    print(f"  rho          = {option.rho():.10f}")
    print(f"  dividend_rho = {option.dividend_rho():.10f}")


if __name__ == "__main__":
    main()
