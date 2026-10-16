"""Native BondForward oracle, retained ownership and state boundaries."""
import gc
import json
import math
from pathlib import Path
import pytest
from itofin import ItofinError, Settings
from itofin.instruments import BondForward, CallableZeroCouponBond, FixedRateBond, Position, ZeroCouponBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Schedule

ROWS = json.loads((Path(__file__).parent / "fixtures/bond_forward_native.json").read_text())["rows"]


def setup(kind="fixed", face=100.):
    """Build the same live plain bond and three independent native curves."""
    settings = Settings()
    today = Date(2, 1, 2025)
    settings.set_evaluation_date(today)
    cal, dc = Calendar.null_calendar(), DayCounter.actual365_fixed()
    quote = SimpleQuote(.03)
    spot = FlatForward.from_quote(today, quote, dc)
    finance, income = [FlatForward(today, r, dc) for r in (.04, .025)]
    if kind == "fixed":
        schedule = Schedule(Date(2, 7, 2024), Date(2, 7, 2026), Frequency.Semiannual, cal, BDC.Unadjusted)
        bond = FixedRateBond(2, face, schedule, [.05], dc, settings, payment_convention=BDC.Unadjusted, issue_date=Date(2, 7, 2024))
    else:
        bond = ZeroCouponBond(2, cal, face, Date(2, 7, 2026), settings, payment_convention=BDC.Unadjusted, issue_date=Date(2, 7, 2024))
    bond.set_engine(DiscountingBondEngine(spot, settings))
    return settings, bond, quote, finance, income, cal, dc


def make(bond, finance, cal, dc, *, income=None, delivery=None, short=False, strike=105.):
    """Construct an actual retained native lazy forward."""
    return BondForward(bond, Date(2, 1, 2025), delivery or Date(2, 7, 2025), Position.Short if short else Position.Long, strike, 0, dc, cal, BDC.Unadjusted, finance, income)


@pytest.mark.parametrize("row", ROWS)
def test_compiled_quantlib(row):
    """Pin signs, separate/default income and raw-currency non-100 redemption."""
    settings, bond, quote, finance, income, cal, dc = setup(row["kind"], row["face"])
    import datetime
    delivery = datetime.date(1899, 12, 30) + datetime.timedelta(days=row["delivery"])
    forward = make(bond, finance, cal, dc, income=income if row["separate"] else None, delivery=Date(delivery.day, delivery.month, delivery.year), short=row["short"])
    assert [forward.npv(), forward.fair_forward_price(), forward.clean_forward_price()] == pytest.approx(row["values"], rel=0., abs=2e-11)
    assert forward.value_date() == Date(2, 1, 2025)
    assert forward.delivery_date() == Date(delivery.day, delivery.month, delivery.year)
    assert forward.settlement_date() == Date(2, 1, 2025)
    assert forward.strike() == 105.
    assert forward.position() == (Position.Short if row["short"] else Position.Long)


def test_live_retention_engine_settings_and_freeze():
    """Original wrapper, native lazy cache and live engine survive external release."""
    settings, bond, quote, finance, income, cal, dc = setup()
    forward = make(bond, finance, cal, dc, income=income)
    initial = forward.npv()
    forward.freeze()
    quote.set_value(.08)
    assert forward.npv() == initial
    forward.recalculate()
    changed = forward.npv()
    assert changed < initial
    quote.set_value(.01)
    assert forward.npv() == changed
    forward.unfreeze()
    assert forward.npv() > initial
    bond.set_engine(DiscountingBondEngine(FlatForward(Date(2, 1, 2025), .09, dc), settings))
    assert forward.npv() < initial
    del bond, finance, income, cal, dc
    gc.collect()
    assert math.isfinite(forward.npv())
    settings.set_evaluation_date(Date(3, 7, 2025))
    assert forward.is_expired()
    assert forward.npv() == 0.
    with pytest.raises(ItofinError):
        forward.fair_forward_price()
    with pytest.raises(ItofinError):
        forward.clean_forward_price()


@pytest.mark.parametrize("strike", [-1., math.nan, math.inf])
def test_invalid_strike(strike):
    """Reject invalid strikes at construction without a partial facade."""
    settings, bond, quote, finance, income, cal, dc = setup()
    with pytest.raises(ItofinError):
        make(bond, finance, cal, dc, strike=strike)


def test_callable_rejection_and_missing_engine():
    """Only fixed/zero plain backing is accepted, and engine failures remain live."""
    settings, bond, quote, finance, income, cal, dc = setup()
    callable_bond = CallableZeroCouponBond(2, cal, 100., Date(2, 7, 2026), dc, [], settings)
    with pytest.raises(ItofinError):
        make(callable_bond, finance, cal, dc)
    unpriced = ZeroCouponBond(2, cal, 100., Date(2, 7, 2026), settings)
    forward = make(unpriced, finance, cal, dc)
    with pytest.raises(ItofinError):
        forward.npv()


def test_silent_settings_refresh_and_delivery_event_policy():
    """Prepare the retained facade when silent today/reference flags change."""
    settings, bond, quote, finance, income, cal, dc = setup()
    schedule = Schedule(Date(2, 7, 2024), Date(2, 7, 2026), Frequency.Semiannual, cal, BDC.Unadjusted)
    bond = FixedRateBond(0, 100., schedule, [.05], dc, settings, payment_convention=BDC.Unadjusted)
    bond.set_engine(DiscountingBondEngine(finance, settings))
    settings.set_evaluation_date(Date(2, 7, 2025))
    forward = make(bond, finance, cal, dc, delivery=Date(3, 1, 2026))
    settings.set_include_todays_cash_flows(False)
    excluded = forward.fair_forward_price()
    settings.set_include_todays_cash_flows(True)
    included = forward.fair_forward_price()
    assert included > excluded
    forward.freeze()
    settings.set_include_todays_cash_flows(False)
    assert forward.fair_forward_price() == included
    forward.unfreeze()
    assert forward.fair_forward_price() == pytest.approx(excluded, abs=2e-11)
    settings.set_evaluation_date(Date(3, 1, 2026))
    assert forward.is_expired()
    settings.set_include_reference_date_events(True)
    assert not forward.is_expired()
    assert math.isfinite(forward.npv())


def test_floating_rejection_and_checked_calendar_horizon():
    """Reject unsupported plain floating metadata and guard original horizon."""
    from itofin.indexes import Currency, IborIndex
    from itofin.instruments import FloatingRateBond
    from itofin.time import Period
    settings, bond, quote, finance, income, cal, dc = setup()
    index = IborIndex("forward-unsupported", Period(3, "Months"), 2, Currency.eur(), cal, BDC.Unadjusted, False, dc, finance, settings)
    schedule = Schedule(Date(2, 7, 2024), Date(2, 7, 2026), Frequency.Quarterly, cal, BDC.Unadjusted)
    floating = FloatingRateBond(2, 100., schedule, index, dc, settings)
    with pytest.raises(ItofinError):
        make(floating, finance, cal, dc)
    country = Calendar.turkey()
    guarded = ZeroCouponBond(2, country, 100., Date(2, 7, 2026), settings, payment_convention=BDC.Unadjusted)
    guarded.set_engine(DiscountingBondEngine(finance, settings))
    forward = make(guarded, finance, cal, dc, delivery=Date(2, 7, 2046))
    initial = forward.npv()
    forward.freeze()
    settings.set_evaluation_date(Date(2, 1, 2045))
    assert forward.settlement_date() == Date(2, 1, 2045)
    assert not forward.is_expired()
    assert forward.npv() == initial
    with pytest.raises(ItofinError):
        forward.recalculate()
    assert forward.npv() == initial
    forward.unfreeze()
    with pytest.raises(ItofinError):
        forward.npv()
    settings.set_evaluation_date(Date(3, 7, 2046))
    assert forward.is_expired()
    assert forward.npv() == 0.


def test_live_financing_and_income_roles_and_adjusted_boundaries():
    """Reprice existing forwards for each live curve role and adjusted dates."""
    fixture = json.loads((Path(__file__).parent / "fixtures/bond_forward_native.json").read_text())
    settings, bond, spot_quote, _, _, cal, dc = setup()
    finance_quote, income_quote = SimpleQuote(.04), SimpleQuote(.025)
    finance = FlatForward.from_quote(Date(2, 1, 2025), finance_quote, dc)
    income = FlatForward.from_quote(Date(2, 1, 2025), income_quote, dc)
    forwards = [make(bond, finance, cal, dc), make(bond, finance, cal, dc, income=income)]
    for row in fixture["live_rows"]:
        if row["stage"] == "finance":
            finance_quote.set_value(.07)
        elif row["stage"] == "income":
            income_quote.set_value(.08)
        elif row["stage"] == "spot":
            spot_quote.set_value(.06)
        for forward, expected in zip(forwards, row["values"], strict=True):
            assert [forward.npv(), forward.fair_forward_price(), forward.clean_forward_price()] == pytest.approx(expected, rel=0., abs=2e-11)
    boundary = BondForward(bond, Date(7, 4, 2025), Date(5, 7, 2025), Position.Long, 105., 2, dc, Calendar.weekends_only(), BDC.Following, finance, income)
    assert boundary.settlement_date() == Date(7, 4, 2025)
    assert boundary.delivery_date() == Date(7, 7, 2025)
    assert [boundary.npv(), boundary.fair_forward_price(), boundary.clean_forward_price()] == pytest.approx(fixture["boundary"]["values"], rel=0., abs=2e-11)
