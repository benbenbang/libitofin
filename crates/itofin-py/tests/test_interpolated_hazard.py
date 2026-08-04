"""InterpolatedHazardRateCurve<BackwardFlat> (#741).

Self-contained hand-integrated oracle, mirroring the Rust one in
crates/libitofin/src/termstructures/credit/interpolatedhazardratecurve.rs
(`hazard_rates_step_between_nodes_and_extrapolate_flat` and
`survival_probabilities_match_the_hand_integrated_step_function`, whose fixture
this copies verbatim).

The nodes sit 0, 360, 720 and 1800 days past the reference date, so under
Actual/360 the node times are exactly 0, 1, 2 and 5. That exactness is itself
the day-count discriminator - under Actual/365Fixed the first node would land
at 0.9863 and every pin below would miss - and it is what makes the hand
integral valid arithmetic rather than an approximation.

Backward-flat reads the RIGHT-hand node on each segment, so the hazard rate is
0.015 on (0, 1], 0.02 on (1, 2] and 0.03 on (2, 5]. The survival probability is
exp(-integral) over that step function.
"""

import math

import pytest
from itofin import ItofinError
from itofin.termstructures import InterpolatedHazardRateCurve
from itofin.time import Date, DayCounter

REFERENCE = Date(15, 6, 2026)
OFFSETS = (0, 360, 720, 1800)
RATES = (0.01, 0.015, 0.02, 0.03)
TOLERANCE = 1e-12


def _dates() -> list[Date]:
    return [REFERENCE + offset for offset in OFFSETS]


def _curve() -> InterpolatedHazardRateCurve:
    return InterpolatedHazardRateCurve(_dates(), list(RATES), DayCounter.actual360())


def test_nodes_round_trip():
    curve = _curve()
    nodes = curve.nodes()

    assert len(nodes) == len(OFFSETS)
    assert [date for date, _ in nodes] == _dates()
    assert [rate for _, rate in nodes] == list(RATES)
    assert curve.dates() == _dates()
    assert curve.hazard_rates() == list(RATES)


def test_hazard_rate_steps_to_the_right_hand_node():
    """The single most discriminating read: at a mid-segment t the curve must
    quote the segment's RIGHT node. On (0, 1] backward-flat gives 0.015, where
    forward-flat would give the left node 0.01 and linear the midpoint 0.0125.
    """
    curve = _curve()

    for t, expected in ((0.0, 0.01), (0.5, 0.015), (1.0, 0.015), (1.5, 0.02), (3.5, 0.03)):
        assert abs(curve.hazard_rate(t) - expected) <= TOLERANCE


def test_survival_probability_matches_the_hand_integrated_step_function():
    """integral(0 -> t) of the step function, computed here:

    t = 0.5 lies in (0, 1] where h = 0.015, so the integral is 0.5 * 0.015
             = 0.0075.
    t = 3.5 spans (0, 1] at 0.015, (1, 2] at 0.02 and 1.5 years of (2, 5] at
             0.03, so the integral is 0.015 + 0.02 + 1.5 * 0.03 = 0.08.
    """
    curve = _curve()

    for t, integral in ((0.0, 0.0), (0.5, 0.0075), (1.0, 0.015), (2.0, 0.035), (3.5, 0.08)):
        survival = math.exp(-integral)
        assert abs(curve.survival_probability(t) - survival) <= TOLERANCE
        assert abs(curve.default_probability(t) - (1.0 - survival)) <= TOLERANCE


def test_the_mid_segment_survival_excludes_the_linear_interpolation_value():
    """What the 0.5 pin above rules out. Under linear interpolation the hazard
    rate would run 0.01 + 0.005 t on (0, 1), whose integral over [0, 0.5] is
    0.5 * (0.01 + 0.0125) / 2 = 0.005625 - separated from the backward-flat
    0.0075 by 1.9e-3 in the survival probability, far above the tolerance.
    """
    curve = _curve()
    linear = math.exp(-0.005625)

    assert abs(curve.survival_probability(0.5) - linear) > 1e-3


def test_date_form_agrees_with_the_year_fraction_form():
    curve = _curve()
    two_years = REFERENCE + 720

    assert abs(curve.survival_probability_date(two_years) - math.exp(-0.035)) <= TOLERANCE
    assert abs(curve.hazard_rate_date(two_years) - 0.02) <= TOLERANCE


def test_queries_past_the_last_node_need_extrapolation():
    """Beyond the last node the survival probability carries on at that node's
    rate, and only with extrapolate=True."""
    curve = _curve()
    at_last_node = curve.survival_probability(5.0)

    with pytest.raises(ItofinError):
        curve.survival_probability(7.0)

    tail = at_last_node * math.exp(-0.03 * 2.0)
    assert abs(curve.survival_probability(7.0, True) - tail) <= TOLERANCE
    assert abs(curve.hazard_rate(7.0, True) - 0.03) <= TOLERANCE


def test_mismatched_dates_and_rates_are_rejected():
    with pytest.raises(ItofinError, match="dates/data count mismatch"):
        InterpolatedHazardRateCurve(
            [REFERENCE, REFERENCE + 360], [0.01], DayCounter.actual360()
        )


def test_a_negative_hazard_rate_is_rejected():
    with pytest.raises(ItofinError, match="negative hazard rate"):
        InterpolatedHazardRateCurve(
            [REFERENCE, REFERENCE + 360], [0.01, -0.001], DayCounter.actual360()
        )
