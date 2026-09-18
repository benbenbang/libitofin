"""Global callback and variable binding oracle from QuantLib 1.43 C++."""

import csv
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper,
    FuturesRateHelper,
    FuturesType,
    PiecewiseYieldCurve,
    SimpleQuoteVariables,
)
from itofin.time import Date, DayCounter, Period


def strip():
    """Build a deposit strip with observable market inputs."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 6, 2026))
    quotes = [SimpleQuote(0.04), SimpleQuote(0.045)]
    helpers = [
        DepositRateHelper(quote, Euribor(Period(months, "Months"), None, settings))
        for months, quote in zip([1, 3], quotes)
    ]
    return settings, quotes, helpers


def build(helpers, **kwargs):
    """Use fixed settlement and the existing default discount convention."""
    return PiecewiseYieldCurve(
        Date(17, 6, 2026),
        helpers,
        DayCounter.actual365_fixed(),
        bootstrap="global",
        **kwargs,
    )


def test_quantlib_joint_futures_convexity_dates_and_penalties():
    """Reprice real futures and an extra deposit while solving its convexity quote."""
    settings, quotes, helpers = strip()
    convexity = SimpleQuote(0.01)
    future = FuturesRateHelper.from_end_date(
        SimpleQuote(95.0),
        Date(17, 6, 2026),
        Date(17, 9, 2026),
        DayCounter.actual360(),
        convexity,
        FuturesType.Custom,
        register_conv_adj=False,
    )
    date_calls = []
    grids = []

    def dates():
        date_calls.append(True)
        return [Date(17, 8, 2026)]

    def penalties(times, data):
        grids.append((times[:], data[:]))
        return [1.0e4 * helpers[1].quote_error(), data[2] - 0.99]

    curve = build(
        [helpers[0], future],
        additional_helpers=[helpers[1]],
        additional_dates=dates,
        additional_penalties=penalties,
        additional_variables=SimpleQuoteVariables([convexity], [0.01], [0.0]),
    )
    oracle = Path(__file__).parent / "fixtures/global_bootstrap/oracle.csv"
    with oracle.open() as source:
        rows = list(csv.DictReader(source))
    assert len(rows) == 2
    for row in rows:
        calls = len(date_calls)
        quotes[1].set_value(float(row["long_quote"]))
        for month, field in [(7, "july_discount"), (8, "august_discount"), (9, "september_discount")]:
            assert abs(curve.discount_date(Date(17, month, 2026)) - float(row[field])) < 1.0e-9
        assert abs(convexity.value() - float(row["convexity"])) < 1.0e-9
        assert abs(convexity.value() - 0.01) > 1.0e-4
        assert abs(future.implied_quote() - 95.0) < 1.0e-9
        assert abs(helpers[1].quote_error()) < 1.0e-9
        assert len(date_calls) > calls
    assert all(len(times) == len(data) == 4 for times, data in grids)
    assert all(times[0] == 0.0 and data[0] == 1.0 for times, data in grids)


@pytest.mark.parametrize("option", ["additional_penalties", "additional_dates", "additional_variables"])
def test_iterative_rejects_global_options(option):
    """No global restriction is silently ignored by the iterative algorithm."""
    _, _, helpers = strip()
    with pytest.raises(ItofinError, match="require.*global"):
        if option == "additional_variables":
            PiecewiseYieldCurve(
                Date(17, 6, 2026),
                helpers,
                DayCounter.actual365_fixed(),
                additional_variables=SimpleQuoteVariables([SimpleQuote(0.1)]),
            )
        elif option == "additional_dates":
            PiecewiseYieldCurve(
                Date(17, 6, 2026),
                helpers,
                DayCounter.actual365_fixed(),
                additional_dates=lambda: [],
            )
        else:
            PiecewiseYieldCurve(
                Date(17, 6, 2026),
                helpers,
                DayCounter.actual365_fixed(),
                additional_penalties=lambda times, data: [],
            )


@pytest.mark.parametrize("option", ["additional_penalties", "additional_dates"])
def test_noncallable_is_rejected_at_construction(option):
    """Invalid callback configuration fails before any lazy solve."""
    _, _, helpers = strip()
    with pytest.raises(ItofinError, match="must be callable"):
        build(helpers, **{option: 123})


@pytest.mark.parametrize(
    "guesses,bounds", [([1, 2], []), ([], [0, 1]), ([0], [0]), ([float("nan")], []), ([1], [float("inf")])]
)
def test_invalid_variables(guesses, bounds):
    """Reject mismatched or non-finite optimizer coordinates and singular bounds."""
    with pytest.raises(ItofinError):
        SimpleQuoteVariables([SimpleQuote(0.1)], guesses, bounds)


def test_extra_dates_need_residuals():
    """A callable date really adds an unknown and an underdetermined fit fails."""
    _, _, helpers = strip()
    curve = build(helpers, additional_dates=lambda: [Date(17, 8, 2026)])
    with pytest.raises(ItofinError, match="less functions"):
        curve.discount(0.1)
