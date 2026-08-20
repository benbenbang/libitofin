"""The remaining index consumers widened to the generic IborIndex, plus the
SwapIndex currency thread (#868).

Construction is the oracle. Every consumer exercised here took Euribor
concretely, so handing it a bare IborIndex raised a TypeError; nothing numeric
is pinned, because the re-type is a pure signature change - each body already
read the same Shared<IborIndex> off either type. The regression arm re-runs the
same consumers with a Euribor, which still passes as the subclass.

The currency is the one part that is not a re-type: both SwapIndex constructors
hard-coded EUR. It stays inert for every ported consumer - the underlying swap
never reads it - so the new currency() getter, which reads it back off the core
index rather than off a cached argument, is its only observable. Both
constructors carry an arm, since a stale hard-code left behind in either one
still compiles, and both a USD and a EUR arm run, so a fresh hard-code of
either currency fails one of them.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import Currency, Euribor, IborIndex, SwapIndex
from itofin.instruments import MakeVanillaSwap
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, FraRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

EVAL = Date(15, 1, 2026)
START = Date(15, 1, 2028)


def _fixture():
    settings = Settings()
    settings.set_evaluation_date(EVAL)
    curve = FlatForward(EVAL, 0.03, DayCounter.actual365_fixed())
    return settings, curve


def _generic_index(currency, curve, settings):
    """A 3M index spelled out field by field, outside every named family."""
    return IborIndex(
        "Generic3M",
        Period(3, "Months"),
        2,
        currency,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
        True,
        DayCounter.actual360(),
        curve,
        settings,
    )


def _swap_index(index, currency, settings, discount=None):
    args = (
        "GenericSwapIndex",
        Period(5, "Years"),
        2,
        currency,
        Calendar.target(),
        Period(1, "Years"),
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        index,
    )
    if discount is None:
        return SwapIndex(*args, settings)
    return SwapIndex.with_exogenous_discount(*args, discount, settings)


def test_make_vanilla_swap_takes_a_generic_index():
    settings, curve = _fixture()
    index = _generic_index(Currency.usd(), curve, settings)
    swap = MakeVanillaSwap(
        Period(5, "Years"),
        index,
        settings,
        effective_date=START,
        nominal=100.0,
    ).build()
    assert swap.nominal() == pytest.approx(100.0)


def test_fra_rate_helper_takes_a_generic_index():
    settings, curve = _fixture()
    index = _generic_index(Currency.usd(), curve, settings)
    helper = FraRateHelper(SimpleQuote(0.03), Period(3, "Months"), index)
    assert helper is not None


def test_swap_index_takes_a_generic_index_and_reports_its_currency():
    settings, curve = _fixture()
    index = _generic_index(Currency.usd(), curve, settings)
    assert _swap_index(index, Currency.usd(), settings).currency().code() == "USD"


def test_swap_index_currency_is_not_a_fresh_usd_hard_code():
    settings, curve = _fixture()
    index = _generic_index(Currency.eur(), curve, settings)
    assert _swap_index(index, Currency.eur(), settings).currency().code() == "EUR"


def test_exogenous_discount_swap_index_threads_its_own_currency():
    settings, curve = _fixture()
    index = _generic_index(Currency.usd(), curve, settings)
    swap_index = _swap_index(index, Currency.gbp(), settings, discount=curve)
    assert swap_index.exogenous_discount()
    assert swap_index.currency().code() == "GBP"


def test_euribor_still_reaches_every_widened_consumer():
    settings, curve = _fixture()
    index = Euribor.three_months(curve, settings)
    swap = MakeVanillaSwap(
        Period(5, "Years"), index, settings, effective_date=START, nominal=100.0
    ).build()
    assert swap.nominal() == pytest.approx(100.0)
    assert FraRateHelper(SimpleQuote(0.03), Period(3, "Months"), index) is not None
    assert _swap_index(index, Currency.eur(), settings).currency().code() == "EUR"
