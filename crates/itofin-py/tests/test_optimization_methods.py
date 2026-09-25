"""Calibration fixtures; set ITOFIN_RELEASE_PARITY=1 with a release wheel for exact Go vectors."""

import math
import os
import struct
from typing import Any

import pytest

from itofin import ItofinError
from itofin.indexes import Euribor
from itofin.models import CalibrationErrorType, CapHelper, HestonModelHelper, HullWhite
from itofin.optimization import ConjugateGradient, EndCriteria, Simplex, SteepestDescent
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, VolatilityType
from itofin.time import Calendar, Date, DayCounter, Frequency, Period

from test_heston_calibration import MATURITIES, _fixture_settings, _seed_model


STRIKE_BITS = [
    [0x3FEFAC53E80821CF, 0x3FEEC1D93A138C3F, 0x3FEDDE266B06EDCB],
    [0x3FEEE6FC4E5C066B, 0x3FEDAD1EA01315B6, 0x3FEC7FB4CB280859],
    [0x3FEDFC74E7AB4083, 0x3FEC86124A9AED52, 0x3FEB21F1FA31573D],
    [0x3FEB422C40CCEB6B, 0x3FE96481FC737590, 0x3FE7A78A19DF52FA],
    [0x3FE8A167781BF00B, 0x3FE6938B2FEF7DA4, 0x3FE4B18A04B47AB2],
    [0x3FE632E5C3C88016, 0x3FE4128A7C687E73, 0x3FE22653D92D71BF],
    [0x3FDD08FC2BC78913, 0x3FD92E6FB34BCD0D, 0x3FD5D6D3FBC52310],
]


def helpers(settings):
    """Build the exact strike-bit Heston market used by the Go calibration gate."""
    reference = Date(15, 1, 2026)
    calendar = Calendar.null_calendar()
    day_counter = DayCounter.actual360()
    return [
        HestonModelHelper(
            Period(length, unit),
            calendar,
            1.0,
            struct.unpack(">d", strike.to_bytes(8, "big"))[0],
            0.1,
            0.04,
            0.50,
            CalibrationErrorType.RelativePriceError,
            reference,
            day_counter,
            settings,
        )
        for (length, unit), row in zip(MATURITIES, STRIKE_BITS)
        for strike in row
    ]


@pytest.mark.parametrize(
    "factory,expected",
    [
        (
            Simplex,
            [0.01000000000107512, 0.1483535700303218, 0.01000000000679777, 5.030267915404000e-11, -0.4546235848312395],
        ),
        (
            ConjugateGradient,
            [0.008655748461209384, 0.20302231484372477, 0.03241055121479137, 0.05583023664447662, -0.36675046501506725],
        ),
        (
            SteepestDescent,
            [0.009279646471165427, 0.2003282187710104, 0.01137238386842711, 0.27757131213875286, -0.708882936730623],
        ),
    ],
)
def test_heston_methods_match_go_and_core(factory, expected):
    """Match release-profile Go vectors or improve the same market in CI."""
    settings = _fixture_settings()
    model = _seed_model()(0.3)
    method = factory(0.1) if factory is Simplex else factory()
    instruments = helpers(settings)
    model.calibrate(instruments, method, EndCriteria(400, 40, 1e-8, 1e-8, 1e-8), 96)
    actual = [model.v0(), model.kappa(), model.theta(), model.sigma(), model.rho()]
    assert all(math.isfinite(value) for value in actual)
    assert actual[0] >= 0 and actual[2] >= 0 and actual[3] >= 0
    assert -1 <= actual[4] <= 1
    error = sum(instrument.calibration_error() ** 2 for instrument in instruments)
    assert math.isfinite(error)
    if factory is Simplex or os.getenv("ITOFIN_RELEASE_PARITY") == "1":
        assert actual == pytest.approx(expected, rel=0, abs=1e-12)
    else:
        early_instruments = helpers(_fixture_settings())
        early_model = _seed_model()(0.3)
        early_model.calibrate(
            early_instruments,
            factory(),
            EndCriteria(10, 2, 1e-8, 1e-8, 1e-8),
            96,
        )
        early_error = sum(instrument.calibration_error() ** 2 for instrument in early_instruments)
        assert error < early_error


def test_simplex_rejects_invalid_characteristic_lengths():
    """Reject invalid scales before the core constructor can panic."""
    for length in (0, -1, math.nan, math.inf, -math.inf):
        with pytest.raises(ValueError):
            Simplex(length)


def test_calibration_rejects_other_python_objects():
    """Report a type error for unsupported methods and a core error for no helpers."""
    model = _seed_model()(0.3)
    wrong_method: Any = object()
    with pytest.raises(TypeError, match="method must be"):
        model.calibrate([], wrong_method, EndCriteria(400, 40, 1e-8, 1e-8, 1e-8), 96)
    with pytest.raises(ItofinError):
        model.calibrate([], Simplex(0.1), EndCriteria(400, 40, 1e-8, 1e-8, 1e-8), 96)


@pytest.mark.parametrize("factory", [Simplex, ConjugateGradient, SteepestDescent])
def test_all_calibration_entrypoints_accept_each_method(factory):
    """Dispatch all methods through each Heston and Hull-White calibration path."""
    method = factory(0.1) if factory is Simplex else factory()
    criteria = EndCriteria(400, 40, 1e-8, 1e-8, 1e-8)
    heston = _seed_model()(0.3)
    reference = Date(15, 1, 2026)
    curve = FlatForward(reference, 0.03, DayCounter.actual365_fixed())
    hull_white = HullWhite(curve, 0.05, 0.01)
    for calibrate in (
        lambda: heston.calibrate([], method, criteria, 96),
        lambda: heston.calibrate_cos([], method, criteria),
        lambda: heston.calibrate_exponential_fitting([], method, criteria),
        lambda: hull_white.calibrate([], method, criteria, False),
        lambda: hull_white.calibrate_caps([], method, criteria, True, 30),
    ):
        with pytest.raises(ItofinError):
            calibrate()


def test_simplex_calibrates_hull_white_caps():
    """Fit a normal cap helper strip with Simplex and keep reversion fixed."""
    settings = _fixture_settings()
    reference = Date(15, 1, 2026)
    day_counter = DayCounter.actual365_fixed()
    curve = FlatForward(reference, 0.03, day_counter)
    index = Euribor.six_months(curve, settings)
    quote = SimpleQuote(0.01)
    helpers = [
        CapHelper(
            Period(years, "Years"),
            quote,
            index,
            Frequency.Annual,
            day_counter,
            False,
            curve,
            CalibrationErrorType.RelativePriceError,
            VolatilityType.Normal,
            0.0,
        )
        for years in (2, 3, 4, 5)
    ]
    model = HullWhite(curve, 0.05, 0.01)
    model.calibrate_caps(helpers, Simplex(0.01), EndCriteria(1000, 100, 1e-6, 1e-8, 1e-8), True, 30)
    assert model.a() == 0.05
    assert math.isfinite(model.sigma()) and model.sigma() > 0
    assert all(math.isfinite(helper.calibration_error()) for helper in helpers)
