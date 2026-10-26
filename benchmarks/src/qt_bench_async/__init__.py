"""Stress benchmark: itofin (Rust/PyO3) vs QuantLib (C++/SWIG).

Setup:
    pip install QuantLib itofin
Run:
    python stress_itofin_vs_quantlib.py                 # default, a few minutes
    python stress_itofin_vs_quantlib.py --scale 3       # heavier
    python stress_itofin_vs_quantlib.py --workers 4     # limit cores
Warning: pins all CPU cores at 100% for several minutes.
"""

# standard library
import argparse
import math
import multiprocessing as mp
import os
import platform
import statistics
import sys
import time
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
from concurrent.futures.process import BrokenProcessPool
from importlib.metadata import version

# pypi/conda library
from rich import print

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

CTX = mp.get_context("spawn")  # same behaviour on macOS, Linux, Windows
LIBS = ("itofin", "QuantLib")

# ======================================================================
# Workload 1: European option sweep (build + price + delta per strike)
# ======================================================================


def make_strikes(n):
    return [40.0 + 50.0 * i / n for i in range(n)]


def itofin_european(strikes, detail=False):
    s = Settings()
    today = Date(15, 6, 2026)
    s.set_evaluation_date(today)
    dc = DayCounter.actual360()
    out, total = [], 0.0
    for k in strikes:
        process = BlackScholesProcess(60.0, 0.08, 0.0, 0.30, today, dc)
        option = VanillaOption(OptionType.Call, k, today + 90, s)
        option.set_engine(process)
        npv, delta = option.npv(), option.delta()
        total += npv + delta
        if detail:
            out.append((npv, delta))
    return out if detail else total


QL_EU_DATE = ql.Date(15, ql.June, 2026)


def ql_european(strikes, detail=False):
    # Only write the global date when it differs, so threads mostly read it.
    if ql.Settings.instance().evaluationDate != QL_EU_DATE:
        ql.Settings.instance().evaluationDate = QL_EU_DATE
    today, dc = QL_EU_DATE, ql.Actual360()
    out, total = [], 0.0
    for k in strikes:
        spot = ql.QuoteHandle(ql.SimpleQuote(60.0))
        r = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.08, dc))
        q = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.0, dc))
        vol = ql.BlackVolTermStructureHandle(ql.BlackConstantVol(today, ql.NullCalendar(), 0.30, dc))
        process = ql.BlackScholesMertonProcess(spot, q, r, vol)
        option = ql.VanillaOption(ql.PlainVanillaPayoff(ql.Option.Call, k), ql.EuropeanExercise(today + 90))
        option.setPricingEngine(ql.AnalyticEuropeanEngine(process))
        npv, delta = option.NPV(), option.delta()
        total += npv + delta
        if detail:
            out.append((npv, delta))
    return out if detail else total


EURO = {"itofin": itofin_european, "QuantLib": ql_european}

# ======================================================================
# Workload 2: heavy Heston calibration (35 helpers, order 192)
# ======================================================================

VOL, R, Q = 0.10, 0.04, 0.50
MONTHS = (6, 12, 18, 24, 36, 48, 60)
MONEYNESS = (-1.5, -0.75, 0.0, 0.75, 1.5)
ORDER = 192
LM_TOL = (1e-8, 1e-8, 1e-8)
END = (1000, 100, 1e-8, 1e-8, 1e-8)
START = (0.01, 0.2, 0.02, 0.5, -0.75)  # v0, kappa, theta, sigma, rho


def _strikes_for(discount_q, discount_r):
    for n in MONTHS:
        tau = n / 12.0
        fwd = discount_q(tau) / discount_r(tau)
        for m in MONEYNESS:
            yield n, fwd * math.exp(-m * VOL * math.sqrt(tau))


def itofin_heston():
    s = Settings()
    ref = Date(15, 1, 2026)
    s.set_evaluation_date(ref)
    dc = DayCounter.actual360()
    rf, dv = FlatForward(ref, R, dc), FlatForward(ref, Q, dc)
    helpers = [
        HestonModelHelper(
            Period(n, "Months"),
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
        for n, k in _strikes_for(dv.discount, rf.discount)
    ]
    model = HestonModel(HestonProcess(R, Q, 1.0, *START, ref, dc))
    model.calibrate(helpers, LevenbergMarquardt(*LM_TOL, False), EndCriteria(*END), ORDER)
    return model.sigma(), model.v0(), model.theta()


def ql_heston():
    ref = ql.Date(15, ql.January, 2026)
    ql.Settings.instance().evaluationDate = ref
    dc = ql.Actual360()
    rf_c, dv_c = ql.FlatForward(ref, R, dc), ql.FlatForward(ref, Q, dc)
    rf, dv = ql.YieldTermStructureHandle(rf_c), ql.YieldTermStructureHandle(dv_c)
    process = ql.HestonProcess(rf, dv, ql.QuoteHandle(ql.SimpleQuote(1.0)), *START)
    model = ql.HestonModel(process)
    engine = ql.AnalyticHestonEngine(model, ORDER)
    helpers = []
    for n, k in _strikes_for(dv_c.discount, rf_c.discount):
        h = ql.HestonModelHelper(
            ql.Period(n, ql.Months),
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


HESTON = {"itofin": itofin_heston, "QuantLib": ql_heston}

# ======================================================================
# Sub-process tasks (top level so "spawn" can pickle them)
# ======================================================================


def peak_rss_mb():
    try:
        # standard library
        import resource
    except ImportError:  # Windows
        return float("nan")
    r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return r / 2**20 if sys.platform == "darwin" else r / 1024


def euro_task(lib, n):
    return EURO[lib](make_strikes(n))


def thread_bench(lib, workers, n):
    fn, strikes = EURO[lib], make_strikes(n)
    fn(strikes[:10])  # warm-up + set global state on this thread
    with ThreadPoolExecutor(max_workers=workers) as ex:
        t0 = time.perf_counter()
        list(ex.map(fn, [strikes] * workers))
        wall = time.perf_counter() - t0
    return workers * n / wall


def soak(lib, iters, n):
    fn, strikes = EURO[lib], make_strikes(n)
    fn(strikes)
    fn(strikes)
    start = peak_rss_mb()
    for _ in range(iters):
        fn(strikes)
    return start, peak_rss_mb()


def in_sandbox(fn, *args):
    """Run fn in a fresh process. Returns None if that process crashes."""
    with ProcessPoolExecutor(max_workers=1, mp_context=CTX) as ex:
        try:
            return ex.submit(fn, *args).result()
        except BrokenProcessPool:
            return None


# ======================================================================
# Timing helpers
# ======================================================================


def timed(fn, *args, repeat):
    fn(*args)  # warm-up
    runs = []
    for _ in range(repeat):
        t0 = time.perf_counter()
        fn(*args)
        runs.append(time.perf_counter() - t0)
    med = statistics.median(runs)
    cv = statistics.pstdev(runs) / statistics.mean(runs) * 100
    return med, cv


def process_bench(lib, workers, n):
    with ProcessPoolExecutor(max_workers=workers, mp_context=CTX) as ex:
        list(ex.map(euro_task, [lib] * workers * 2, [10] * workers * 2))  # spin up
        t0 = time.perf_counter()
        list(ex.map(euro_task, [lib] * workers, [n] * workers))
        wall = time.perf_counter() - t0
    return workers * n / wall


def ratio(a, b):
    return f"{a / b:.2f}x" if b else "n/a"


# ======================================================================
# Main
# ======================================================================


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--workers", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--repeat", type=int, default=5)
    a = ap.parse_args()

    n_single = int(20_000 * a.scale)
    n_worker = int(20_000 * a.scale)
    n_soak, soak_iters = int(5_000 * a.scale), 50

    print(
        f"Python {platform.python_version()} | {platform.machine()} | "
        f"{os.cpu_count()} CPUs | QuantLib {version('QuantLib')} | "
        f"itofin {version('itofin')}"
    )
    print(f"scale={a.scale} workers={a.workers} repeat={a.repeat}\n")

    # --- 0. Correctness ------------------------------------------------
    print("== 0. Correctness ==")
    probe = make_strikes(2_000)
    it, qq = itofin_european(probe, True), ql_european(probe, True)
    d_npv = max(abs(x[0] - y[0]) for x, y in zip(it, qq))
    d_dlt = max(abs(x[1] - y[1]) for x, y in zip(it, qq))
    print(f"European max |diff| over {len(probe)} strikes: NPV {d_npv:.2e} | delta {d_dlt:.2e}")
    for name, x, y in zip(("sigma", "v0", "theta"), itofin_heston(), ql_heston()):
        print(f"Heston {name:<5}: itofin {x:.6f} | QuantLib {y:.6f}")

    results = {}

    # --- 1. Single-thread European -------------------------------------
    print(f"\n== 1. Single-thread European ({n_single:,} options/run) ==")
    strikes = make_strikes(n_single)
    for lib in LIBS:
        med, cv = timed(EURO[lib], strikes, repeat=a.repeat)
        results[("single", lib)] = n_single / med
        print(f"{lib:<9} {med / n_single * 1e6:8.2f} us/option | {n_single / med:>10,.0f} options/s | CV {cv:4.1f}%")

    # --- 2. Heavy Heston -----------------------------------------------
    print(f"\n== 2. Heston calibration ({len(MONTHS) * len(MONEYNESS)} helpers, order {ORDER}) ==")
    for lib in LIBS:
        med, cv = timed(HESTON[lib], repeat=max(3, a.repeat // 2))
        results[("heston", lib)] = med
        print(f"{lib:<9} {med * 1e3:9.1f} ms/calibration | CV {cv:4.1f}%")

    # --- 3. Multi-thread (sandboxed) -----------------------------------
    print(f"\n== 3. {a.workers} threads x {n_worker:,} options (isolated process) ==")
    for lib in LIBS:
        thr = in_sandbox(thread_bench, lib, a.workers, n_worker)
        if thr is None:
            print(f"{lib:<9} CRASHED under concurrent threads")
            continue
        results[("threads", lib)] = thr
        print(f"{lib:<9} {thr:>10,.0f} options/s | speed-up vs 1 thread {ratio(thr, results[('single', lib)])}")

    # --- 4. Multi-process ----------------------------------------------
    print(f"\n== 4. {a.workers} processes x {n_worker:,} options ==")
    for lib in LIBS:
        thr = process_bench(lib, a.workers, n_worker)
        results[("procs", lib)] = thr
        print(f"{lib:<9} {thr:>10,.0f} options/s | speed-up vs 1 process {ratio(thr, results[('single', lib)])}")

    # --- 5. Memory soak ------------------------------------------------
    print(f"\n== 5. Memory soak ({soak_iters} x {n_soak:,} options, fresh process) ==")
    for lib in LIBS:
        res = in_sandbox(soak, lib, soak_iters, n_soak)
        if res is None:
            print(f"{lib:<9} CRASHED during soak")
            continue
        start, end = res
        print(f"{lib:<9} peak RSS {start:7.1f} MB -> {end:7.1f} MB (growth {end - start:+.1f} MB)")

    # --- Summary -------------------------------------------------------
    print("\n== Summary (itofin advantage; > 1.00x means itofin is faster) ==")
    for key, label in (
        ("single", "Single-thread European"),
        ("threads", "Multi-thread European"),
        ("procs", "Multi-process European"),
    ):
        i, q = results.get((key, "itofin")), results.get((key, "QuantLib"))
        if i and q:
            print(f"{label:<24} {ratio(i, q)}")
    i, q = results[("heston", "itofin")], results[("heston", "QuantLib")]
    print(f"{'Heston calibration':<24} {ratio(q, i)}")


if __name__ == "__main__":
    main()
