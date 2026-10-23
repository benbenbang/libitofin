"""Bounded par asset swaps against independently generated QuantLib 1.43 values."""

import csv
import gc
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import AssetSwap, FixedRateBond, ZeroCouponBond
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule

ROWS = list(csv.DictReader((Path(__file__).parent / "fixtures/asset_swap_native.tsv").read_text().splitlines(), delimiter="\t"))


def market(row=None, *, price=None, spread=None, pay=None, attach=True):
    """Use explicit annual fixed and semiannual floating NullCalendar schedules."""
    row = row or ROWS[0]
    settings = Settings()
    day = int(row["evaluation"][-2:])
    reference = Date(day, 1, 2026)
    settings.set_evaluation_date(reference)
    settings.set_include_reference_date_events(bool(int(row["include_settlement_flows"])))
    calendar = Calendar.null_calendar()
    dc = DayCounter.actual360()
    quotes = [SimpleQuote(float(row[k])) for k in ["forward", "discount"]]
    curves = [FlatForward.from_quote(reference, quotes[0], dc), FlatForward.from_quote(reference, quotes[1], DayCounter.actual365_fixed())]
    index = IborIndex("AssetBinding", Period(6, "Months"), 0, Currency.eur(), calendar, BDC.Unadjusted, False, dc, curves[0], settings)
    index.add_fixing(Date(5, 1, 2026), float(row["forward"]))
    fixed = Schedule(Date(5, 1, 2026), Date(5, 1, 2029), Frequency.Annual, calendar, BDC.Unadjusted)
    floating = Schedule(Date(5, 1, 2026), Date(5, 1, 2029), Frequency.Semiannual, calendar, BDC.Unadjusted)
    if row["kind"] == "fixed":
        bond = FixedRateBond(0, float(row["face"]), fixed, [float(row["coupon"])], dc, settings, payment_convention=BDC.Unadjusted, redemption=float(row["redemption"]))
    else:
        bond = ZeroCouponBond(0, calendar, float(row["face"]), Date(5, 1, 2029), settings, payment_convention=BDC.Unadjusted, redemption=float(row["redemption"]))
    swap = AssetSwap(bool(int(row["pay_bond_coupon"])) if pay is None else pay, bond, float(row["clean_price"]) if price is None else price, index, float(row["spread"]) if spread is None else spread, floating)
    if attach:
        swap.set_engine(curves[1], settings)
    return swap, settings, quotes, curves, index, bond, floating


@pytest.mark.parametrize("row", ROWS, ids=lambda row: row["name"])
def test_asset_swap_native_oracle(row):
    """All six pricing queries match the same independently pinned wheel profile."""
    swap, *_ = market(row)
    assert swap.par_swap()
    assert swap.pay_bond_coupon() == bool(int(row["pay_bond_coupon"]))
    assert swap.clean_price() == float(row["clean_price"])
    assert swap.spread() == float(row["spread"])
    assert swap.upfront_date() == Date(5, 1, 2026)
    assert not swap.is_calculated()
    for name in ["npv", "bond_leg_npv", "floating_leg_npv", "floating_leg_bps", "fair_spread", "fair_clean_price"]:
        assert getattr(swap, name)() == pytest.approx(float(row[name]), rel=2e-13, abs=2e-10)
    assert swap.is_calculated()
    assert not swap.is_expired()
    fair = swap.fair_spread()
    assert abs(market(row, spread=fair)[0].npv()) < 3e-9
    assert market(row, spread=swap.spread() + 1e-4)[0].npv() - swap.npv() == pytest.approx(swap.floating_leg_bps(), rel=2e-12, abs=3e-10)
    assert market(row, pay=not swap.pay_bond_coupon())[0].npv() == -swap.npv()
    if row["name"] != "upfront-excluded":
        assert abs(market(row, price=swap.fair_clean_price())[0].npv()) < 3e-9


def test_asset_swap_retention_freeze_and_live_updates():
    """Released Python wrappers retain native bonds, quotes, curves and Settings."""
    swap, settings, quotes, curves, index, bond, schedule = market()
    before = swap.npv()
    swap.freeze()
    quotes[0].set_value(.05)
    assert swap.npv() == before
    swap.recalculate()
    assert abs(swap.npv() - before) > 1
    swap.unfreeze()
    before = swap.npv()
    quotes[1].set_value(.06)
    assert abs(swap.npv() - before) > .01
    del curves, index, bond, schedule
    gc.collect()
    quotes[0].set_value(.03)
    assert swap.npv() == pytest.approx(swap.bond_leg_npv() + swap.floating_leg_npv(), abs=1e-12)
    settings.set_evaluation_date(Date(5, 1, 2029))
    assert swap.is_expired()
    assert swap.npv() == 0
    with pytest.raises(ItofinError):
        swap.fair_spread()


def test_asset_swap_policy_flags_and_errors():
    """Silent flow flags must invalidate this swap, not merely its source bond."""
    swap, settings, _, curves, index, bond, schedule = market(ROWS[-1])
    excluded = swap.npv()
    settings.set_include_reference_date_events(True)
    assert swap.npv() == pytest.approx(excluded - 2.75, abs=2e-12)
    other = Settings()
    other.set_evaluation_date(Date(5, 1, 2026))
    with pytest.raises(ItofinError):
        swap.set_engine(curves[1], other)
    for price, spread in [(float("nan"), 0), (100, float("inf"))]:
        with pytest.raises(ItofinError):
            AssetSwap(True, bond, price, index, spread, schedule)
    wrong = Schedule(Date(5, 1, 2026), Date(5, 1, 2028), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
    with pytest.raises(ItofinError):
        AssetSwap(True, bond, 100, index, 0, wrong)
    empty, *_ = market(attach=False)
    with pytest.raises(ItofinError):
        empty.npv()


def test_asset_swap_generated_schedule_matches_explicit_settlement_profile():
    """Default schedules follow native backwards generation from settlement."""
    swap, settings, _, curves, index, bond, _ = market(ROWS[-1])
    generated = AssetSwap(True, bond, swap.clean_price(), index, swap.spread())
    generated.set_engine(curves[1], settings)
    assert generated.npv() == swap.npv()
    assert generated.upfront_date() == swap.upfront_date()


def test_asset_swap_explicit_schedule_before_evaluation_date():
    """Only generated settlement-based schedules need an evaluation date at construction."""
    settings = Settings()
    calendar = Calendar.null_calendar()
    dc = DayCounter.actual360()
    curve = FlatForward(Date(2, 1, 2026), .03, dc)
    index = IborIndex("AssetWithoutDate", Period(6, "Months"), 0, Currency.eur(), calendar, BDC.Unadjusted, False, dc, curve, settings)
    floating = Schedule(Date(5, 1, 2026), Date(5, 1, 2029), Frequency.Semiannual, calendar, BDC.Unadjusted)
    bond = ZeroCouponBond(0, calendar, 100, Date(5, 1, 2029), settings)
    swap = AssetSwap(True, bond, 90, index, 0, floating)
    assert swap.clean_price() == 90
    with pytest.raises(ItofinError):
        AssetSwap(True, bond, 90, index, 0)
    settings.set_evaluation_date(Date(2, 1, 2026))
    swap.set_engine(curve, settings)
    assert abs(swap.npv()) > 0


def policy_market(*, stub=False, missing_today=False):
    swap, settings, _, curves, index, bond, schedule = market(ROWS[-1] if missing_today else ROWS[0])
    if stub:
        schedule = Schedule(Date(5, 2, 2026), Date(5, 1, 2029), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
    if missing_today:
        index = IborIndex("AssetMissingToday", Period(6, "Months"), 0, Currency.eur(), Calendar.null_calendar(), BDC.Unadjusted, False, DayCounter.actual360(), curves[0], settings)

    def fresh():
        result = AssetSwap(True, bond, 97.25, index, .0017, schedule)
        result.set_engine(curves[1], settings)
        return result

    return fresh(), settings, index, fresh


@pytest.mark.parametrize("flag", ["reference", "today", "par"])
def test_asset_swap_silent_policy_cached_frozen_recalculate(flag):
    """Every accessible silent flag must agree with a fresh same-input native swap."""
    swap, settings, _, fresh = policy_market(stub=flag == "par", missing_today=flag != "par")
    if flag != "par":
        settings.set_include_todays_cash_flows(None)
    setter = {"reference": settings.set_include_reference_date_events, "today": settings.set_include_todays_cash_flows, "par": settings.set_using_at_par_coupons}[flag]
    old, new = (True, False) if flag == "par" else (False, True)
    setter(old)
    before = swap.npv()
    assert swap.is_calculated()
    setter(new)
    after = fresh().npv()
    assert abs(after - before) > 1e-6
    assert swap.npv() == pytest.approx(after, rel=0, abs=2e-12)
    swap.freeze()
    setter(old)
    assert swap.npv() == after
    swap.recalculate()
    assert swap.npv() == pytest.approx(fresh().npv(), rel=0, abs=2e-12)
    assert swap.npv() == pytest.approx(before, rel=0, abs=2e-12)
    setter(new)
    assert swap.npv() == before
    swap.calculate()
    swap.unfreeze()
    assert swap.npv() == pytest.approx(fresh().npv(), rel=0, abs=2e-12)
    if flag == "today":
        for reference in [False, True]:
            settings.set_include_reference_date_events(reference)
            for today in [True, False, None]:
                settings.set_include_todays_cash_flows(today)
                assert settings.include_todays_cash_flows() is today
                assert swap.npv() == pytest.approx(fresh().npv(), rel=0, abs=2e-12)
                included = reference if today is None else today
                assert swap.npv() == pytest.approx(after if included else before, rel=0, abs=2e-12)


def test_asset_swap_today_fixing_after_cached_forecast():
    """A supplied today's fixing replaces a cached forecast, including forced frozen repricing."""
    swap, _, index, fresh = policy_market(missing_today=True)
    forecast = swap.npv()
    swap.freeze()
    index.add_fixing(Date(5, 1, 2026), .09)
    assert swap.npv() == forecast
    swap.recalculate()
    assert abs(swap.npv() - forecast) > 1
    assert swap.npv() == pytest.approx(fresh().npv(), rel=0, abs=2e-12)
    swap.unfreeze()
    assert swap.npv() == pytest.approx(fresh().npv(), rel=0, abs=2e-12)
