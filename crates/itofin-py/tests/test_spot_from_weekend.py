"""Spot dates from a Saturday evaluation date through the bindings (#1407).

QuantLib v1.44 counts settlement days from the unadjusted evaluation date
(makeois.cpp:78-84, makevanillaswap.cpp:85-89, test-suite
overnightindexedswap.cpp:1261 and swap.cpp:619), while the MakeVanillaSwap
index default still adjusts it on the fixing calendar first
(makevanillaswap.cpp:82). On Saturday 20 June 2026 two TARGET days give
Tuesday 23 June; the index default gives Wednesday 24 June.
"""

import pytest

from itofin import Settings
from itofin.indexes import Estr, Euribor
from itofin.instruments import MakeOis
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, OISRateHelper, SwapRateHelper
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period

SATURDAY = Date(20, 6, 2026)
TUESDAY = Date(23, 6, 2026)
WEDNESDAY = Date(24, 6, 2026)


def _settings():
    settings = Settings()
    settings.set_evaluation_date(SATURDAY)
    return settings


def test_ois_helper_settlement_days_count_from_the_saturday():
    settings = _settings()
    helper = OISRateHelper(2, Period(1, "Years"), SimpleQuote(0.02), Estr(None, settings), 2,
        BDC.Following, Frequency.Annual, Period(0, "Days"), settings)
    assert helper.earliest_date() == TUESDAY


def test_make_ois_default_spot_counts_from_the_saturday():
    settings = _settings()
    curve = FlatForward(SATURDAY, 0.02, DayCounter.actual365_fixed())

    def npv(effective_date=None):
        return MakeOis(Period(1, "Years"), Estr(curve, settings), settings, fixed_rate=0.03,
            nominal=1.0e6, effective_date=effective_date).build().npv()

    spot = npv()
    assert spot == pytest.approx(npv(TUESDAY), abs=1.0e-8)
    assert abs(spot - npv(WEDNESDAY)) > 1.0e-2


def test_swap_helper_index_default_still_adjusts_the_saturday():
    settings = _settings()
    helper = SwapRateHelper(SimpleQuote(0.03), Period(5, "Years"), Calendar.target(), Frequency.Annual,
        BDC.ModifiedFollowing, DayCounter.thirty360_bond_basis(), Euribor(Period(6, "Months"), None, settings))
    assert helper.earliest_date() == WEDNESDAY
