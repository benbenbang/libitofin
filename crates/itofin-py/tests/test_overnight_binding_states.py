"""OIS state recovery and additive-spread updates against QuantLib 1.43."""

import json
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Estr
from itofin.instruments import MakeOis
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, OISRateHelper, PiecewiseLogLinearDiscount, RateAveraging
from itofin.time import BusinessDayConvention, Date, DayCounter, Frequency, Period

ORACLE = json.loads(
    (Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/overnight_averaging/bindings.json").read_text()
)
MODES = {"simple": RateAveraging.Simple, "compound": RateAveraging.Compound}


def _date(value):
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


@pytest.mark.parametrize("mode", MODES)
def test_ois_today_forecast_recovers_after_missing_past_fixing(mode):
    """The same swap recovers after its missing fixing becomes today's forecast."""
    assert ORACLE["quantlib"] == "1.43"
    case = ORACLE["swap_cases"][0]
    assert case["name"] == "today_start"
    inputs, expected = case["inputs"], case["results"][mode]
    settings = Settings()
    settings.set_evaluation_date(_date(inputs["evaluation_date"]))
    reference = _date(inputs["reference_date"])
    forward = FlatForward(reference, inputs["forward_rate"], DayCounter.actual365_fixed())
    discount = FlatForward(reference, inputs["discount_rate"], DayCounter.actual365_fixed())
    swap = MakeOis(
        Period(inputs["tenor_years"], "Years"), Estr(forward, settings), settings,
        fixed_rate=inputs["fixed_rate"], effective_date=_date(inputs["effective_date"]),
        nominal=inputs["nominal"], payment_lag=inputs["payment_lag"],
        discounting_term_structure=discount, averaging_method=MODES[mode],
    ).build()
    assert swap.npv() == pytest.approx(expected["npv"], abs=1e-7, rel=0)
    assert swap.fair_rate() == pytest.approx(expected["fair_rate"], abs=1e-12, rel=0)
    assert swap.is_calculated()
    settings.set_evaluation_date(Date(8, 7, 2026))
    assert not swap.is_calculated()
    with pytest.raises(ItofinError, match="[Mm]issing.*fixing"):
        swap.npv()
    assert not swap.is_calculated()
    settings.set_evaluation_date(_date(inputs["evaluation_date"]))
    assert swap.npv() == pytest.approx(expected["npv"], abs=1e-7, rel=0)
    assert swap.fair_rate() == pytest.approx(expected["fair_rate"], abs=1e-12, rel=0)


@pytest.mark.parametrize("mode", MODES)
def test_ois_additive_spread_quote_updates_rebootstrap_existing_curve(mode):
    """A live spread quote changes the fitted discount, with both modes retained."""
    cases = ORACLE["ois_cases"]
    assert [case["name"] for case in cases] == ["spread_initial", "spread_updated"]
    inputs = cases[0]["inputs"]
    settings = Settings()
    settings.set_evaluation_date(_date(inputs["evaluation_date"]))
    spread = SimpleQuote(inputs["overnight_spread"])
    helper = OISRateHelper(
        2, Period(1, "Years"), SimpleQuote(inputs["quote"]), Estr(None, settings),
        inputs["payment_lag"], BusinessDayConvention.Following, Frequency.Annual,
        Period(inputs["forward_start_months"], "Months"), settings,
        overnight_spread=spread, averaging_method=MODES[mode],
    )
    curve = PiecewiseLogLinearDiscount(_date(inputs["reference_date"]), [helper], DayCounter.actual360())
    discounts = []
    for case in cases:
        spread.set_value(case["inputs"]["overnight_spread"])
        expected = case["results"][mode]
        assert helper.maturity_date() == _date(expected["maturity"])
        assert helper.pillar_date() == _date(expected["pillar"])
        discounts.append(curve.discount_date(helper.maturity_date()))
        assert discounts[-1] == pytest.approx(expected["discount"], abs=1e-12, rel=0)
        assert helper.implied_quote() == pytest.approx(expected["quote"], abs=1e-12, rel=0)
    assert abs(discounts[0] - discounts[1]) > 1e-6
