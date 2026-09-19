"""Pinned QuantLib 1.43 spread/upfront ISDA helper prices and quote recalibration."""

import csv
import gc
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import PricingModel
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, PiecewiseDefaultCurve, SpreadCdsHelper, UpfrontCdsHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period

FIXTURE = Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/isda_helpers/bindings.csv"


@pytest.mark.parametrize("kind", ["spread", "upfront"])
@pytest.mark.parametrize("rate,stages", [(0.03, [0, 1]), (0.04, [2])])
def test_isda_helpers_match_independent_prices_and_recalibrate(kind, rate, stages):
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    settings.set_include_todays_cash_flows(False)
    dc = DayCounter.actual365_fixed()
    discount = FlatForward(today, rate, dc)
    values = [0.005, 0.01, 0.015] if kind == "spread" else [0.01, 0.02, 0.04]
    quotes = [SimpleQuote(value) for value in values]
    helpers = []
    for years, quote in zip([1, 3, 5], quotes):
        args = [
            Period(years, "Years"),
            1,
            Calendar.target(),
            Frequency.Quarterly,
            BusinessDayConvention.Following,
            DateGeneration.CDS,
            DayCounter.actual360(),
            0.4,
            discount,
            settings,
        ]
        helper = (
            SpreadCdsHelper.with_terms(quote, *args, model=PricingModel.Isda)
            if kind == "spread"
            else UpfrontCdsHelper(quote, 0.01, *args, model=PricingModel.Isda)
        )
        helpers.append(helper)
    curve = PiecewiseDefaultCurve(today, helpers, dc)
    with FIXTURE.open() as stream:
        rows = list(csv.DictReader(stream))
    for stage in stages:
        if stage:
            quotes[1].set_value(values[1] + 0.002)
        expected = [row for row in rows if row["kind"] == kind and int(row["stage"]) == stage]
        assert len(expected) == 3
        for helper, row in zip(helpers, expected):
            pillar = helper.pillar_date()
            assert pillar == Date(1, 1, 1901) + (int(row["pillar_serial"]) - 367)
            assert curve.survival_probability_date(pillar) == pytest.approx(float(row["survival"]), rel=0, abs=1e-10)
            assert helper.implied_quote() == pytest.approx(float(row["implied_quote"]), rel=0, abs=1e-10)
    last = helpers[-1].pillar_date()
    survival = curve.survival_probability_date(last)
    del helpers, quotes, discount, helper, quote, args, dc, settings
    gc.collect()
    assert curve.survival_probability_date(last) == survival


def test_isda_helper_rejects_incompatible_curve_day_count():
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    dc = DayCounter.actual360()
    helper = SpreadCdsHelper.with_terms(
        SimpleQuote(0.01),
        Period(1, "Years"),
        1,
        Calendar.target(),
        Frequency.Quarterly,
        BusinessDayConvention.Following,
        DateGeneration.CDS,
        dc,
        0.4,
        FlatForward(today, 0.03, dc),
        settings,
        model=PricingModel.Isda,
    )
    curve = PiecewiseDefaultCurve(today, [helper], DayCounter.actual365_fixed())
    with pytest.raises(ItofinError, match="Act/365"):
        curve.calculate()
