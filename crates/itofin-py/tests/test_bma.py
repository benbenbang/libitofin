"""Independent QuantLib BMA curve, coupon and retained-market contracts."""

import datetime
import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import AverageBMACoupon
from itofin.indexes import BMAIndex, UsdLibor
from itofin.instruments import BMASwap, SwapType
from itofin.quotes import SimpleQuote
from itofin.termstructures import BMASwapRateHelper, FlatForward, PiecewiseLogLinearDiscount
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule

ORACLE = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/bma.json").read_text())


def serial(d):
    return (datetime.date(d.year, d.month, d.day) - datetime.date(1899, 12, 30)).days


def near(value, expected, tolerance=1e-9):
    assert math.isfinite(value) and math.isfinite(expected)
    assert abs(value - expected) <= tolerance


class Market:
    def __init__(self, history=True):
        self.today, self.spot = Date(23, 10, 2025), Date(27, 10, 2025)
        self.settings = Settings()
        self.settings.set_evaluation_date(self.today)
        self.dc, self.bdc = DayCounter.actual360(), DayCounter.actual_actual_isda()
        self.cal = Calendar.united_states("GovernmentBond")
        self.risk_quote = SimpleQuote(0.04)
        self.risk = FlatForward.from_quote(self.spot, self.risk_quote, self.dc)
        self.libor = UsdLibor(Period(3, "Months"), self.risk, self.settings)
        self.bma = BMAIndex(None, self.settings)
        if history:
            self.bma.add_fixing(Date(22, 10, 2025), 0.03)
        self.quotes = [SimpleQuote(n["fraction"]) for n in ORACLE["nodes"]]
        self.curve, self.helpers = self.build()
        self.index = BMAIndex(self.curve, self.settings)

    def build(self):
        helpers = [BMASwapRateHelper(q, Period(n["years"], "Years"), 2, self.cal,
                                    Period(3, "Months"), BDC.Following, self.bdc, self.bma, self.libor)
                   for q, n in zip(self.quotes, ORACLE["nodes"], strict=True)]
        return PiecewiseLogLinearDiscount(self.today, helpers, self.dc), helpers

    def swap(self, years, index=None):
        end = Date(27, 10, 2025 + years)
        libor_dates = Schedule(self.spot, end, Frequency.Quarterly, self.libor.fixing_calendar(), BDC.ModifiedFollowing)
        bma_dates = Schedule(self.spot, end, Frequency.Quarterly, self.cal, BDC.Following)
        result = BMASwap(SwapType.Payer, 100, libor_dates, 0.75, 0, self.libor, self.dc,
                         bma_dates, index or self.index, self.bdc, self.settings)
        result.set_engine(self.risk, self.settings)
        return result

    def coupon(self):
        return AverageBMACoupon(Date(27, 1, 2026), 100, self.spot, Date(27, 1, 2026), self.index,
                                self.dc, 1.2, 0.001)


def test_bma_quantlib_curve_swap_coupon_and_holiday_oracles():
    assert ORACLE["quantlib"] == "1.43"
    assert [n["years"] for n in ORACLE["nodes"]] == [1, 2, 3, 4, 5, 7, 10, 15, 20, 30]
    m = Market()
    assert serial(m.today) == ORACLE["today"] and serial(m.spot) == ORACLE["settlement"]
    dates, data = m.curve.dates(), m.curve.data()
    for pos, node in enumerate(ORACLE["nodes"], 1):
        assert serial(dates[pos]) == node["pillar"]
        near(data[pos], node["discount"])
        near(m.helpers[pos - 1].implied_quote(), node["fraction"])
        swap = m.swap(node["years"])
        assert not swap.is_calculated()
        near(swap.npv(), node["npv"])
        assert swap.is_calculated()
        near(swap.fair_libor_fraction(), node["fair_fraction"])
        near(swap.fair_libor_spread(), node["fair_spread"])
        for leg, label in enumerate(["libor", "bma"]):
            near(swap.leg_npv(leg), node[f"{label}_npv"])
            near(swap.leg_bps(leg), node[f"{label}_bps"])
        with pytest.raises(ItofinError):
            swap.leg_npv(2)
        near(swap.npv(), node["npv"])
    coupon = m.coupon()
    near(coupon.rate(), ORACLE["coupon"]["rate"])
    near(coupon.amount(), ORACLE["coupon"]["amount"])
    near(coupon.accrual_period(), 92 / 360)
    assert list(map(serial, coupon.fixing_dates())) == ORACLE["coupon"]["fixing_dates"]
    holiday = m.index.fixing_schedule(Date(20, 12, 2024), Date(8, 1, 2025))
    assert list(map(serial, holiday)) == ORACLE["holiday_fixings"]
    assert m.index.is_valid_fixing_date(Date(26, 12, 2024))
    assert not m.index.is_valid_fixing_date(Date(25, 12, 2024))
    value = m.index.value_date(Date(22, 10, 2025))
    assert value == Date(23, 10, 2025)
    assert m.index.maturity_date(value) == Date(30, 10, 2025)
    assert m.index.fixing_calendar() == m.cal
    near(m.index.fixing(Date(22, 10, 2025)), 0.03, 0)
    assert math.isfinite(m.index.fixing(Date(29, 10, 2025)))
    assert m.index.has_historical_fixing(Date(22, 10, 2025))
    assert m.index.past_fixing(Date(22, 10, 2025)) == 0.03
    assert m.index.past_fixing(Date(29, 10, 2025)) is None


def test_bma_updates_history_recovery_and_cold_retention():
    m = Market(history=False)
    swap, coupon = m.swap(5), m.coupon()
    with pytest.raises(ItofinError):
        coupon.rate()
    m.bma.add_fixing(Date(22, 10, 2025), 0.03)
    near(coupon.rate(), ORACLE["coupon"]["rate"])
    before = swap.npv()
    m.quotes[4].set_value(0.69)
    assert not swap.is_calculated()
    assert abs(swap.npv() - before) > 1e-6
    fresh_curve, fresh_helpers = m.build()
    fresh_index = BMAIndex(fresh_curve, m.settings)
    near(swap.npv(), m.swap(5, fresh_index).npv())
    m.index.clear_fixings()
    assert not m.bma.has_historical_fixing(Date(22, 10, 2025))
    assert m.bma.past_fixing(Date(22, 10, 2025)) is None
    with pytest.raises(ItofinError):
        coupon.amount()
    m.bma.add_fixing(Date(22, 10, 2025), 0.031)
    assert math.isfinite(coupon.amount())
    for bad in [float("nan"), float("inf")]:
        with pytest.raises(ItofinError):
            m.index.add_fixing(Date(29, 10, 2025), bad)
    with pytest.raises(ItofinError):
        m.index.add_fixing(Date(24, 10, 2025), 0.03)
    with pytest.raises(ItofinError):
        m.index.fixing_schedule(Date(8, 1, 2025), Date(20, 12, 2024))
    with pytest.raises(ItofinError):
        AverageBMACoupon(m.spot, 100, m.spot, m.spot, m.index, m.dc)
    cold = m.swap(10)
    expected = m.swap(10).npv()
    assert not cold.is_calculated()
    del m, swap, coupon, fresh_helpers, fresh_index, fresh_curve
    gc.collect()
    near(cold.npv(), expected)
