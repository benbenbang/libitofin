"""Additional matrix constructors preserve nodes, quotes and reference-date updates."""

import csv
import gc
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import SwaptionVolatilityMatrix, VolatilityType
from itofin.time import Calendar, Date, DayCounter, Period

from test_swaption_vol_matrix import BDC, EVAL, OPTION_TENORS, SWAP_TENORS, VOLS

FIXTURE = Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/swaption_matrix/quantlib.csv"


@pytest.mark.parametrize("form", [0, 1, 2, 3, 4])
def test_extra_constructors_match_independent_nodes_and_retain_inputs(form):
    settings = Settings()
    settings.set_evaluation_date(EVAL)
    calendar = Calendar.target()
    dc = DayCounter.actual365_fixed()
    quotes = [[SimpleQuote(value) for value in row] for row in VOLS]
    if form == 0:
        surface = SwaptionVolatilityMatrix.moving(
            calendar, BDC, OPTION_TENORS, SWAP_TENORS, quotes, dc, VolatilityType.ShiftedLognormal, settings
        )
    elif form == 1:
        surface = SwaptionVolatilityMatrix.fixed_quotes(
            EVAL, calendar, BDC, OPTION_TENORS, SWAP_TENORS, quotes, dc, VolatilityType.ShiftedLognormal
        )
    elif form == 2:
        surface = SwaptionVolatilityMatrix.moving_matrix(
            calendar, BDC, OPTION_TENORS, SWAP_TENORS, VOLS, dc, VolatilityType.ShiftedLognormal, settings
        )
    elif form == 3:
        surface = SwaptionVolatilityMatrix(
            EVAL, calendar, BDC, OPTION_TENORS, SWAP_TENORS, VOLS, dc, VolatilityType.ShiftedLognormal
        )
    else:
        dates = [
            calendar.advance(EVAL, n, unit, BDC, False)
            for n, unit in [(1, "Months"), (6, "Months"), (1, "Years"), (5, "Years"), (10, "Years"), (30, "Years")]
        ]
        surface = SwaptionVolatilityMatrix.with_option_dates(
            EVAL, calendar, BDC, dates, SWAP_TENORS, VOLS, dc, VolatilityType.ShiftedLognormal
        )
    with FIXTURE.open() as stream:
        rows = [row for row in csv.reader(stream) if row[0] == "node" and int(row[1]) == form]
    assert len(rows) == 24
    for _, _, i, j, vol, *_ in rows:
        assert surface.volatility(OPTION_TENORS[int(i)], SWAP_TENORS[int(j)], 0.02) == pytest.approx(
            float(vol), rel=0, abs=1e-16
        )
    with FIXTURE.open() as stream:
        observation = next(row for row in csv.reader(stream) if row[0] == "observe" and int(row[1]) == form)
    option_date = Date(15, 7, 2026)
    assert surface.volatility_date(option_date, 1.0, 0.02) == pytest.approx(float(observation[2]), rel=0, abs=1e-16)
    settings.set_evaluation_date(Date(15, 6, 2025))
    assert surface.volatility_date(option_date, 1.0, 0.02) == pytest.approx(float(observation[3]), rel=0, abs=1e-16)
    settings.set_evaluation_date(EVAL)
    if form in (0, 1):
        quotes[0][0].set_value(0.2)
    del quotes, calendar, dc
    gc.collect()
    assert surface.volatility(OPTION_TENORS[0], SWAP_TENORS[0], 0.02) == pytest.approx(
        0.2 if form in (0, 1) else 0.13, rel=0, abs=1e-16
    )
    before = surface.black_variance(OPTION_TENORS[0], SWAP_TENORS[0], 0.02)
    settings.set_evaluation_date(Date(15, 7, 2026))
    after = surface.black_variance(OPTION_TENORS[0], SWAP_TENORS[0], 0.02)
    if form in (0, 2):
        assert before != after
    else:
        assert before == after


def test_explicit_option_dates_reject_unsorted_axis():
    with pytest.raises(ItofinError):
        SwaptionVolatilityMatrix.with_option_dates(
            EVAL,
            Calendar.target(),
            BDC,
            [Date(15, 6, 2028), Date(15, 6, 2027)],
            [Period(1, "Years"), Period(5, "Years")],
            [[0.1, 0.2], [0.3, 0.4]],
            DayCounter.actual365_fixed(),
            VolatilityType.ShiftedLognormal,
        )
