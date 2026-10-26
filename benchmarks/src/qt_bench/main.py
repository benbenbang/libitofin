"""Benchmark itofin (Rust/PyO3) vs QuantLib (C++/SWIG) from Python.

Setup:
    pip install QuantLib itofin
    python bench_itofin_vs_quantlib.py
"""

# standard library
import math
import statistics
import time

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import CalibrationErrorType, HestonModel, HestonModelHelper
from itofin.optimization import EndCriteria, LevenbergMarquardt
from itofin.processes import BlackScholesProcess, HestonProcess
from itofin.termstructures import FlatForward
from itofin.time import Calendar, Date, DayCounter, Period

# quantlib library
import QuantLib as ql

# ---------------------------------------------------------------- helpers


def bench(fn, number, repeat=7):
    """Return median seconds per call (after one warm-up call)."""
    fn()
    runs = []
    for _ in range(repeat):
        t0 = time.perf_counter()
        for _ in range(number):
            fn()
        runs.append((time.perf_counter() - t0) / number)
    return statistics.median(runs)


# ---------------------------------------------------------------- test 1: European sweep
# Spot 60, r 8%, q 0%, vol 30%, 90 days, Actual/360 (QuantLib's europeanoption.cpp case)

STRIKES = [40.0 + i * 0.05 for i in range(1000)]


def itofin_european_sweep():
    s = Settings()
    today = Date(15, 6, 2026)
    s.set_evaluation_date(today)
    dc = DayCounter.actual360()
    out = []
    for k in STRIKES:
        process = BlackScholesProcess(60.0, 0.08, 0.0, 0.30, today, dc)
        option = VanillaOption(OptionType.Call, k, today + 90, s)
        option.set_engine(process)
        out.append((option.npv(), option.delta()))
    return out


def ql_european_sweep():
    today = ql.Date(15, ql.June, 2026)
    ql.Settings.instance().evaluationDate = today
    dc = ql.Actual360()
    out = []
    for k in STRIKES:
        spot = ql.QuoteHandle(ql.SimpleQuote(60.0))
        r = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.08, dc))
        q = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.0, dc))
        vol = ql.BlackVolTermStructureHandle(ql.BlackConstantVol(today, ql.NullCalendar(), 0.30, dc))
        process = ql.BlackScholesMertonProcess(spot, q, r, vol)
        option = ql.VanillaOption(ql.PlainVanillaPayoff(ql.Option.Call, k), ql.EuropeanExercise(today + 90))
        option.setPricingEngine(ql.AnalyticEuropeanEngine(process))
        out.append((option.NPV(), option.delta()))
    return out


# ---------------------------------------------------------------- test 2: Heston calibration
# Flat 10% vol surface, r 4%, q 50%, 3x3 grid (itofin's README example)

VOL, R, Q = 0.10, 0.04, 0.50
MATURITIES = (1, 2, 3)
MONEYNESS = (-1.0, 0.0, 1.0)
LM_TOL = (1e-8, 1e-8, 1e-8)
END = (400, 40, 1e-8, 1e-8, 1e-8)
ORDER = 96


def itofin_heston():
    s = Settings()
    ref = Date(15, 1, 2026)
    s.set_evaluation_date(ref)
    dc = DayCounter.actual360()
    rf, dv = FlatForward(ref, R, dc), FlatForward(ref, Q, dc)
    helpers = []
    for n in MATURITIES:
        tau = float(n)
        fwd = dv.discount(tau) / rf.discount(tau)
        for m in MONEYNESS:
            k = fwd * math.exp(-m * VOL * math.sqrt(tau))
            helpers.append(
                HestonModelHelper(
                    Period(n, "Years"),
                    Calendar.null_calendar(),
                    1.0,
                    k,
                    VOL,
                    R,
                    Q,
                    CalibrationErrorType.RelativePriceError,
                    ref,
                    dc,
                    s,
                )
            )
    # (risk_free, dividend, spot, v0, kappa, theta, sigma, rho, ref, dc)
    model = HestonModel(HestonProcess(R, Q, 1.0, 0.01, 0.2, 0.02, 0.5, -0.75, ref, dc))
    model.calibrate(helpers, LevenbergMarquardt(*LM_TOL, False), EndCriteria(*END), ORDER)
    return model.sigma(), model.v0(), model.theta()


def ql_heston():
    ref = ql.Date(15, ql.January, 2026)
    ql.Settings.instance().evaluationDate = ref
    dc = ql.Actual360()
    rf_curve, dv_curve = ql.FlatForward(ref, R, dc), ql.FlatForward(ref, Q, dc)
    rf, dv = (
        ql.YieldTermStructureHandle(rf_curve),
        ql.YieldTermStructureHandle(dv_curve),
    )
    # (riskFree, dividend, s0, v0, kappa, theta, sigma, rho)
    process = ql.HestonProcess(rf, dv, ql.QuoteHandle(ql.SimpleQuote(1.0)), 0.01, 0.2, 0.02, 0.5, -0.75)
    model = ql.HestonModel(process)
    engine = ql.AnalyticHestonEngine(model, ORDER)
    helpers = []
    for n in MATURITIES:
        tau = float(n)
        fwd = dv_curve.discount(tau) / rf_curve.discount(tau)
        for m in MONEYNESS:
            k = fwd * math.exp(-m * VOL * math.sqrt(tau))
            h = ql.HestonModelHelper(
                ql.Period(n, ql.Years),
                ql.NullCalendar(),
                1.0,
                k,
                ql.QuoteHandle(ql.SimpleQuote(VOL)),
                rf,
                dv,
                ql.BlackCalibrationHelper.RelativePriceError,
            )
            h.setPricingEngine(engine)
            helpers.append(h)
    model.calibrate(helpers, ql.LevenbergMarquardt(*LM_TOL), ql.EndCriteria(*END))
    return model.sigma(), model.v0(), model.theta()
