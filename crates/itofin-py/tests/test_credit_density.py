"""Default-density integration, independent CDS repricing, and retained inputs."""

import gc

import pytest

from itofin import ItofinError
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.pricingengines import MidPointCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import InterpolatedDefaultDensityCurve, PiecewiseDefaultDensityCurve, SpreadCdsHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period

from test_credit_bootstrap import Market, QUOTES, RECOVERY_RATE, SETTLEMENT_DAYS, TENORS, TODAY


@pytest.mark.parametrize(
    ("interpolation", "survivals", "densities"),
    [
        ("BackwardFlat", [0.98, 0.92, 0.8], [0.04, 0.08, 0.08]),
        ("Linear", [0.9875, 0.945, 0.83], [0.03, 0.06, 0.08]),
    ],
)
def test_density_integral_and_flat_extrapolation(interpolation, survivals, densities):
    dates = [TODAY, TODAY + 360, TODAY + 720]
    values = [0.02, 0.04, 0.08]
    curve = InterpolatedDefaultDensityCurve(dates, values, DayCounter.actual360(), interpolation, Calendar.target())
    for t, survival, density in zip([0.5, 1.5, 3.0], survivals, densities):
        assert curve.survival_probability(t, True) == pytest.approx(survival, rel=0, abs=1e-14)
        assert curve.default_density(t, True) == pytest.approx(density, rel=0, abs=1e-14)
        assert curve.hazard_rate(t, True) == pytest.approx(density / survival, rel=0, abs=1e-14)
    assert curve.times() == [0.0, 1.0, 2.0]
    assert curve.nodes() == list(zip(dates, values))
    returned = curve.default_densities()
    returned[1] = values[1] = 9.0
    dates[0] = Date(1, 1, 2020)
    assert curve.data() == [0.02, 0.04, 0.08]
    assert curve.dates()[0] == TODAY
    with pytest.raises(ItofinError):
        curve.survival_probability(3.0)


@pytest.mark.parametrize("interpolation", ["BackwardFlat", "Linear"])
def test_density_bootstrap_reprices_fresh_contracts_after_collection(interpolation):
    market = Market()
    first_quote = SimpleQuote(QUOTES[0])
    market.helpers[0] = SpreadCdsHelper(
        first_quote, Period(TENORS[0], "Years"), SETTLEMENT_DAYS, market.calendar,
        Frequency.Quarterly, BusinessDayConvention.Following, DateGeneration.TwentiethIMM,
        market.day_counter, RECOVERY_RATE, market.discount, market.settings,
    )
    curve = PiecewiseDefaultDensityCurve(TODAY, market.helpers, market.day_counter, interpolation)
    curve.calculate()
    assert len(curve.nodes()) == 5
    assert curve.default_densities() == curve.data()
    del market.curve, market.helpers
    gc.collect()
    for first in [QUOTES[0], 0.0055]:
        first_quote.set_value(first)
        for quote, tenor in zip([first, *QUOTES[1:]], TENORS):
            swap = CreditDefaultSwap.with_terms(
                ProtectionSide.Buyer, 1.0, quote, market.round_trip_schedule(tenor),
                BusinessDayConvention.Following, market.day_counter, market.settings,
                protection_start=TODAY + SETTLEMENT_DAYS,
            )
            swap.set_engine(MidPointCdsEngine(curve, RECOVERY_RATE, market.discount, market.settings))
            assert swap.fair_spread() == pytest.approx(quote, rel=0, abs=1e-6)


def test_density_inputs_reject_invalid_nodes_and_conventions():
    dc = DayCounter.actual360()
    with pytest.raises(ValueError, match="interpolation"):
        InterpolatedDefaultDensityCurve([TODAY, TODAY + 360], [0.02, 0.03], dc, "Cubic")
    for values in ([-0.01, 0.03], [0.02], [0.02, float("nan")]):
        with pytest.raises(ItofinError):
            InterpolatedDefaultDensityCurve([TODAY, TODAY + 360], values, dc)
    with pytest.raises(ItofinError):
        PiecewiseDefaultDensityCurve(TODAY, [], dc)
