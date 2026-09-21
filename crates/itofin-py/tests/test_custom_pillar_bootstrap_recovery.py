"""Custom-pillar helper failures stay fatal under approximate bootstrap options."""

import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FraRateHelper, IterativeBootstrapOptions, PiecewiseLogLinearDiscount, Pillar,
)
from itofin.time import Date, DayCounter, Period


def _date(value):
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def test_custom_pillar_errors_propagate_through_approximate_bootstrap_and_recover():
    oracle = json.loads(
        (Path(__file__).parents[3] / "sdk/go/testdata/custom_pillars.json").read_text()
    )
    row = next(case for case in oracle["yield"] if case["kind"] == 0)
    today = Date(15, 6, 2026)
    target = _date(row["latest_relevant"])

    def build(options):
        settings = Settings()
        settings.set_evaluation_date(today)
        index = Euribor(Period(3, "Months"), None, settings)
        helper = FraRateHelper(
            SimpleQuote(0.03), Period(3, "Months"), index,
            pillar=Pillar.CustomDate, custom_pillar_date=_date(row["pillar"]),
        )
        curve = PiecewiseLogLinearDiscount(
            today, [helper], DayCounter.actual365_fixed(), iterative_options=options,
        )
        return settings, helper, curve

    _, _, strict = build(None)
    expected = strict.discount_date(target, True)
    assert math.isfinite(expected)
    assert expected == pytest.approx(row["discount"], abs=1e-12, rel=0)
    settings, helper, curve = build(
        IterativeBootstrapOptions(max_attempts=2, dont_throw=True)
    )
    original = curve.discount_date(target, True)
    assert original == pytest.approx(expected, abs=1e-12, rel=0)
    assert helper.implied_quote() == pytest.approx(0.03, abs=1e-12, rel=0)

    settings.set_evaluation_date(Date(15, 6, 2028))
    with pytest.raises(ItofinError, match="pillar date.*earliest date"):
        helper.implied_quote()
    with pytest.raises(ItofinError, match="pillar date.*earliest date"):
        curve.discount_date(target, True)
    with pytest.raises(ItofinError, match="pillar date.*earliest date"):
        curve.discount_date(target, True)

    settings.set_evaluation_date(today)
    recovered = curve.discount_date(target, True)
    assert math.isfinite(recovered)
    assert recovered == pytest.approx(original, abs=1e-12, rel=0)
    assert helper.implied_quote() == pytest.approx(0.03, abs=1e-12, rel=0)
