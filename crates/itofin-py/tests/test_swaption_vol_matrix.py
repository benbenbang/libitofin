"""Oracle for the interpolated ATM swaption vol matrix facade (issue #613).

The fixture is the core module test's, in turn QuantLib's
``swaptionvolstructuresutilities.hpp`` grid: TARGET, ModifiedFollowing,
Actual/365F, option tenors [1M,6M,1Y,5Y,10Y,30Y] x swap tenors [1Y,5Y,10Y,30Y]
and the 6x4 vols, all read against evaluation date 15-June-2026.

Three arms:

A. Node recovery on the fixed-reference constructor. Bilinear is exact at the
   nodes, so all 24 come back; the core asserts 1e-16 and this pass keeps 1e-12.
   The query strike is 0.05, not the core's 0.0, so it also pins that the ATM
   grid range-checks the strike and then ignores it.
B. Quote-bump refresh on the moving constructor. Reading a node, bumping its
   quote and re-reading pins the observer chain end to end through Python: an
   interpolation built once and never rebuilt serves the stale vol. Bumping a
   DIFFERENT node's quote and re-reading the first pins that the grid rebuilds
   from the quotes rather than flooding one value across the lattice.
C. Engine integration. The same swaption priced on the matrix and on a
   ``ConstantSwaptionVolatility`` at the vol the matrix serves for that
   swaption's own coordinates must agree.

   Arm C's coincidence is exact by construction, not to a tolerance. The engine
   reads ``black_variance(exercise_date, swap_length, strike)``, i.e.
   ``vol^2 * time_from_reference(exercise_date)``; both surfaces are Act/365F off
   15-June-2026 with a zero shift, so given the same vol the two routes run the
   identical float sequence over the identical swap, curve and annuity.

   The coordinates are made to line up rather than assumed. EXERCISE is built by
   advancing EVAL 2Y on TARGET/ModifiedFollowing, which is exactly what the
   surface's own tenor-to-date conversion does, so ``volatility(2Y, ...)`` reads
   the engine's option date. The swap runs EXERCISE to EXERCISE+7y = 2556 days,
   and the engine's ``swap_length`` rounds ``2556/365.25*12 = 83.98`` to 84
   months = 7.0 years exactly, which is what the 7Y swap tenor resolves to.

   2Y x 7Y sits strictly inside the grid but on no node, so the matrix must
   interpolate: an interpolator that snapped to a node would be caught by
   ``test_the_engine_coordinates_are_off_the_nodes``. What arm C pins is that
   the engine reads THIS surface at the swaption's own coordinates - both sides
   read the interpolated number from the same matrix, so it is not an
   independent check of that number. Arm A and the core's 1e-16 test cover that.
"""

import pytest

from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import (
    EuropeanExercise,
    SettlementMethod,
    SettlementType,
    Swaption,
    SwapType,
    VanillaSwap,
)
from itofin.pricingengines import BlackSwaptionEngine, CashAnnuityModel
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantSwaptionVolatility,
    FlatForward,
    SwaptionVolatilityMatrix,
    VolatilityType,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

EVAL = Date(15, 6, 2026)
BDC = BusinessDayConvention.ModifiedFollowing

OPTION_TENORS = [
    Period(1, "Months"),
    Period(6, "Months"),
    Period(1, "Years"),
    Period(5, "Years"),
    Period(10, "Years"),
    Period(30, "Years"),
]
SWAP_TENORS = [
    Period(1, "Years"),
    Period(5, "Years"),
    Period(10, "Years"),
    Period(30, "Years"),
]
VOLS = [
    [0.1300, 0.1560, 0.1390, 0.1220],
    [0.1440, 0.1580, 0.1460, 0.1260],
    [0.1600, 0.1590, 0.1470, 0.1290],
    [0.1640, 0.1470, 0.1370, 0.1220],
    [0.1400, 0.1300, 0.1250, 0.1100],
    [0.1130, 0.1090, 0.1070, 0.0930],
]

TOL = 1e-12
STRIKE = 0.03
QUERY_STRIKE = 0.05
BUMP = 0.05

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)

EXERCISE = Calendar.target().advance(EVAL, 2, "Years", BDC, False)
SWAP_END = Date(EXERCISE.day, EXERCISE.month, EXERCISE.year + 7)
ENGINE_OPTION_TENOR = Period(2, "Years")
ENGINE_SWAP_TENOR = Period(7, "Years")
SPAN_DAYS = 2556


def _fixed_matrix():
    return SwaptionVolatilityMatrix(
        EVAL,
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        SWAP_TENORS,
        VOLS,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
    )


def _moving_matrix():
    """The grid over a distinct ``SimpleQuote`` per node, plus those quotes."""
    quotes = [[SimpleQuote(vol) for vol in row] for row in VOLS]
    matrix = SwaptionVolatilityMatrix.moving(
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        SWAP_TENORS,
        quotes,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        SETTINGS,
    )
    return matrix, quotes


def _fixture():
    """A fresh curve/swap/swaption on the one shared ``Settings``.

    Every arm rebuilds its own: an ``Instrument`` caches its NPV and the engine
    silently installs its own discounting engine on the swap it prices, so a
    reused swaption would let arm C pass on a stale number.
    """
    curve = FlatForward(EVAL, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        EXERCISE,
        SWAP_END,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.Unadjusted,
    )
    floating = Schedule(
        EXERCISE,
        SWAP_END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.Unadjusted,
    )
    swap = VanillaSwap(
        SwapType.Payer,
        100.0,
        fixed,
        STRIKE,
        DayCounter.thirty360_bond_basis(),
        floating,
        Euribor.six_months(curve, SETTINGS),
        0.0,
        DayCounter.actual360(),
        SETTINGS,
    )
    swaption = Swaption(
        swap,
        EuropeanExercise(EXERCISE),
        SettlementType.Physical,
        SettlementMethod.PhysicalOTC,
        SETTINGS,
    )
    return curve, floating, swaption


def _npv_on(surface):
    curve, _floating, swaption = _fixture()
    swaption.set_black_engine(
        BlackSwaptionEngine(surface, curve, SETTINGS, CashAnnuityModel.SwapRate)
    )
    return swaption.npv()


def test_fixed_reference_matrix_recovers_every_node():
    matrix = _fixed_matrix()
    for i, option_tenor in enumerate(OPTION_TENORS):
        for j, swap_tenor in enumerate(SWAP_TENORS):
            got = matrix.volatility(option_tenor, swap_tenor, QUERY_STRIKE)
            assert got == pytest.approx(VOLS[i][j], abs=TOL), f"node ({i},{j})"


def test_the_atm_matrix_ignores_the_query_strike():
    matrix = _fixed_matrix()
    coordinates = (ENGINE_OPTION_TENOR, ENGINE_SWAP_TENOR)
    assert matrix.volatility(*coordinates, 0.0) == matrix.volatility(*coordinates, 0.20)


def test_a_quote_bump_refreshes_the_moving_matrix():
    matrix, quotes = _moving_matrix()
    node = (OPTION_TENORS[0], SWAP_TENORS[0], STRIKE)
    neighbour = (OPTION_TENORS[0], SWAP_TENORS[1], STRIKE)

    assert matrix.volatility(*node) == pytest.approx(VOLS[0][0], abs=TOL)

    quotes[0][0].set_value(VOLS[0][0] + BUMP)
    assert matrix.volatility(*node) == pytest.approx(VOLS[0][0] + BUMP, abs=TOL)

    quotes[0][1].set_value(VOLS[0][1] + BUMP)
    assert matrix.volatility(*node) == pytest.approx(VOLS[0][0] + BUMP, abs=TOL), (
        "bumping a neighbour's quote must leave this node alone"
    )
    assert matrix.volatility(*neighbour) == pytest.approx(VOLS[0][1] + BUMP, abs=TOL)


def test_the_engine_coordinates_are_off_the_nodes():
    """2Y x 7Y is inside the grid but on no node, so the served vol is genuinely
    interpolated: it lies strictly between its four surrounding node vols."""
    matrix = _fixed_matrix()
    served = matrix.volatility(ENGINE_OPTION_TENOR, ENGINE_SWAP_TENOR, STRIKE)
    corners = [VOLS[2][1], VOLS[2][2], VOLS[3][1], VOLS[3][2]]
    assert served not in [vol for row in VOLS for vol in row]
    assert min(corners) < served < max(corners)


def test_the_swap_spans_the_seven_year_tenor_the_matrix_is_queried_at():
    """The engine reads swap_length off the floating schedule's endpoints, which
    are the unadjusted EXERCISE and SWAP_END only because the schedule says so.
    2556 days round to 84 months, the 7Y the matrix query resolves to."""
    _curve, floating, _swaption = _fixture()
    dates = floating.dates()
    assert dates[0] == EXERCISE
    assert dates[-1] == SWAP_END
    assert EXERCISE + SPAN_DAYS == SWAP_END
    assert round(SPAN_DAYS / 365.25 * 12.0) / 12.0 == 7.0


def test_flat_extrapolation_clamps_past_the_grid():
    """Both constructors route on flat_extrapolation, so both are pinned: an
    inverted flag or a route that ignores it is otherwise invisible. Past the
    30Y option tenor the plain grid extends the boundary surface while the flat
    one clamps to the 30Y x 30Y corner vol exactly."""
    past = (Period(40, "Years"), SWAP_TENORS[-1], STRIKE, True)
    corner = VOLS[-1][-1]

    plain_fixed = _fixed_matrix()
    flat_fixed = SwaptionVolatilityMatrix(
        EVAL,
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        SWAP_TENORS,
        VOLS,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        None,
        True,
    )
    assert flat_fixed.volatility(*past) == pytest.approx(corner, abs=TOL)
    assert plain_fixed.volatility(*past) != flat_fixed.volatility(*past)

    plain_moving, _quotes = _moving_matrix()
    flat_moving = SwaptionVolatilityMatrix.moving(
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        SWAP_TENORS,
        [[SimpleQuote(vol) for vol in row] for row in VOLS],
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        SETTINGS,
        None,
        True,
    )
    assert flat_moving.volatility(*past) == pytest.approx(corner, abs=TOL)
    assert plain_moving.volatility(*past) != flat_moving.volatility(*past)


def test_the_engine_prices_off_this_matrix():
    matrix = _fixed_matrix()
    served = matrix.volatility(ENGINE_OPTION_TENOR, ENGINE_SWAP_TENOR, STRIKE)
    constant = ConstantSwaptionVolatility(
        EVAL,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        served,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        0.0,
    )
    npv_matrix = _npv_on(matrix)
    npv_constant = _npv_on(constant)
    print(
        f"\nserved vol at 2Yx7Y = {served!r}"
        f"\nnpv on the matrix    = {npv_matrix!r}"
        f"\nnpv on the constant  = {npv_constant!r}"
    )
    assert npv_matrix > 0.0
    assert npv_matrix == npv_constant
