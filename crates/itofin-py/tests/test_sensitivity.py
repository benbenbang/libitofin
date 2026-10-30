"""Bump-and-reprice sensitivities against a manual bump through the bindings."""

# standard library
from typing import Any, cast

# third party
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Currency, Estr, Euribor, IborIndex
from itofin.instruments import FixedRateBond, FloatingRateBond, MakeOis, MakeVanillaSwap
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.risk import SensitivityAnalysis, bucket_analysis, parallel_analysis
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule

TODAY = Date(15, 1, 2026)
H = 1e-4


def _market():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    forward, discount = SimpleQuote(0.035), SimpleQuote(0.03)
    curves = [FlatForward.from_quote(TODAY, q, DayCounter.actual365_fixed()) for q in (forward, discount)]
    return settings, forward, discount, curves[0], curves[1]


def _portfolio():
    settings, forward, discount, forward_curve, discount_curve = _market()
    schedule = Schedule(TODAY, Date(15, 1, 2031), Frequency.Annual, Calendar.null_calendar(), BDC.Unadjusted)
    bond = FixedRateBond(2, 100.0, schedule, [0.045], DayCounter.actual360(), settings)
    bond.set_engine(DiscountingBondEngine(discount_curve, settings))
    effective = Date(19, 1, 2026)
    swap = MakeVanillaSwap(Period(5, "Years"), Euribor.six_months(discount_curve, settings), settings, fixed_rate=0.04, effective_date=effective).build()
    ois = MakeOis(Period(2, "Years"), Estr(discount_curve, settings), settings, fixed_rate=0.04, effective_date=effective).build()
    return settings, [discount], [bond, swap, ois]


def _floating():
    settings, forward, discount, forward_curve, discount_curve = _market()
    index = IborIndex("py-sensitivity", Period(3, "Months"), 2, Currency.eur(), Calendar.null_calendar(), BDC.Unadjusted, False, DayCounter.actual360(), forward_curve, settings)
    schedule = Schedule(Date(15, 10, 2025), Date(15, 1, 2029), Frequency.Quarterly, Calendar.null_calendar(), BDC.Unadjusted)
    bond = FloatingRateBond(2, 1000.0, schedule, index, DayCounter.actual360(), settings, spreads=[0.002])
    bond.set_engine(DiscountingBondEngine(discount_curve, settings))
    return index, [forward, discount], bond


def _npv(quotes, instruments, quantities, bump):
    originals = [q.value() for q in quotes]
    for q, value in zip(quotes, originals):
        q.set_value(value + bump)
    total = 0.0
    for quantity, instrument in zip(quantities, instruments):
        total += quantity * instrument.npv()
    for q, value in zip(quotes, originals):
        q.set_value(value)
    return total


def _manual(quotes, instruments, quantities):
    reference = _npv(quotes, instruments, quantities, 0.0)
    up = _npv(quotes, instruments, quantities, H)
    down = _npv(quotes, instruments, quantities, -H)
    return reference, (up - down) / (2.0 * H), (up - 2.0 * reference + down) / (H * H), (up - reference) / H


def test_parallel_is_bit_identical_to_the_manual_bump_and_restores_the_quote():
    """Fixed bond, swap and OIS reprice exactly as a manual SimpleQuote bump."""
    _, quotes, instruments = _portfolio()
    quantities = [1.0, 2.0, -1.0]
    reference, first, second, one_side = _manual(quotes, instruments, quantities)
    assert parallel_analysis(quotes, instruments, quantities, H) == (first, second)
    kind = SensitivityAnalysis.OneSide
    assert parallel_analysis(quotes, instruments, quantities, H, kind, reference) == (one_side, None)
    assert quotes[0].value() == 0.03


def test_floating_bond_runs_the_preflight_and_matches_the_binding_npv():
    """A missing past fixing fails the preflight; with it the bump is exact."""
    index, quotes, bond = _floating()
    with pytest.raises(ItofinError):
        parallel_analysis(quotes, [bond])
    index.add_fixing(Date(13, 1, 2026), 0.0387)
    _, first, second, _ = _manual(quotes, [bond], [1.0])
    result = parallel_analysis(quotes, [bond])
    assert result[0] == pytest.approx(first, rel=1e-12)
    assert result[1] == pytest.approx(second, rel=1e-12)
    assert [q.value() for q in quotes] == [0.035, 0.03]


def test_labelled_buckets_sum_to_the_parallel_shift():
    """A mapping keeps its labels and order; a sequence stays index-aligned."""
    index, quotes, bond = _floating()
    index.add_fixing(Date(13, 1, 2026), 0.0387)
    labelled = bucket_analysis({"forward": quotes[0], "discount": quotes[1]}, [bond])
    assert list(labelled) == ["forward", "discount"]
    assert bucket_analysis(quotes, [bond]) == list(labelled.values())
    total = sum(first for first, _ in labelled.values())
    assert total == pytest.approx(parallel_analysis(quotes, [bond])[0], rel=1e-6)
    one_side = bucket_analysis(quotes[:1], [bond], kind=SensitivityAnalysis.OneSide)
    assert one_side[0][1] is None


def test_errors_leave_the_quotes_untouched():
    """Zero shift and empty inputs are core errors; other instruments are deferred."""
    _, quotes, instruments = _portfolio()
    with pytest.raises(ItofinError, match="zero shift"):
        parallel_analysis(quotes, instruments, shift=0.0)
    with pytest.raises(ItofinError, match="empty instrument"):
        bucket_analysis(quotes, [])
    with pytest.raises(TypeError, match="#1322"):
        bucket_analysis(quotes, cast(Any, [quotes[0]]))
    assert quotes[0].value() == 0.03
