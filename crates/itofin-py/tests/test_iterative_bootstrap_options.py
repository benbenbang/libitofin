"""Iterative yield bootstrap options against the independent QuantLib oracle."""

import gc
import math
import re
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper, IterativeBootstrapOptions, PiecewiseYieldCurve,
    PiecewiseLogLinearDiscount, PiecewiseLinearZero, PiecewiseCubicZero,
    PiecewiseLinearForward, PiecewiseConvexMonotoneForward, PiecewiseFlatForward,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

REFERENCE = Date(15, 6, 2026)


def inputs(rate):
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    dc = DayCounter.actual365_fixed()
    index = IborIndex("oracle", Period(1, "Years"), 0, Currency.eur(), Calendar.null_calendar(),
                      BusinessDayConvention.Unadjusted, False, dc, None, settings)
    quote = SimpleQuote(rate)
    helper = DepositRateHelper(quote, index)
    return quote, helper, dc, settings


def oracle():
    path = Path(__file__).parents[2] / "libitofin/tests/fixtures/iterative_bootstrap/oracle.txt"
    result = {}
    for line in path.read_text().splitlines():
        match = re.fullmatch(r"(\w+) \(([^)]+)\) (\S+)", line)
        if match:
            result[match[1]] = [float(x) for x in match[2].split(",")] + [float(match[3])]
    assert len(result) == 7
    return result


@pytest.mark.parametrize("name, rate, lo, hi, attempts, fallback, evaluations", [
    ("positive_min", .015, .04, .1, 3, False, 100),
    ("negative_max", -.015, -.1, -.04, 3, False, 100),
    ("positive", .25, .01, .1, 3, False, 100),
    ("negative", -.25, -.1, -.01, 3, False, 100),
    ("fallback_upper", .25, .01, .1, 1, True, 100),
    ("fallback_lower", -.25, -.1, -.01, 1, True, 100),
    ("eval_fallback", .25, .01, .4, 1, True, 1),
])
def test_iterative_bootstrap_quantlib_oracle(name, rate, lo, hi, attempts, fallback, evaluations):
    _quote, helper, dc, _settings = inputs(rate)
    options = IterativeBootstrapOptions(min_value=lo, max_value=hi, max_attempts=attempts,
                                       dont_throw=fallback, max_evaluations=evaluations)
    curve = PiecewiseLinearZero(REFERENCE, [helper], dc, options)
    expected = oracle()[name]
    assert curve.data() == pytest.approx(expected[:2], abs=1e-12, rel=0)
    assert curve.discount(1) == pytest.approx(expected[2], abs=1e-12, rel=0)


@pytest.mark.parametrize("factory, discount", [
    (PiecewiseLogLinearDiscount, True), (PiecewiseLinearZero, False),
    (PiecewiseCubicZero, False), (PiecewiseLinearForward, False),
    (PiecewiseConvexMonotoneForward, False), (PiecewiseFlatForward, False),
    *[(lambda r, h, d, iterative_options=None, interpolation=i:
       PiecewiseYieldCurve(r, h, d, interpolation, iterative_options=iterative_options), True)
      for i in ("LogLinear", "Linear", "Cubic")],
])
def test_iterative_bootstrap_all_factories(factory, discount):
    _q, helper, dc, _s = inputs(.25)
    lo, hi = (.9, 1.0) if discount else (.01, .1)
    strict = IterativeBootstrapOptions(min_value=lo, max_value=hi)
    curve = factory(REFERENCE, [helper], dc, iterative_options=strict)
    with pytest.raises(ItofinError):
        curve.discount(1)
    _q, helper, dc, _s = inputs(.25)
    retry = IterativeBootstrapOptions(min_value=lo, max_value=hi, max_attempts=3, accuracy=1e-12)
    curve = factory(REFERENCE, [helper], dc, iterative_options=retry)
    assert curve.discount(1) == pytest.approx(.8, abs=1e-12, rel=0)
    for option in (None, IterativeBootstrapOptions()):
        _q, helper, dc, _s = inputs(.25)
        assert factory(REFERENCE, [helper], dc, iterative_options=option).discount(1) == pytest.approx(.8, abs=1e-12, rel=0)


def test_iterative_bootstrap_retry_updates_and_retention():
    quote, helper, dc, settings = inputs(.25)
    options = IterativeBootstrapOptions(min_value=.01, max_value=.1, max_attempts=2)
    curve = PiecewiseLinearZero(REFERENCE, [helper], dc, options)
    with pytest.raises(ItofinError):
        curve.discount(1)
    quote.set_value(.1)
    assert curve.discount(1) == pytest.approx(1 / 1.1, abs=1e-12, rel=0)
    quote.set_value(.15)
    expected = 1 / 1.15
    del quote, helper, dc, settings, options
    gc.collect()
    assert curve.discount(1) == pytest.approx(expected, abs=1e-12, rel=0)
    quote, helper, dc, settings = inputs(.01)
    curve = PiecewiseLinearZero(REFERENCE, [helper], dc, IterativeBootstrapOptions())
    assert curve.data()[1] == pytest.approx(math.log1p(.01), abs=1e-12, rel=0)
    quote.set_value(.4)
    assert curve.data()[1] == pytest.approx(math.log1p(.4), abs=1e-12, rel=0)
    quote.set_value(10)
    with pytest.raises(ItofinError):
        curve.data()
    quote.set_value(.01)
    assert curve.data()[1] == pytest.approx(math.log1p(.01), abs=1e-12, rel=0)
    tomorrow = Date(16, 6, 2026)
    settings.set_evaluation_date(tomorrow)
    maturity = helper.maturity_date()
    assert curve.discount_date(maturity) / curve.discount_date(tomorrow) == pytest.approx(1 / 1.01, abs=1e-12, rel=0)



@pytest.mark.parametrize("kwargs", [
    {"accuracy": 0}, {"accuracy": float("nan")}, {"min_value": float("inf")},
    {"min_value": .2, "max_value": .1}, {"max_attempts": 0}, {"max_factor": .5},
    {"min_factor": float("nan")}, {"dont_throw_steps": 0}, {"max_evaluations": 0},
])
def test_iterative_bootstrap_invalid_options(kwargs):
    with pytest.raises(ItofinError):
        IterativeBootstrapOptions(**cast(dict[str, Any], kwargs))


def test_iterative_bootstrap_options_require_iterative_algorithm():
    _q, helper, dc, _s = inputs(.25)
    options = IterativeBootstrapOptions()
    with pytest.raises(ItofinError, match="require"):
        PiecewiseYieldCurve(REFERENCE, [helper], dc, bootstrap="global", iterative_options=options)
    with pytest.raises(ItofinError, match="require"):
        PiecewiseConvexMonotoneForward(REFERENCE, [helper], dc, "local", options)
    assert PiecewiseLinearZero(REFERENCE, [helper], dc, options).discount(1) > 0


@pytest.mark.parametrize("negative", [False, True])
def test_iterative_bootstrap_asymmetric_factors(negative):
    rate, lo, hi, min_factor, max_factor = (-.25, -.1, -.01, 3, 1) if negative else (.25, .01, .1, 1, 3)
    _quote, helper, dc, _settings = inputs(rate)
    options = IterativeBootstrapOptions(min_value=lo, max_value=hi, max_attempts=2,
                                       min_factor=min_factor, max_factor=max_factor)
    curve = PiecewiseLinearZero(REFERENCE, [helper], dc, options)
    assert curve.discount(1) == pytest.approx(1 / (1 + rate), abs=1e-12, rel=0)


def test_iterative_bootstrap_accuracy_override():
    """QuantLib 1.43, same NullCalendar deposit with accuracy=0.01."""
    _quote, helper, dc, _settings = inputs(.25)
    curve = PiecewiseLinearZero(REFERENCE, [helper], dc, IterativeBootstrapOptions(accuracy=.01))
    assert curve.data()[1] == pytest.approx(.2254558624429838, abs=1e-12, rel=0)
    assert curve.discount(1) == pytest.approx(.7981522881625791, abs=1e-12, rel=0)
