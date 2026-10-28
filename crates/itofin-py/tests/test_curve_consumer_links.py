"""Curve handles given to native consumers keep the curve's owners alive."""

import gc

import pytest

from itofin.indexes import Euribor
from itofin.time import Date, Period
from test_global_bootstrap import build, strip
from test_joint_yield_curves import Market


@pytest.mark.parametrize("member", [0, 1])
def test_joint_member_consumer_retains_assembly(member):
    """An index over a joint member recalculates after the Python owners are collected."""
    market = Market()
    owner = market.build()
    curve = owner.curve(member)
    months = [3, 6][member]
    index = Euribor(Period(months, "Months"), curve, market.settings)
    fixing_date = Date(23, 4, 2026)
    before = index.fixing(fixing_date, False)
    del owner, curve
    gc.collect()
    market.quote.set_value(0.031)
    after = index.fixing(fixing_date, False)
    fresh = Euribor(Period(months, "Months"), market.build().curve(member), market.settings)
    assert abs(after - before) > 1e-8
    assert after == pytest.approx(fresh.fixing(fixing_date, False), abs=1e-12, rel=0.0)


def test_local_global_curve_consumers_follow_quotes():
    """Every consumer of a locally built global curve sees a quote change."""
    settings, quotes, helpers = strip()
    curve = build(helpers)
    indices = [Euribor(Period(1, "Months"), curve, settings) for _ in range(2)]
    fixing_date = Date(15, 7, 2026)
    before = [index.fixing(fixing_date, False) for index in indices]
    quotes[0].set_value(0.041)
    after = [index.fixing(fixing_date, False) for index in indices]
    assert before[0] == before[1]
    assert after[0] == after[1]
    assert abs(after[0] - before[0]) > 1e-8
    del curve
    gc.collect()
    quotes[0].set_value(0.042)
    assert all(abs(index.fixing(fixing_date, False) - after[0]) > 1e-8 for index in indices)
