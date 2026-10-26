"""Extra workloads: market reuse, spot repricing, curve bootstrap, swaptions."""

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import (
    EuropeanExercise,
    OptionType,
    SettlementMethod,
    SettlementType,
    Swaption,
    SwapType,
    VanillaOption,
    VanillaSwap,
)
from itofin.models import HullWhite
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, FlatForward, PiecewiseYieldCurve, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

# quantlib library
import QuantLib as ql

# ======================================================================
# 6a. European, market built once (only option + engine per strike)
# ======================================================================


def itofin_european_reuse(strikes):
    s = Settings()
    today = Date(15, 6, 2026)
    s.set_evaluation_date(today)
    process = BlackScholesProcess(60.0, 0.08, 0.0, 0.30, today, DayCounter.actual360())
    expiry, total = today + 90, 0.0
    for k in strikes:
        option = VanillaOption(OptionType.Call, k, expiry, s)
        option.set_engine(process)
        total += option.npv() + option.delta()
    return total


def ql_european_reuse(strikes):
    today = ql.Date(15, ql.June, 2026)
    ql.Settings.instance().evaluationDate = today
    dc = ql.Actual360()
    process = ql.BlackScholesMertonProcess(
        ql.QuoteHandle(ql.SimpleQuote(60.0)),
        ql.YieldTermStructureHandle(ql.FlatForward(today, 0.0, dc)),
        ql.YieldTermStructureHandle(ql.FlatForward(today, 0.08, dc)),
        ql.BlackVolTermStructureHandle(ql.BlackConstantVol(today, ql.NullCalendar(), 0.30, dc)),
    )
    engine = ql.AnalyticEuropeanEngine(process)
    exercise, total = ql.EuropeanExercise(today + 90), 0.0
    for k in strikes:
        option = ql.VanillaOption(ql.PlainVanillaPayoff(ql.Option.Call, k), exercise)
        option.setPricingEngine(engine)
        total += option.NPV() + option.delta()
    return total


EURO_REUSE = {"itofin": itofin_european_reuse, "QuantLib": ql_european_reuse}

# ======================================================================
# 6b. Reprice one option at n new spots (each library's natural way)
# ======================================================================


def _spots(n):
    return [55.0 + 10.0 * i / n for i in range(n)]  # every step is a new value


def itofin_respot(n):
    s = Settings()
    today = Date(15, 6, 2026)
    s.set_evaluation_date(today)
    dc = DayCounter.actual360()
    option = VanillaOption(OptionType.Call, 65.0, today + 90, s)
    total = 0.0
    for spot in _spots(n):
        option.set_engine(BlackScholesProcess(spot, 0.08, 0.0, 0.30, today, dc))
        total += option.npv() + option.delta()
    return total


def ql_respot(n):
    today = ql.Date(15, ql.June, 2026)
    ql.Settings.instance().evaluationDate = today
    dc = ql.Actual360()
    spot = ql.SimpleQuote(60.0)
    process = ql.BlackScholesMertonProcess(
        ql.QuoteHandle(spot),
        ql.YieldTermStructureHandle(ql.FlatForward(today, 0.0, dc)),
        ql.YieldTermStructureHandle(ql.FlatForward(today, 0.08, dc)),
        ql.BlackVolTermStructureHandle(ql.BlackConstantVol(today, ql.NullCalendar(), 0.30, dc)),
    )
    option = ql.VanillaOption(ql.PlainVanillaPayoff(ql.Option.Call, 65.0), ql.EuropeanExercise(today + 90))
    option.setPricingEngine(ql.AnalyticEuropeanEngine(process))
    total = 0.0
    for value in _spots(n):
        spot.setValue(value)
        total += option.NPV() + option.delta()
    return total


RESPOT = {"itofin": itofin_respot, "QuantLib": ql_respot}

# ======================================================================
# 7. Curve bootstrap: 3 Euribor deposits + 9 swaps vs 6M Euribor
# ======================================================================

DEPOSITS = ((1, 0.0300), (3, 0.0310), (6, 0.0320))  # months, rate
SWAPS = (
    (1, 0.0330),
    (2, 0.0340),
    (3, 0.0345),
    (5, 0.0350),
    (7, 0.0355),
    (10, 0.0360),
    (15, 0.0365),
    (20, 0.0368),
    (30, 0.0370),
)  # years, rate
TIMES = [0.5 * i for i in range(1, 60)]  # 0.5y to 29.5y
BUMP_INDEX = len(DEPOSITS) + 5  # the 10Y swap quote


def _it_curve():
    s = Settings()
    today = Date(15, 1, 2026)
    s.set_evaluation_date(today)
    idx6m = Euribor(Period(6, "Months"), None, s)
    quotes, helpers = [], []
    for m, r in DEPOSITS:
        q = SimpleQuote(r)
        quotes.append(q)
        helpers.append(DepositRateHelper(q, Euribor(Period(m, "Months"), None, s)))
    for y, r in SWAPS:
        q = SimpleQuote(r)
        quotes.append(q)
        helpers.append(
            SwapRateHelper(
                q,
                Period(y, "Years"),
                Calendar.target(),
                Frequency.Annual,
                BusinessDayConvention.Unadjusted,
                DayCounter.thirty360_bond_basis(),
                idx6m,
            )
        )
    curve = PiecewiseYieldCurve(today, helpers, DayCounter.actual365_fixed())
    return s, curve, quotes  # keep Settings alive with the curve


def _ql_curve():
    today = ql.Date(15, ql.January, 2026)
    ql.Settings.instance().evaluationDate = today
    idx6m = ql.Euribor6M()
    quotes, helpers = [], []
    for m, r in DEPOSITS:
        q = ql.SimpleQuote(r)
        quotes.append(q)
        helpers.append(ql.DepositRateHelper(ql.QuoteHandle(q), ql.Euribor(ql.Period(m, ql.Months))))
    for y, r in SWAPS:
        q = ql.SimpleQuote(r)
        quotes.append(q)
        helpers.append(
            ql.SwapRateHelper(
                ql.QuoteHandle(q),
                ql.Period(y, ql.Years),
                ql.TARGET(),
                ql.Annual,
                ql.Unadjusted,
                ql.Thirty360(ql.Thirty360.BondBasis),
                idx6m,
            )
        )
    curve = ql.PiecewiseLogLinearDiscount(today, helpers, ql.Actual365Fixed())
    return helpers, curve, quotes  # keep helpers alive with the curve


def itofin_curve_dfs():
    _, curve, _ = _it_curve()
    return [curve.discount(t) for t in TIMES]


def ql_curve_dfs():
    _, curve, _ = _ql_curve()
    return [curve.discount(t) for t in TIMES]


CURVE_DFS = {"itofin": itofin_curve_dfs, "QuantLib": ql_curve_dfs}


def itofin_bootstrap(reps):
    total = 0.0
    for _ in range(reps):
        _, curve, _ = _it_curve()
        total += curve.discount(29.5)  # first query triggers the bootstrap
    return total


def ql_bootstrap(reps):
    total = 0.0
    for _ in range(reps):
        _, curve, _ = _ql_curve()
        total += curve.discount(29.5)
    return total


BOOTSTRAP = {"itofin": itofin_bootstrap, "QuantLib": ql_bootstrap}


def itofin_curve_bump(n):
    _s, curve, quotes = _it_curve()
    curve.discount(1.0)
    q = quotes[BUMP_INDEX]
    base, total = q.value(), 0.0
    for i in range(n):
        q.set_value(base + 0.0001 * (i % 2 + 1))  # alternate +1bp / +2bp
        total += curve.discount(10.0)
    return total


def ql_curve_bump(n):
    _h, curve, quotes = _ql_curve()
    curve.discount(1.0)
    q = quotes[BUMP_INDEX]
    base, total = q.value(), 0.0
    for i in range(n):
        q.setValue(base + 0.0001 * (i % 2 + 1))
        total += curve.discount(10.0)
    return total


CURVE_BUMP = {"itofin": itofin_curve_bump, "QuantLib": ql_curve_bump}

# ======================================================================
# 8. Swaption sweep: 1Y into 5Y payer, Hull-White + Jamshidian
# ======================================================================


def make_swpn_strikes(n):
    return [0.01 + 0.04 * i / n for i in range(n)]  # i = n/2 gives 3%


def itofin_swaptions(strikes, detail=False):
    s = Settings()
    today = Date(15, 1, 2026)
    s.set_evaluation_date(today)
    curve = FlatForward(today, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        Date(15, 1, 2028), Date(15, 1, 2033), Frequency.Annual, Calendar.target(), BusinessDayConvention.Unadjusted
    )
    floating = Schedule(
        Date(15, 1, 2028), Date(15, 1, 2033), Frequency.Semiannual, Calendar.target(), BusinessDayConvention.Unadjusted
    )
    index = Euribor.six_months(curve, s)
    model = HullWhite(curve, 0.05, 0.01)
    exercise = EuropeanExercise(Date(15, 1, 2027))
    fixed_dc, float_dc = DayCounter.thirty360_bond_basis(), DayCounter.actual360()
    out, total = [], 0.0
    for k in strikes:
        swap = VanillaSwap(SwapType.Payer, 100.0, fixed, k, fixed_dc, floating, index, 0.0, float_dc, s)
        swaption = Swaption(swap, exercise, SettlementType.Physical, SettlementMethod.PhysicalOTC, s)
        swaption.set_jamshidian_engine(model)
        npv = swaption.npv()
        total += npv
        if detail:
            out.append(npv)
    return out if detail else total


def ql_swaptions(strikes, detail=False):
    today = ql.Date(15, ql.January, 2026)
    ql.Settings.instance().evaluationDate = today
    curve = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.03, ql.Actual365Fixed()))

    def schedule(freq):
        return ql.Schedule(
            ql.Date(15, 1, 2028),
            ql.Date(15, 1, 2033),
            ql.Period(freq),
            ql.TARGET(),
            ql.Unadjusted,
            ql.Unadjusted,
            ql.DateGeneration.Forward,
            False,
        )

    fixed, floating = schedule(ql.Annual), schedule(ql.Semiannual)
    index = ql.Euribor6M(curve)
    engine = ql.JamshidianSwaptionEngine(ql.HullWhite(curve, 0.05, 0.01))
    exercise = ql.EuropeanExercise(ql.Date(15, 1, 2027))
    fixed_dc, float_dc = ql.Thirty360(ql.Thirty360.BondBasis), ql.Actual360()
    out, total = [], 0.0
    for k in strikes:
        swap = ql.VanillaSwap(ql.Swap.Payer, 100.0, fixed, k, fixed_dc, floating, index, 0.0, float_dc)
        swaption = ql.Swaption(swap, exercise, ql.Settlement.Physical, ql.Settlement.PhysicalOTC)
        swaption.setPricingEngine(engine)
        npv = swaption.NPV()
        total += npv
        if detail:
            out.append(npv)
    return out if detail else total


SWPN = {"itofin": itofin_swaptions, "QuantLib": ql_swaptions}
