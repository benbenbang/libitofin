"""Oracle for the interpolated (spread) swaption vol cube facade (issue #614).

The fixture is the core module test's, in turn QuantLib's
``swaptionvolstructuresutilities.hpp`` data: the ``AtmVolatility`` 6x4 grid as a
moving ``SwaptionVolatilityMatrix`` (TARGET, ModifiedFollowing, Actual/365F),
the ``VolatilityCube`` 3x3 node by 5 strike-spread grid over it, and two
hand-built ``EuriborSwapIsdaFixA``-convention swap indexes (long 2Y over 6M
Euribor, short 1Y over 3M Euribor) off a 5% flat curve, all against evaluation
date 15-June-2026.

``VOL_SPREADS`` is row-major over the ``(option tenor, swap tenor)`` nodes, the
ordering the facade documents and validates: row ``i * len(SWAP_TENORS) + j`` is
the smile at ``(OPTION_TENORS[i], SWAP_TENORS[j])``.

Four arms:

A. ATM recovery at strike spread 0 (C++ ``makeAtmVolTest``). At the cube's own
   at-the-money strike the interpolated spread is the input 0.0000 column, so
   the cube must serve exactly what the ATM matrix serves. This is what pins
   that ``atm_strike_from_tenor`` reads the same forward the smile is centred
   on: a facade that routed it through the wrong base index would shift the
   query off the zero column and fail here.
B. Vol-spread recovery at every smile node (C++ ``makeVolSpreadsTest``). At
   ``atm_strike + spread`` the served vol minus the ATM vol must be the input
   quote, for all 9 nodes x 5 spreads. This is the arm that pins the row-major
   ordering end to end: a transposed grid still recovers arm A (the zero column
   is symmetric) but scrambles 24 of these 45 values (the three diagonal nodes
   are transpose-fixed, and the zero column matches on the six swapped ones).
C. Quote-bump refresh, mirroring the core's third arm. Bumping one node's
   spread quote must move that node's smile at that strike, leave the same
   node's other strikes alone, and leave a different node alone - the
   per-strike interpolators are rebuilt from the quotes rather than served
   stale, and the rebuild writes only where the bump was.
D. Engine integration. The same swaption, struck 50bp above the money so the
   smile is doing work, priced on the cube and on the bare ATM matrix. The
   engine reads the vol at the swaption's own strike, so the cube must serve the
   ATM vol plus a nonzero interpolated spread there and the two NPVs must
   differ. Both remain positive.

The core asserts 1e-16 on arms A-C; this pass keeps 1e-12. Every query passes
``extrapolate=True``, as the core test does: the 30Y option tenor sits on the
ATM grid's last node, so the default would range-check against it.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Currency, Euribor, SwapIndex
from itofin.instruments import EuropeanExercise, SettlementMethod, SettlementType, Swaption, SwapType, VanillaSwap
from itofin.pricingengines import BlackSwaptionEngine, CashAnnuityModel
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    InterpolatedSwaptionVolatilityCube,
    SwaptionVolatilityMatrix,
    VolatilityType,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

EVAL = Date(15, 6, 2026)
BDC = BusinessDayConvention.ModifiedFollowing
TOL = 1e-12
RATE = 0.05

ATM_OPTION_TENORS = [
    Period(1, "Months"),
    Period(6, "Months"),
    Period(1, "Years"),
    Period(5, "Years"),
    Period(10, "Years"),
    Period(30, "Years"),
]
ATM_SWAP_TENORS = [
    Period(1, "Years"),
    Period(5, "Years"),
    Period(10, "Years"),
    Period(30, "Years"),
]
ATM_VOLS = [
    [0.1300, 0.1560, 0.1390, 0.1220],
    [0.1440, 0.1580, 0.1460, 0.1260],
    [0.1600, 0.1590, 0.1470, 0.1290],
    [0.1640, 0.1470, 0.1370, 0.1220],
    [0.1400, 0.1300, 0.1250, 0.1100],
    [0.1130, 0.1090, 0.1070, 0.0930],
]

OPTION_TENORS = [Period(1, "Years"), Period(10, "Years"), Period(30, "Years")]
SWAP_TENORS = [Period(2, "Years"), Period(10, "Years"), Period(30, "Years")]
STRIKE_SPREADS = [-0.020, -0.005, 0.000, 0.005, 0.020]
VOL_SPREADS = [
    [0.0599, 0.0049, 0.0000, -0.0001, 0.0127],
    [0.0729, 0.0086, 0.0000, -0.0024, 0.0098],
    [0.0738, 0.0102, 0.0000, -0.0039, 0.0065],
    [0.0465, 0.0063, 0.0000, -0.0032, -0.0010],
    [0.0558, 0.0084, 0.0000, -0.0050, -0.0057],
    [0.0576, 0.0083, 0.0000, -0.0043, -0.0014],
    [0.0437, 0.0059, 0.0000, -0.0030, -0.0006],
    [0.0533, 0.0078, 0.0000, -0.0045, -0.0046],
    [0.0545, 0.0079, 0.0000, -0.0042, -0.0020],
]

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)

BUMP = 0.0100
BUMPED_NODE = len(SWAP_TENORS) + 1

ENGINE_OPTION_TENOR = Period(2, "Years")
ENGINE_SWAP_TENOR = Period(7, "Years")
ENGINE_MONEYNESS = 0.005
EXERCISE = Calendar.target().advance(EVAL, 2, "Years", BDC, False)
SWAP_END = Date(EXERCISE.day, EXERCISE.month, EXERCISE.year + 7)


def _fixture():
    """The curve, ATM matrix, cube and the cube's own spread quotes.

    Rebuilt per arm: arm C mutates its quotes, and a shared cube would let a
    later arm read the bumped smile.
    """
    curve = FlatForward(EVAL, RATE, DayCounter.actual365_fixed())
    atm = SwaptionVolatilityMatrix.moving(
        Calendar.target(),
        BDC,
        ATM_OPTION_TENORS,
        ATM_SWAP_TENORS,
        [[SimpleQuote(vol) for vol in row] for row in ATM_VOLS],
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        SETTINGS,
    )
    quotes = [[SimpleQuote(spread) for spread in row] for row in VOL_SPREADS]
    cube = InterpolatedSwaptionVolatilityCube(
        atm,
        OPTION_TENORS,
        SWAP_TENORS,
        STRIKE_SPREADS,
        quotes,
        _swap_index(Period(2, "Years"), Euribor.six_months(curve, SETTINGS)),
        _swap_index(Period(1, "Years"), Euribor.three_months(curve, SETTINGS)),
        SETTINGS,
    )
    return curve, atm, cube, quotes


def _swap_index(tenor, ibor_index):
    """An EuriborSwapIsdaFixA-convention index: annual 30/360 bond-basis fixed
    leg on TARGET, 2 settlement days, over the given forecasting index."""
    return SwapIndex(
        "EuriborSwapIsdaFixA",
        tenor,
        2,
        Currency.eur(),
        Calendar.target(),
        Period(1, "Years"),
        BDC,
        DayCounter.thirty360_bond_basis(),
        ibor_index,
        SETTINGS,
    )


def test_the_cube_recovers_the_atm_vols_at_strike_spread_zero():
    _curve, atm, cube, _quotes = _fixture()
    for option_tenor in OPTION_TENORS:
        for swap_tenor in SWAP_TENORS:
            strike = cube.atm_strike_from_tenor(option_tenor, swap_tenor)
            expected = atm.volatility(option_tenor, swap_tenor, strike, True)
            got = cube.volatility(option_tenor, swap_tenor, strike, True)
            assert got == pytest.approx(expected, abs=TOL), f"{option_tenor} x {swap_tenor}"


def test_the_cube_recovers_the_input_vol_spreads_at_every_smile_node():
    _curve, atm, cube, _quotes = _fixture()
    for i, option_tenor in enumerate(OPTION_TENORS):
        for j, swap_tenor in enumerate(SWAP_TENORS):
            atm_strike = cube.atm_strike_from_tenor(option_tenor, swap_tenor)
            atm_vol = atm.volatility(option_tenor, swap_tenor, atm_strike, True)
            inputs = VOL_SPREADS[i * len(SWAP_TENORS) + j]
            for k, strike_spread in enumerate(STRIKE_SPREADS):
                served = cube.volatility(
                    option_tenor, swap_tenor, atm_strike + strike_spread, True
                )
                assert served - atm_vol == pytest.approx(inputs[k], abs=TOL), (
                    f"{option_tenor} x {swap_tenor} at strike spread {strike_spread}"
                )


def test_a_vol_spread_quote_bump_refreshes_the_smile():
    _curve, atm, cube, quotes = _fixture()
    inputs = VOL_SPREADS[BUMPED_NODE]

    def spread_at(node, i, j, k):
        option_tenor = OPTION_TENORS[i]
        swap_tenor = SWAP_TENORS[j]
        atm_strike = cube.atm_strike_from_tenor(option_tenor, swap_tenor)
        atm_vol = atm.volatility(option_tenor, swap_tenor, atm_strike, True)
        served = cube.volatility(
            option_tenor, swap_tenor, atm_strike + STRIKE_SPREADS[k], True
        )
        assert node == i * len(SWAP_TENORS) + j
        return served - atm_vol

    assert spread_at(BUMPED_NODE, 1, 1, 0) == pytest.approx(inputs[0], abs=TOL)

    quotes[BUMPED_NODE][0].set_value(inputs[0] + BUMP)

    assert spread_at(BUMPED_NODE, 1, 1, 0) == pytest.approx(inputs[0] + BUMP, abs=TOL)
    assert spread_at(BUMPED_NODE, 1, 1, 3) == pytest.approx(inputs[3], abs=TOL), (
        "an untouched strike on the bumped node must be unchanged"
    )
    assert spread_at(0, 0, 0, 0) == pytest.approx(VOL_SPREADS[0][0], abs=TOL), (
        "a different node must be unchanged: a mis-indexed write would flood it"
    )


def _npv_on(surface, curve, strike):
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
        strike,
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
    swaption.set_black_engine(
        BlackSwaptionEngine(surface, curve, SETTINGS, CashAnnuityModel.SwapRate)
    )
    return swaption.npv()


def test_the_engine_prices_off_the_cubes_smile_not_the_atm_surface():
    curve, atm, cube, _quotes = _fixture()
    coordinates = (ENGINE_OPTION_TENOR, ENGINE_SWAP_TENOR)
    atm_strike = cube.atm_strike_from_tenor(*coordinates)
    strike = atm_strike + ENGINE_MONEYNESS

    cube_vol = cube.volatility(*coordinates, strike, True)
    atm_vol = atm.volatility(*coordinates, strike, True)
    assert abs(cube_vol - atm_vol) > 1e-6, (
        "the fixture must place the engine's strike where the smile is nonzero"
    )

    npv_cube = _npv_on(cube, curve, strike)
    npv_atm = _npv_on(atm, curve, strike)
    print(
        f"\natm strike at 2Yx7Y = {atm_strike!r}"
        f"\nvol on the cube     = {cube_vol!r}"
        f"\nvol on the matrix   = {atm_vol!r}"
        f"\nnpv on the cube     = {npv_cube!r}"
        f"\nnpv on the matrix   = {npv_atm!r}"
    )
    assert npv_cube > 0.0
    assert npv_atm > 0.0
    assert abs(npv_cube - npv_atm) > 1e-8


def test_the_cube_rejects_a_mis_shaped_vol_spread_grid():
    """The row count is the node count and the column count the strike-spread
    count; both are checked in the facade, before the core's dimension error."""
    _curve, atm, _cube, _quotes = _fixture()
    curve = FlatForward(EVAL, RATE, DayCounter.actual365_fixed())
    bases = (
        _swap_index(Period(2, "Years"), Euribor.six_months(curve, SETTINGS)),
        _swap_index(Period(1, "Years"), Euribor.three_months(curve, SETTINGS)),
    )

    def build(quotes):
        return InterpolatedSwaptionVolatilityCube(
            atm,
            OPTION_TENORS,
            SWAP_TENORS,
            STRIKE_SPREADS,
            quotes,
            *bases,
            SETTINGS,
        )

    full = [[SimpleQuote(spread) for spread in row] for row in VOL_SPREADS]
    with pytest.raises(ItofinError, match="one row per"):
        build(full[:-1])
    with pytest.raises(ItofinError, match="one column per strike spread"):
        build([row[:-1] for row in full])
