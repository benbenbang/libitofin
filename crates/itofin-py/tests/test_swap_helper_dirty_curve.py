"""A direct `implied_quote` on a helper of a dirty curve (#1344).

Moving the helper's own quote leaves its bootstrapped curve stale. The direct
query re-bootstraps first and prices the new quote, as QuantLib's
`LazyObject::calculate` does, instead of panicking on a re-entrant borrow.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import PiecewiseYieldCurve, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period


def test_swap_helper_implied_quote_on_a_dirty_curve_returns_a_float():
    """The issue's repro: a float close to the new quote, not a PanicException."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 6, 2026))
    q = SimpleQuote(0.03)
    helper = SwapRateHelper(
        q,
        Period(1, "Years"),
        Calendar.target(),
        Frequency.Annual,
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        Euribor(Period(6, "Months"), None, settings),
    )
    outer = PiecewiseYieldCurve(Date(17, 6, 2026), [helper], DayCounter.actual365_fixed())
    outer.discount(0.5)
    q.set_value(0.04)
    result = helper.implied_quote()
    assert isinstance(result, float)
    assert result == pytest.approx(0.04, abs=1e-9, rel=0)
