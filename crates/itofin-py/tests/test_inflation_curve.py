"""InterpolatedZeroInflationCurve<Linear> (#749).

Directly-built curve, no bootstrap. Mirrors the Rust oracle in
crates/libitofin/src/termstructures/inflation/interpolatedzeroinflationcurve.rs
(`nodes_round_trip_the_input_dates`,
`the_base_node_sits_before_the_reference_date_at_a_negative_time` and
`zero_rate_date_quantizes_to_the_period_start_and_interpolates`), on a fixture
of its own so every time below is exact arithmetic.

The reference date is 13 Aug 2007 and the base date, the first node, is 1 Jul
2007 - before it. Under Thirty360 BondBasis the base node time is
30 * (7 - 8) + (1 - 13) = -42, over 360, and the second node 1 Sep 2007 sits at
30 * (9 - 8) + (1 - 13) = 18, over 360. Every node falls on the first of a
month, which under Frequency.Monthly is already the start of its own inflation
period, so a node read reaches that node's time exactly and returns its rate
with no interpolation.
"""

import pytest
from itofin import ItofinError
from itofin.termstructures import InterpolatedZeroInflationCurve, ZeroInflationTermStructure
from itofin.time import Date, DayCounter, Frequency

REFERENCE = Date(13, 8, 2007)
DATES = [
    Date(1, 7, 2007),
    Date(1, 9, 2007),
    Date(1, 1, 2008),
    Date(1, 7, 2008),
    Date(1, 7, 2009),
    Date(1, 7, 2012),
]
RATES = [0.02, 0.022, 0.025, 0.027, 0.030, 0.032]
TOLERANCE = 1e-12


def _curve() -> InterpolatedZeroInflationCurve:
    return InterpolatedZeroInflationCurve(
        REFERENCE, list(DATES), list(RATES), Frequency.Monthly, DayCounter.thirty360_bond_basis()
    )


def test_nodes_round_trip():
    curve = _curve()
    nodes = curve.nodes()

    assert isinstance(curve, ZeroInflationTermStructure)
    assert len(nodes) == len(DATES)
    assert [date for date, _ in nodes] == DATES
    assert [rate for _, rate in nodes] == RATES
    assert curve.dates() == DATES


def test_the_base_node_sits_before_the_reference_date_at_a_negative_time():
    """The divergence from a yield curve, whose first node is its own reference
    date at time zero. Here the first node is the base date, 42 thirty-360 days
    before the reference date."""
    curve = _curve()

    assert curve.base_date() == DATES[0]
    assert curve.times()[0] < 0.0
    assert curve.times()[0] == pytest.approx(-42.0 / 360.0, abs=TOLERANCE)
    assert curve.times()[1] == pytest.approx(18.0 / 360.0, abs=TOLERANCE)
    assert len(curve.times()) == len(DATES)


def test_frequency_round_trips():
    assert _curve().frequency() == Frequency.Monthly


def test_a_node_date_reads_back_its_own_rate():
    curve = _curve()

    for date, rate in zip(DATES, RATES):
        assert abs(curve.zero_rate_date(date) - rate) <= TOLERANCE

    assert abs(curve.zero_rate(curve.times()[0]) - RATES[0]) <= TOLERANCE


def test_zero_rate_date_quantizes_to_the_period_start():
    """The single most discriminating read. 15 Sep 2007 lies inside the monthly
    period starting 1 Sep 2007, so the date form must return that node's rate
    0.022 outright.

    The year-fraction form quantizes nothing, and that is what the pin rules
    out: 15 Sep 2007 sits at 30 * (9 - 8) + (15 - 13) = 32 over 360, which is
    (32 - 18) / (138 - 18) = 7/60 of the way from node 1 to node 2, so linear
    interpolation gives 0.022 + 7/60 * 0.003 = 0.02235. An unquantized date
    form would return that instead.
    """
    curve = _curve()
    mid_september = Date(15, 9, 2007)

    assert abs(curve.zero_rate_date(mid_september) - RATES[1]) <= TOLERANCE
    assert abs(curve.zero_rate(32.0 / 360.0) - 0.02235) <= TOLERANCE
    assert abs(curve.zero_rate_date(mid_september) - curve.zero_rate(32.0 / 360.0)) > 1e-4


def test_a_date_below_the_base_date_is_rejected():
    """The base date bounds the curve below, looser than a yield curve's
    reference date: June 2007 precedes it, July 2007 does not."""
    curve = _curve()

    assert abs(curve.zero_rate_date(Date(20, 7, 2007)) - RATES[0]) <= TOLERANCE
    with pytest.raises(ItofinError, match="is before base date"):
        curve.zero_rate_date(Date(20, 6, 2007))


def test_mismatched_dates_and_rates_are_rejected():
    with pytest.raises(ItofinError, match="indices/dates count mismatch"):
        InterpolatedZeroInflationCurve(
            REFERENCE,
            list(DATES),
            RATES[:2],
            Frequency.Monthly,
            DayCounter.thirty360_bond_basis(),
        )
