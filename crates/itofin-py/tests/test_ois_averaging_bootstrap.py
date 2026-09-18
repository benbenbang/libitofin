"""QuantLib-backed arithmetic/compound OIS bootstrap and observer propagation."""

import gc
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
    (Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/overnight_averaging/ois.json").read_text()
)
CASES = {case["name"]: case for case in ORACLE["ois_cases"]}
MODES = {"simple": RateAveraging.Simple, "compound": RateAveraging.Compound}


def _date(value):
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def _curve(case, mode):
    inputs = case["inputs"]
    settings = Settings()
    settings.set_evaluation_date(_date(inputs["evaluation_date"]))
    quote = SimpleQuote(inputs["quote"])
    discount = None
    if inputs["discount_rate"] is not None:
        discount = FlatForward(_date(inputs["reference_date"]), inputs["discount_rate"], DayCounter.actual365_fixed())
    helper = OISRateHelper(
        2, Period(inputs.get("tenor_years", 1), "Years"), quote, Estr(None, settings), inputs["payment_lag"],
        BusinessDayConvention.Following,
        Frequency.Semiannual if inputs.get("payment_frequency") == "semiannual" else Frequency.Annual,
        Period(inputs["forward_start_months"], "Months"),
        settings, discounting_curve=discount, averaging_method=MODES[mode],
    )
    with pytest.raises(ItofinError, match="term structure"):
        helper.implied_quote()
    curve = PiecewiseLogLinearDiscount(_date(inputs["reference_date"]), [helper], DayCounter.actual360())
    return settings, quote, helper, curve, discount


def _assert_oracle(case, mode, helper, curve):
    expected = case["results"][mode]
    assert helper.maturity_date() == _date(expected["maturity"])
    assert helper.pillar_date() == _date(expected["pillar"])
    assert curve.discount_date(helper.maturity_date()) == pytest.approx(expected["discount"], abs=1e-12, rel=0)
    assert helper.implied_quote() == pytest.approx(expected["quote"], abs=1e-12, rel=0)
    assert helper.quote_error() == pytest.approx(0.0, abs=1e-12)


@pytest.mark.parametrize("case_name", ["baseline", "forward_start", "external_discount"])
@pytest.mark.parametrize("mode", MODES)
def test_ois_helper_modes_match_quantlib(case_name, mode):
    """Helper dates, discount factors and rates match independent QuantLib curves."""
    case = CASES[case_name]
    _, _, helper, curve, _ = _curve(case, mode)
    _assert_oracle(case, mode, helper, curve)


@pytest.mark.parametrize("case_name", ["baseline", "forward_start"])
@pytest.mark.parametrize("mode", MODES)
def test_ois_helper_modes_reprice_independent_swap(case_name, mode):
    """The fitted curve reprices a separately built annual-payment OIS."""
    case = CASES[case_name]
    settings, _, helper, curve, discount = _curve(case, mode)
    _assert_oracle(case, mode, helper, curve)
    swap = MakeOis(
        Period(1, "Years"), Estr(curve, settings), settings,
        fixed_rate=case["inputs"]["quote"], effective_date=helper.earliest_date(), nominal=1e6,
        payment_lag=case["inputs"]["payment_lag"], discounting_term_structure=discount or curve,
        averaging_method=MODES[mode],
    ).build()
    assert swap.fair_rate() == pytest.approx(case["inputs"]["quote"], abs=1e-12, rel=0)
    assert swap.npv() == pytest.approx(0.0, abs=1e-6)


@pytest.mark.parametrize("mode", MODES)
def test_ois_quote_and_evaluation_date_updates_match_quantlib(mode):
    """Both quote and settings observers invalidate the existing curve and swap."""
    settings, quote, helper, curve, _ = _curve(CASES["baseline"], mode)
    _assert_oracle(CASES["baseline"], mode, helper, curve)
    swap = MakeOis(
        Period(1, "Years"), Estr(curve, settings), settings,
        fixed_rate=0.05, effective_date=helper.earliest_date(),
        nominal=1e6, payment_lag=2, averaging_method=MODES[mode],
    ).build()
    assert swap.npv() == pytest.approx(0.0, abs=1e-6)
    assert swap.is_calculated()
    quote.set_value(CASES["quote_updated"]["inputs"]["quote"])
    assert not swap.is_calculated()
    _assert_oracle(CASES["quote_updated"], mode, helper, curve)
    assert swap.fair_rate() == pytest.approx(0.06, abs=1e-12, rel=0)
    settings.set_evaluation_date(_date(CASES["date_updated"]["inputs"]["evaluation_date"]))
    assert not swap.is_calculated()
    _assert_oracle(CASES["date_updated"], mode, helper, curve)


@pytest.mark.parametrize("mode", MODES)
def test_ois_curve_retains_helper_quote_index_and_settings(mode):
    """A curve remains usable after all Python construction dependencies disappear."""
    settings, quote, helper, curve, discount = _curve(CASES["external_discount"], mode)
    maturity = helper.maturity_date()
    del settings, quote, helper, discount
    gc.collect()
    expected = CASES["external_discount"]["results"][mode]["discount"]
    assert curve.discount_date(maturity) == pytest.approx(expected, abs=1e-12, rel=0)
