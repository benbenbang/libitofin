"""Oracle for the SABR swaption vol cube and smile section facades (issue #615).

The fixture is the core module test's ``build_common_sabr_cube``, in turn
QuantLib's ``swaptionvolstructuresutilities.hpp`` data: the ``AtmVolatility``
6x4 grid as a moving ``SwaptionVolatilityMatrix``, the ``VolatilityCube`` 3x3
node by 5 strike-spread grid over it, per-node SABR guesses [0.2, 0.5, 0.4, 0.0]
with every parameter free and the ATM-calibration pass on, and two hand-built
``EuriborSwapIsdaFixA``-convention swap indexes off a 5% flat Actual/360 curve,
all against evaluation date 15-June-2026.

Both base indexes forecast off **6M** Euribor here, long 2Y and short 1Y. That
differs from the interpolated-cube fixture (#614), whose short base is over 3M
Euribor, and it is the core SABR fixture's choice: the short index sets the ATM
forward at the short swap tenors, so the wrong one moves the strikes the smile
is fitted at and quietly degrades the fit rather than failing loudly.

Four arms:

A. ATM recovery over the full 6x4 ATM grid (C++ ``makeAtmVolTest``, tolerance
   3e-4). The cube is not an interpolator here: every node's smile is a fitted
   SABR curve, so agreeing with the ATM surface at the money is what the dense
   ATM-calibration pass buys. Wire ``is_atm_calibrated`` false and this arm
   fails; the tolerance is the fit's, not a floating-point margin.
B. Vol-spread recovery over the 3x3 cube nodes x 5 strike spreads (C++
   ``makeVolSpreadsTest``, tolerance 12e-4), computed exactly as the core does:
   the served vol at ``atm_strike + spread`` minus the ATM vol at the ATM strike
   must be the input spread quote. This is the arm that pins the row-major
   ordering of both the vol-spread grid and the per-node parameter guesses.
C. ``SabrSmileSection`` on its own: a smile at known parameters is finite and
   positive at the forward, and the two paths deferred to core #586 (a normal
   volatility type, a non-zero shift) and an out-of-range beta are rejected at
   construction rather than silently accepted.
D. Engine integration - the flagship. The same swaption struck 50bp above the
   money, priced through ``BlackSwaptionEngine`` reading the calibrated cube and
   then the bare ATM matrix. The engine reads the vol at the swaption's own
   strike, so the fitted smile must move the NPV off the flat-ATM one.

Arms A and B reproduce the core test's worst errors exactly - 2.8951485188991044e-4
against 3e-4, and 1.9485699837217332e-4 against 12e-4 - so the fixture is the same
fixture and the same calibration, not merely a passing one. Arm A's margin is
thin because 3e-4 is QuantLib's own measured margin for this fit; a drift there
means the fixture has moved, and the fix is the fixture, not the tolerance.

The cube is calibrated once for the whole module: a Levenberg-Marquardt fit per
node plus the dense pass is the expensive part, and no arm mutates a quote.
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
    SabrSmileSection,
    SabrSwaptionVolatilityCube,
    SwaptionVolatilityMatrix,
    VolatilityType,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

EVAL = Date(15, 6, 2026)
BDC = BusinessDayConvention.ModifiedFollowing
RATE = 0.05
ATM_TOL = 3e-4
SPREAD_TOL = 12e-4

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
PARAMETERS_GUESS = [0.2, 0.5, 0.4, 0.0]

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)

ENGINE_OPTION_TENOR = Period(2, "Years")
ENGINE_SWAP_TENOR = Period(7, "Years")
ENGINE_MONEYNESS = 0.005
EXERCISE = Calendar.target().advance(EVAL, 2, "Years", BDC, False)
SWAP_END = Date(EXERCISE.day, EXERCISE.month, EXERCISE.year + 7)


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


@pytest.fixture(scope="module")
def sabr_cube():
    """The curve, the ATM matrix and the calibrated cube over it.

    Module-scoped: calibrating the nine nodes plus the dense ATM pass is the
    expensive part of this file, and no arm mutates a quote.
    """
    curve = FlatForward(EVAL, RATE, DayCounter.actual360())
    euribor6m = Euribor.six_months(curve, SETTINGS)
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
    cube = SabrSwaptionVolatilityCube(
        atm,
        OPTION_TENORS,
        SWAP_TENORS,
        STRIKE_SPREADS,
        [[SimpleQuote(spread) for spread in row] for row in VOL_SPREADS],
        _swap_index(Period(2, "Years"), euribor6m),
        _swap_index(Period(1, "Years"), euribor6m),
        [[SimpleQuote(guess) for guess in PARAMETERS_GUESS] for _ in VOL_SPREADS],
        [False, False, False, False],
        True,
        SETTINGS,
    )
    return curve, atm, cube


def test_the_calibrated_cube_recovers_the_atm_vols(sabr_cube):
    _curve, atm, cube = sabr_cube
    worst = 0.0
    for option_tenor in ATM_OPTION_TENORS:
        for swap_tenor in ATM_SWAP_TENORS:
            strike = cube.atm_strike_from_tenor(option_tenor, swap_tenor)
            expected = atm.volatility(option_tenor, swap_tenor, strike, True)
            served = cube.volatility(option_tenor, swap_tenor, strike, True)
            worst = max(worst, abs(expected - served))
    print(f"\nworst atm recovery error = {worst!r} (tolerance {ATM_TOL})")
    assert worst < ATM_TOL


def test_the_calibrated_cube_recovers_the_input_vol_spreads(sabr_cube):
    _curve, atm, cube = sabr_cube
    worst = 0.0
    for i, option_tenor in enumerate(OPTION_TENORS):
        for j, swap_tenor in enumerate(SWAP_TENORS):
            atm_strike = cube.atm_strike_from_tenor(option_tenor, swap_tenor)
            atm_vol = atm.volatility(option_tenor, swap_tenor, atm_strike, True)
            inputs = VOL_SPREADS[i * len(SWAP_TENORS) + j]
            for k, strike_spread in enumerate(STRIKE_SPREADS):
                served = cube.volatility(
                    option_tenor, swap_tenor, atm_strike + strike_spread, True
                )
                worst = max(worst, abs(inputs[k] - (served - atm_vol)))
    print(f"\nworst vol-spread recovery error = {worst!r} (tolerance {SPREAD_TOL})")
    assert worst < SPREAD_TOL


def test_a_sabr_smile_section_is_queryable_at_known_parameters():
    smile = SabrSmileSection(1.0, 0.039, 0.3, 0.6, 0.02, 0.01)
    assert smile.atm_level == pytest.approx(0.039)
    assert smile.exercise_time == pytest.approx(1.0)
    assert (smile.alpha, smile.beta, smile.nu, smile.rho) == (0.3, 0.6, 0.02, 0.01)

    at_the_money = smile.volatility(smile.atm_level)
    assert at_the_money > 0.0
    assert smile.variance(smile.atm_level) == pytest.approx(
        at_the_money**2 * smile.exercise_time
    )
    assert smile.volatility(0.05) > 0.0


def test_a_sabr_smile_section_rejects_the_deferred_and_invalid_inputs():
    """The two #586 deferrals (normal volatility, non-zero shift) and an
    out-of-range parameter are rejected at construction, not on query."""
    with pytest.raises(ItofinError, match="beta must be in"):
        SabrSmileSection(1.0, 0.039, 0.3, 2.0, 0.02, 0.01)
    with pytest.raises(ItofinError, match="#586"):
        SabrSmileSection(1.0, 0.039, 0.3, 0.6, 0.02, 0.01, shift=0.01)
    with pytest.raises(ItofinError, match="#586"):
        SabrSmileSection(
            1.0,
            0.039,
            0.3,
            0.6,
            0.02,
            0.01,
            volatility_type=VolatilityType.Normal,
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


def test_the_engine_prices_a_swaption_off_the_calibrated_sabr_smile(sabr_cube):
    curve, atm, cube = sabr_cube
    coordinates = (ENGINE_OPTION_TENOR, ENGINE_SWAP_TENOR)
    atm_strike = cube.atm_strike_from_tenor(*coordinates)
    strike = atm_strike + ENGINE_MONEYNESS

    cube_vol = cube.volatility(*coordinates, strike, True)
    atm_vol = atm.volatility(*coordinates, strike, True)
    assert abs(cube_vol - atm_vol) > 1e-6, (
        "the fixture must place the engine's strike where the fitted smile is "
        "away from the atm surface"
    )

    npv_cube = _npv_on(cube, curve, strike)
    npv_atm = _npv_on(atm, curve, strike)
    print(
        f"\natm strike at 2Yx7Y = {atm_strike!r}"
        f"\nvol on the sabr cube = {cube_vol!r}"
        f"\nvol on the matrix    = {atm_vol!r}"
        f"\nnpv on the sabr cube = {npv_cube!r}"
        f"\nnpv on the matrix    = {npv_atm!r}"
    )
    assert npv_cube > 0.0
    assert npv_atm > 0.0
    assert abs(npv_cube - npv_atm) > 1e-8


def test_the_cube_rejects_a_mis_shaped_guess_or_spread_grid(sabr_cube):
    """The vol-spread and guess grids are both row-major over the nodes, and the
    fixed-parameter flags are one per SABR parameter; all three shapes are
    checked in the facade."""
    _curve, atm, _cube = sabr_cube
    curve = FlatForward(EVAL, RATE, DayCounter.actual360())
    euribor6m = Euribor.six_months(curve, SETTINGS)
    bases = (
        _swap_index(Period(2, "Years"), euribor6m),
        _swap_index(Period(1, "Years"), euribor6m),
    )

    def build(spreads, guesses, fixed):
        return SabrSwaptionVolatilityCube(
            atm,
            OPTION_TENORS,
            SWAP_TENORS,
            STRIKE_SPREADS,
            spreads,
            *bases,
            guesses,
            fixed,
            True,
            SETTINGS,
        )

    spreads = [[SimpleQuote(spread) for spread in row] for row in VOL_SPREADS]
    guesses = [[SimpleQuote(guess) for guess in PARAMETERS_GUESS] for _ in VOL_SPREADS]
    flags = [False, False, False, False]

    with pytest.raises(ItofinError, match="one row per"):
        build(spreads[:-1], guesses, flags)
    with pytest.raises(ItofinError, match="one column per strike spread"):
        build([row[:-1] for row in spreads], guesses, flags)
    with pytest.raises(ItofinError, match="one column per SABR parameter"):
        build(spreads, [row[:-1] for row in guesses], flags)
    with pytest.raises(ItofinError, match="one flag per SABR parameter"):
        build(spreads, guesses, flags[:-1])
