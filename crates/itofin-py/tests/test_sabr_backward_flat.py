"""QuantLib 1.43 oracle and live ownership contract for SABR interpolation."""

import csv
import gc
from pathlib import Path
from typing import Any, TypedDict, cast

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Currency, Euribor, SwapIndex
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, SabrSwaptionVolatilityCube, SwaptionVolatilityMatrix, VolatilityType
from itofin.time import Calendar, Date, DayCounter, Period
from test_sabr_swaption_cube import (
    ATM_OPTION_TENORS, ATM_SWAP_TENORS, ATM_VOLS, BDC, EVAL, OPTION_TENORS,
    PARAMETERS_GUESS, STRIKE_SPREADS, SWAP_TENORS, VOL_SPREADS,
)


class CubeInputs(TypedDict):
    atm_vol: SwaptionVolatilityMatrix
    option_tenors: list[Period]
    swap_tenors: list[Period]
    strike_spreads: list[float]
    vol_spreads: list[list[SimpleQuote]]
    swap_index_base: SwapIndex
    short_swap_index_base: SwapIndex
    parameters_guess: list[list[SimpleQuote]]
    is_parameter_fixed: list[bool]
    is_atm_calibrated: bool
    settings: Settings


def cube_inputs(dense: bool) -> CubeInputs:
    settings = Settings()
    settings.set_evaluation_date(EVAL)
    curve = FlatForward(EVAL, 0.05, DayCounter.actual360())
    ibor = Euribor.six_months(curve, settings)

    def index(years):
        return SwapIndex(
            "EuriborSwapIsdaFixA", Period(years, "Years"), 2, Currency.eur(),
            Calendar.target(), Period(1, "Years"), BDC,
            DayCounter.thirty360_bond_basis(), ibor, settings,
        )

    quotes = [[SimpleQuote(v) for v in row] for row in VOL_SPREADS]
    atm = SwaptionVolatilityMatrix.moving(
        Calendar.target(), BDC, ATM_OPTION_TENORS, ATM_SWAP_TENORS,
        [[SimpleQuote(v) for v in row] for row in ATM_VOLS],
        DayCounter.actual365_fixed(), VolatilityType.ShiftedLognormal, settings,
    )
    return CubeInputs(
        atm_vol=atm, option_tenors=OPTION_TENORS, swap_tenors=SWAP_TENORS,
        strike_spreads=STRIKE_SPREADS, vol_spreads=quotes,
        swap_index_base=index(2), short_swap_index_base=index(1),
        parameters_guess=[[SimpleQuote(v) for v in PARAMETERS_GUESS] for _ in VOL_SPREADS],
        is_parameter_fixed=[False] * 4, is_atm_calibrated=dense, settings=settings,
    )


def value(cube):
    return cube.volatility(Period(2, "Years"), Period(5, "Years"), 0.05, False)


def test_backward_flat_quantlib_oracle_and_default():
    path = Path(__file__).parents[2] / "libitofin/tests/fixtures/sabr_backward_flat/oracle.csv"
    with path.open() as stream:
        rows = list(csv.DictReader(stream))
    assert len(rows) == 72
    for dense in (False, True):
        inputs = cube_inputs(dense)
        cubes = [SabrSwaptionVolatilityCube(**inputs, backward_flat=flag) for flag in (False, True)]
        default = SabrSwaptionVolatilityCube(**inputs)
        positional = SabrSwaptionVolatilityCube(
            inputs["atm_vol"], inputs["option_tenors"], inputs["swap_tenors"],
            inputs["strike_spreads"], inputs["vol_spreads"], inputs["swap_index_base"],
            inputs["short_swap_index_base"], inputs["parameters_guess"], inputs["is_parameter_fixed"],
            inputs["is_atm_calibrated"], inputs["settings"], False, False, 50, 0.0001, True,
        )
        del inputs
        gc.collect()
        for row in rows:
            if bool(int(row["is_atm_calibrated"])) != dense:
                continue
            option = Period(int(row["option_tenor"][:-1]), "Years")
            swap = Period(int(row["swap_tenor"][:-1]), "Years")
            strike = float(row["strike"])
            for flag, cube in enumerate(cubes):
                expected = float(row["vol_backward_flat" if flag else "vol_bilinear"])
                assert abs(cube.volatility(option, swap, strike, True) - expected) < 1e-6
            assert default.volatility(option, swap, strike, True) == cubes[0].volatility(option, swap, strike, True)
            assert positional.volatility(option, swap, strike, True) == cubes[1].volatility(option, swap, strike, True)


@pytest.mark.parametrize("dense", [False, True])
def test_backward_flat_survives_live_recalibration(dense):
    inputs = cube_inputs(dense)
    cube = SabrSwaptionVolatilityCube(**inputs, backward_flat=True)

    def verify():
        got = value(cube)
        assert abs(got - value(SabrSwaptionVolatilityCube(**inputs, backward_flat=True))) < 1e-14
        assert abs(got - value(SabrSwaptionVolatilityCube(**inputs, backward_flat=False))) > 1e-3
        return got

    initial = verify()
    inputs["vol_spreads"][3][0].set_value(VOL_SPREADS[3][0] + 0.01)
    after_quote = verify()
    assert abs(after_quote - initial) > 1e-7
    inputs["settings"].set_evaluation_date(Date(16, 6, 2026))
    after_date = verify()
    assert abs(after_date - after_quote) > 1e-10
    cold = SabrSwaptionVolatilityCube(**inputs, backward_flat=True)
    del inputs
    gc.collect()
    assert value(cube) == after_date
    assert value(cold) == after_date


def test_backward_flat_rejects_invalid_inputs_without_poisoning():
    inputs = cube_inputs(False)
    for invalid in (2, -1, "true", None):
        with pytest.raises(TypeError):
            SabrSwaptionVolatilityCube(**inputs, backward_flat=cast(Any, invalid))
    for option in (False, True):
        bad = inputs.copy()
        if option:
            bad["option_tenors"] = inputs["option_tenors"][:1]
        else:
            bad["swap_tenors"] = inputs["swap_tenors"][:1]
        bad["vol_spreads"] = inputs["vol_spreads"][:3]
        bad["parameters_guess"] = inputs["parameters_guess"][:3]
        with pytest.raises(ItofinError):
            SabrSwaptionVolatilityCube(**bad, backward_flat=True)
    assert value(SabrSwaptionVolatilityCube(**inputs, backward_flat=True)) > 0
