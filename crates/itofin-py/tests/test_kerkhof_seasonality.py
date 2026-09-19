"""Independent QuantLib Kerkhof factors and zero-curve integration."""

import csv
import gc
import math
from pathlib import Path

import pytest

from itofin import ItofinError
from itofin.termstructures import InterpolatedZeroInflationCurve, InterpolatedYoYInflationCurve, KerkhofSeasonality
from itofin.time import Date, DayCounter, Frequency

FACTORS = [1.20, 1.004, 0.997, 1.006, 0.995, 1.003, 0.991, 1.008, 0.998, 1.005, 0.996, 1.002]
FIXTURE = Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/kerkhof_seasonality.csv"


def test_kerkhof_matches_independent_factors_and_curve_rates():
    with FIXTURE.open() as stream:
        rows = list(csv.DictReader(stream))
    assert len(rows) == 72
    for row in rows:
        date = Date(1, 1, 1901) + (int(row["date"]) - 367)
        anchor = Date(1, 1, 1901) + (int(row["anchor"]) - 367)
        correction = KerkhofSeasonality(anchor, FACTORS)
        assert correction.seasonality_base_date() == anchor
        assert correction.frequency() == Frequency.Monthly
        assert correction.seasonality_factor(date) == pytest.approx(float(row["factor"]), rel=0, abs=1e-14)
        curve = InterpolatedZeroInflationCurve(
            Date(13, 8, 2007),
            [Date(1, 7, 2007), Date(1, 1, 2015)],
            [0.02, 0.035],
            Frequency.Monthly if row["frequency"] == "12" else Frequency.Quarterly,
            DayCounter.actual365_fixed() if row["daycounter"] == "A365" else DayCounter.thirty360_bond_basis(),
        )
        curve.set_seasonality(correction)
        copied = correction.seasonality_factors()
        copied[0] = 9.0
        assert correction.seasonality_factors() == FACTORS
        del correction
        gc.collect()
        if not math.isnan(float(row["curve"])):
            assert curve.zero_rate_date(date, True) == pytest.approx(float(row["curve"]), rel=0, abs=1e-12)


@pytest.mark.parametrize("factors", [[], [1.0] * 11, [1.0] * 13, [1.0] * 24])
def test_kerkhof_rejects_invalid_monthly_factors(factors):
    with pytest.raises(ItofinError):
        KerkhofSeasonality(Date(31, 1, 2007), factors)


def test_kerkhof_yoy_correction_is_rejected():
    curve = InterpolatedYoYInflationCurve(
        Date(13, 8, 2007),
        [Date(1, 7, 2007), Date(1, 1, 2015)],
        [0.02, 0.035],
        Frequency.Monthly,
        DayCounter.actual365_fixed(),
    )
    curve.set_seasonality(KerkhofSeasonality(Date(31, 1, 2007), FACTORS))
    with pytest.raises(ItofinError, match="YoY"):
        curve.yoy_rate_date(Date(1, 8, 2008))
