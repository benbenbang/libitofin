"""Oracle for the cap/floor term-volatility surface facade (issue #622).

The fixture is the core module test's (``capfloortermvolsurface.rs`` tests):
TARGET, ModifiedFollowing, Actual/365F, reference 15-June-2026, option tenors
[1Y,2Y,3Y,4Y] x strikes [1%..5%] and the 4x5 grid ``0.10 + 0.01*i + 0.001*j``.
The grid is deliberately NON-FLAT and NON-SQUARE: a transposed grid fails the
core's dimension check outright, and a swapped axis fails numerically, so the
node-recovery arm cannot pass on a facade that crosses the axes.

The two pinned constructors need no ``Settings``: they fix the reference date,
so no query reads an evaluation date. The two moving ones do, and the fixture's
``SETTINGS`` sits on ``REFERENCE`` so all four agree on the reference date.

Four arms:

A. Node recovery at 1e-12, on BOTH constructors. Bicubic is exact at the nodes,
   so all 20 come back. Running it on the quote form as well as the matrix form
   is what pins the ORIENTATION of the handle grid - arm B only reads two cells
   of one row, which a transposing ``with_quotes`` would survive.
B. Quote-bump refresh on the quote constructor. Reading a node, bumping its
   quote and re-reading pins the observer chain end to end through Python: a
   spline built once and never rebuilt serves the stale vol. The untouched
   neighbour must not move, which catches a rebuild that floods one value
   across the grid.
C. The three query forms agree at an off-node point (18M x 2.5%, on no tenor
   and no strike node). ``volatility`` resolves the tenor against the surface's
   own calendar and convention, ``volatility_date`` takes that date, and
   ``volatility_time`` takes the year fraction; they are one call chain in the
   core, so this is an EXACT equality, not a tolerance. Both the date and the
   time are computed independently on the Python side - the date by advancing
   the reference on TARGET/ModifiedFollowing, which is what the core's
   tenor-to-date conversion does, and the time as Act/365F's integer days over
   365 - so the arm is an independent check of the resolution chain rather than
   a tautology. The value must also land between the four surrounding nodes,
   which pins that the query interpolates rather than snapping to one.
D. The shape guard. A ragged grid is rejected as an ``ItofinError`` before it
   reaches the core, where a short row would index a `Matrix` out of bounds and
   panic across the FFI boundary.
E. The two MOVING constructors (#623), whose reference date floats
   ``settlement_days`` off the evaluation date rather than being pinned. They
   are a distinct code path, not a thin wrapper: they carry the settlement days
   the optionlet-stripping adapter reads back, and the quote form registers
   market data the matrix form does not. Both go through arm A, so a
   transposed grid cannot hide behind the flat fixture the stripping oracle
   uses; the quote form also goes through arm B's bump. At zero settlement days
   their reference date IS the evaluation date, which the fixture sets to
   REFERENCE, so they must recover exactly the pinned surface's nodes.
"""

import datetime

import pytest

from itofin import ItofinError, Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import CapFloorTermVolSurface
from itofin.time import BusinessDayConvention, Calendar, DayCounter, Date, Period

REFERENCE = Date(15, 6, 2026)
BDC = BusinessDayConvention.ModifiedFollowing

OPTION_TENORS = [Period(n, "Years") for n in (1, 2, 3, 4)]
STRIKES = [0.01, 0.02, 0.03, 0.04, 0.05]
VOLS = [[0.10 + 0.01 * i + 0.001 * j for j in range(5)] for i in range(4)]

TOL = 1e-12
BUMPED = 0.5

OFF_NODE_TENOR = Period(18, "Months")
OFF_NODE_STRIKE = 0.025

SETTINGS = Settings()
SETTINGS.set_evaluation_date(REFERENCE)


def _matrix_surface():
    return CapFloorTermVolSurface(
        REFERENCE,
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        STRIKES,
        VOLS,
        DayCounter.actual365_fixed(),
    )


def _quote_surface():
    """The surface over a distinct ``SimpleQuote`` per node, plus those quotes."""
    quotes = [[SimpleQuote(vol) for vol in row] for row in VOLS]
    surface = CapFloorTermVolSurface.with_quotes(
        REFERENCE,
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        STRIKES,
        quotes,
        DayCounter.actual365_fixed(),
    )
    return surface, quotes


def _moving_matrix_surface():
    """The floating-reference surface over fixed volatilities, at zero settlement
    days so its reference date is the evaluation date."""
    return CapFloorTermVolSurface.moving(
        0,
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        STRIKES,
        VOLS,
        DayCounter.actual365_fixed(),
        SETTINGS,
    )


def _moving_quote_surface():
    """The floating-reference surface over a distinct quote per node, plus those
    quotes."""
    quotes = [[SimpleQuote(vol) for vol in row] for row in VOLS]
    surface = CapFloorTermVolSurface.moving_with_quotes(
        0,
        Calendar.target(),
        BDC,
        OPTION_TENORS,
        STRIKES,
        quotes,
        DayCounter.actual365_fixed(),
        SETTINGS,
    )
    return surface, quotes


def _year_fraction(start, end):
    """Actual/365F between two ``Date``s, computed off Python's own calendar."""
    days = datetime.date(end.year, end.month, end.day) - datetime.date(
        start.year, start.month, start.day
    )
    return days.days / 365.0


@pytest.mark.parametrize(
    "build",
    [
        _matrix_surface,
        lambda: _quote_surface()[0],
        _moving_matrix_surface,
        lambda: _moving_quote_surface()[0],
    ],
)
def test_every_node_vol_comes_back(build):
    surface = build()
    for i, tenor in enumerate(OPTION_TENORS):
        for j, strike in enumerate(STRIKES):
            got = surface.volatility(tenor, strike)
            assert got == pytest.approx(VOLS[i][j], abs=TOL), f"node ({i},{j})"


@pytest.mark.parametrize("build", [_quote_surface, _moving_quote_surface])
def test_a_quote_bump_refreshes_only_its_own_node(build):
    surface, quotes = build()
    assert surface.volatility(OPTION_TENORS[0], STRIKES[0]) == pytest.approx(
        VOLS[0][0], abs=TOL
    )

    quotes[0][0].set_value(BUMPED)

    assert surface.volatility(OPTION_TENORS[0], STRIKES[0]) == pytest.approx(
        BUMPED, abs=TOL
    ), "the bumped node must serve the new vol"
    assert surface.volatility(OPTION_TENORS[0], STRIKES[1]) == pytest.approx(
        VOLS[0][1], abs=TOL
    ), "the untouched neighbour must not move"


def test_the_three_query_forms_agree_off_the_nodes():
    surface = _matrix_surface()
    end_date = Calendar.target().advance(REFERENCE, 18, "Months", BDC, False)

    by_tenor = surface.volatility(OFF_NODE_TENOR, OFF_NODE_STRIKE)
    by_date = surface.volatility_date(end_date, OFF_NODE_STRIKE)
    by_time = surface.volatility_time(
        _year_fraction(REFERENCE, end_date), OFF_NODE_STRIKE
    )

    assert by_tenor == by_date
    assert by_tenor == by_time

    corners = [VOLS[0][1], VOLS[0][2], VOLS[1][1], VOLS[1][2]]
    assert min(corners) <= by_tenor <= max(corners)


def test_a_ragged_grid_is_rejected():
    with pytest.raises(ItofinError):
        CapFloorTermVolSurface(
            REFERENCE,
            Calendar.target(),
            BDC,
            OPTION_TENORS,
            STRIKES,
            [VOLS[0], VOLS[1][:3], VOLS[2], VOLS[3]],
            DayCounter.actual365_fixed(),
        )
