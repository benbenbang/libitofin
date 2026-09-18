"""QuantLib-backed Simple/Compound OIS pricing and binding ownership contracts."""

import gc
import json
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Estr
from itofin.instruments import MakeOis
from itofin.termstructures import FlatForward, RateAveraging
from itofin.time import Date, DayCounter, Period

ORACLE = json.loads(
    (Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/overnight_averaging/swaps.json").read_text()
)
SWAP_CASES = [case for case in ORACLE["swap_cases"] if case["name"] in ("future", "forward_updated")]
MODES = {"simple": RateAveraging.Simple, "compound": RateAveraging.Compound}


def _date(value):
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def _builder(case, averaging, fixed_rate=None):
    inputs = case["inputs"]
    settings = Settings()
    settings.set_evaluation_date(_date(inputs["evaluation_date"]))
    reference = _date(inputs["reference_date"])
    forward = FlatForward(reference, inputs["forward_rate"], DayCounter.actual365_fixed())
    discount = FlatForward(reference, inputs["discount_rate"], DayCounter.actual365_fixed())
    return MakeOis(
        Period(inputs["tenor_years"], "Years"),
        Estr(forward, settings),
        settings,
        fixed_rate=fixed_rate,
        effective_date=_date(inputs["effective_date"]),
        nominal=inputs["nominal"],
        payment_lag=inputs["payment_lag"],
        discounting_term_structure=discount,
        averaging_method=averaging,
    )


@pytest.mark.parametrize("case", SWAP_CASES, ids=lambda case: case["name"])
@pytest.mark.parametrize("mode", MODES)
def test_ois_averaging_retains_inputs_and_matches_quantlib(case, mode):
    """Dropping Python inputs and the builder leaves a correctly priced swap."""
    builder = _builder(case, MODES[mode], case["inputs"]["fixed_rate"])
    gc.collect()
    swap = builder.build()
    del builder
    gc.collect()
    expected = case["results"][mode]
    assert swap.nominal() == case["inputs"]["nominal"]
    assert swap.fixed_rate() == case["inputs"]["fixed_rate"]
    assert swap.npv() == pytest.approx(expected["npv"], abs=1e-7, rel=0)
    assert swap.fair_rate() == pytest.approx(expected["fair_rate"], abs=1e-12, rel=0)
    assert swap.price() == swap.npv()
    assert swap.is_calculated()


@pytest.mark.parametrize("case", SWAP_CASES, ids=lambda case: case["name"])
@pytest.mark.parametrize("mode", MODES)
def test_ois_averaging_par_fill_matches_quantlib(case, mode):
    """Omitting the fixed rate fills each averaging mode's independent fair rate."""
    swap = _builder(case, MODES[mode]).build()
    assert swap.fixed_rate() == pytest.approx(case["results"][mode]["fair_rate"], abs=1e-12, rel=0)
    assert swap.npv() == pytest.approx(0.0, abs=1e-7)


def test_ois_default_remains_compound():
    """The existing default must not silently change to arithmetic averaging."""
    case = SWAP_CASES[0]
    swap = _builder(case, None, case["inputs"]["fixed_rate"]).build()
    assert swap.npv() == pytest.approx(case["results"]["compound"]["npv"], abs=1e-7, rel=0)
    assert abs(swap.npv() - case["results"]["simple"]["npv"]) > 1.0


@pytest.mark.parametrize("mode", MODES)
def test_ois_missing_fixing_error_leaves_future_pricing_usable(mode):
    """Missing past fixings raise ItofinError without poisoning later valuation."""
    settings = Settings()
    settings.set_evaluation_date(Date(7, 7, 2026))
    curve = FlatForward(Date(7, 7, 2026), 0.04, DayCounter.actual365_fixed())
    index = Estr(curve, settings)
    past = MakeOis(
        Period(1, "Years"), index, settings,
        fixed_rate=0.05, effective_date=Date(6, 7, 2026), averaging_method=MODES[mode],
    ).build()
    with pytest.raises(ItofinError, match="[Mm]issing.*fixing"):
        past.npv()
    future = MakeOis(
        Period(1, "Years"), index, settings,
        effective_date=Date(9, 7, 2026), averaging_method=MODES[mode],
    ).build()
    assert future.npv() == pytest.approx(0.0, abs=1e-12)


@pytest.mark.parametrize("mode", MODES)
def test_ois_missing_evaluation_date_build_can_retry(mode):
    """A stored builder can build successfully once its missing settings arrive."""
    settings = Settings()
    curve = FlatForward(Date(7, 7, 2026), 0.04, DayCounter.actual365_fixed())
    builder = MakeOis(Period(1, "Years"), Estr(curve, settings), settings, averaging_method=MODES[mode])
    with pytest.raises(ItofinError, match="evaluation date"):
        builder.build()
    settings.set_evaluation_date(Date(7, 7, 2026))
    assert builder.build().npv() == pytest.approx(0.0, abs=1e-12)
