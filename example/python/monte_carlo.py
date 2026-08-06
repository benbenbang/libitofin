"""Price options by Monte Carlo: European, Heston, and American.

Three different MC engines, attached through three different setters:

* `MCEuropeanEngine`      -> `option.set_mc_engine(...)`
* `MCEuropeanHestonEngine`-> `option.set_mc_heston_engine(...)`
* `MCAmericanEngine`      -> `option.set_mc_american_engine(...)` on an option
  built with `VanillaOption.american(...)`.

Unlike the analytic engine, an MC engine reports a standard error via
`option.error_estimate()`, and the American engine additionally reports the
fraction of paths exercised early via `option.exercise_probability()`. Both of
those raise on the analytic engine, so only call them on the MC options.

Pricing is seeded, so a given seed reproduces bitwise. The European result is
cross-checked against the analytic engine; the Heston and American results are
banded against QuantLib's cached values.

Run it with:

    python example/python/monte_carlo.py
"""

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import MCAmericanEngine, MCEuropeanEngine, MCEuropeanHestonEngine
from itofin.processes import BlackScholesProcess, HestonProcess
from itofin.time import Date, DayCounter


def mc_european() -> None:
    """A European call priced by MC and checked against the analytic engine."""
    settings = Settings()
    today = Date(15, 6, 2026)
    settings.set_evaluation_date(today)
    expiry = today + 360
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.20, today, DayCounter.actual360())

    # Analytic reference.
    analytic = VanillaOption(OptionType.Call, 100.0, expiry, settings)
    analytic.set_engine(process)

    # MC: single time step is enough for a path-independent European payoff.
    # NB: MCEuropeanEngine does not accept the antithetic variate.
    mc = VanillaOption(OptionType.Call, 100.0, expiry, settings)
    mc.set_mc_engine(MCEuropeanEngine(process, steps=1, samples=40000, seed=42))

    print("MC European call (K=100, 1y, 40000 paths, seed 42):")
    print(f"  analytic NPV = {analytic.npv():.6f}")
    print(f"  MC NPV       = {mc.npv():.6f}  +/- {mc.error_estimate():.6f} (std err)")


def mc_heston() -> None:
    """A Heston put priced by MC against QuantLib's cached hestonmodel.cpp
    fixture (expected value ~0.0633, banded by the standard error)."""
    settlement = Date(27, 12, 2004)
    exercise = Date(28, 3, 2005)
    settings = Settings()
    settings.set_evaluation_date(settlement)

    # HestonProcess argument order (see processes.pyi): risk-free, dividend,
    # spot, v0, kappa, theta, sigma, rho, reference date, day count. These are
    # the cached-test parameters, not everyday market levels.
    process = HestonProcess(
        0.7,  # risk-free rate
        0.4,  # dividend yield
        1.05,  # spot
        0.3,  # v0 (initial variance)
        1.16,  # kappa (mean reversion speed)
        0.2,  # theta (long-run variance)
        0.8,  # sigma (vol of vol)
        0.8,  # rho (spot/vol correlation)
        settlement,
        DayCounter.actual_actual_isda(),
    )

    option = VanillaOption(OptionType.Put, 1.05, exercise, settings)
    # The Heston MC engine does accept the antithetic variate.
    option.set_mc_heston_engine(
        MCEuropeanHestonEngine(process, steps_per_year=11, antithetic=True, samples=50000, seed=1234)
    )

    print("\nMC Heston put (K=1.05, 50000 paths, antithetic, seed 1234):")
    print(f"  MC NPV       = {option.npv():.10f}  +/- {option.error_estimate():.2e}")
    print("  cached value = 0.0632851309  (QuantLib hestonmodel.cpp)")


def mc_american() -> None:
    """An American put by least-squares Monte Carlo (Longstaff-Schwartz).

    Built with `VanillaOption.american(...)`, exercisable at any time over
    [earliest, latest]. The engine reports both a standard error and the
    early-exercise probability. Reproduces QuantLib's cached value 2.0544."""
    settings = Settings()
    today = Date(15, 5, 1998)
    settings.set_evaluation_date(today)
    settlement = Date(17, 5, 1998)
    maturity = Date(17, 5, 1999)
    process = BlackScholesProcess(36.0, 0.06, 0.0, 0.20, settlement, DayCounter.actual365_fixed())

    option = VanillaOption.american(
        OptionType.Put,
        36.0,
        earliest=settlement,
        latest=maturity,
        settings=settings,
    )
    option.set_mc_american_engine(
        MCAmericanEngine(
            process,
            steps=75,
            antithetic=True,
            absolute_tolerance=0.02,
            seed=42,
            polynomial_order=3,  # basis-polynomial order for the regression
        )
    )

    # A European put on the same market is the lower bound: early exercise can
    # only add value.
    european = VanillaOption(OptionType.Put, 36.0, maturity, settings)
    european.set_engine(process)

    print("\nMC American put (K=36, LSM, 75 steps, antithetic, seed 42):")
    print(f"  American NPV = {option.npv():.6f}  +/- {option.error_estimate():.6f}")
    print(f"  exercise probability = {option.exercise_probability():.4f}")
    print(f"  European NPV = {european.npv():.6f}  (early exercise adds value)")
    print("  cached American value = 2.0544  (QuantLib americanoption.cpp)")


def main() -> None:
    mc_european()
    mc_heston()
    mc_american()


if __name__ == "__main__":
    main()
