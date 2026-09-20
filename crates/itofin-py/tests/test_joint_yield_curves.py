"""QuantLib piecewiseyieldcurve.cpp testMultiCurveTwoPiecewiseYieldCurves parity."""

import gc

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import ForwardRateAgreement, Position
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    FraRateHelper,
    IborIborBasisSwapRateHelper,
    JointYieldCurves,
    SwapRateHelper,
)
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule


class Market:
    """The upstream nine-FRA/twelve-basis/nine-swap mutually coupled market."""

    def __init__(self, today=None, settlement_days=2, fixing_fixture=False):
        """Retain market inputs independently of the native assembly."""
        self.today = today or Date(23, 10, 2025)
        self.settings = Settings()
        self.settings.set_evaluation_date(self.today)
        self.calendar = Calendar.target()
        self.dc = DayCounter.actual360()
        self.indices = [Euribor(Period(months, "Months"), None, self.settings) for months in [3, 6]]
        self.quote, self.basis, self.discount_quote = map(SimpleQuote, [0.03, 0.002, 0.02])
        self.discount = FlatForward.from_quote(self.advance(self.today, 2, "Days"), self.discount_quote, self.dc)
        self.settlement_days = settlement_days
        self.fixing_fixture = fixing_fixture
        self.fixings = []

    def advance(self, date, number, unit):
        """Apply the Euribor calendar conventions."""
        return self.calendar.advance(date, number, unit, BDC.ModifiedFollowing, False)

    def template(self, months, side, indices=None):
        """Build a reusable standalone basis-helper template."""
        base, other = indices or self.indices
        return IborIborBasisSwapRateHelper(
            self.basis,
            Period(months, "Months"),
            self.settlement_days,
            self.calendar,
            BDC.ModifiedFollowing,
            False,
            base,
            other,
            self.discount,
            side,
        )

    def strips(self):
        """Create fresh unassigned helper instances for each assembly."""
        first = [FraRateHelper.from_months(self.quote, i, self.indices[0]) for i in range(1, 10)]
        second = [
            SwapRateHelper(
                self.quote,
                Period(i, "Years"),
                self.calendar,
                Frequency.Annual,
                BDC.Following,
                DayCounter.thirty360_bond_basis(),
                self.indices[1],
                self.discount,
            )
            for i in range(2, 11)
        ]
        basis = [self.template(i * 12, True) for i in range(2, 11)]
        basis += [self.template(i * 6, False) for i in range(2 if self.fixing_fixture else 1, 4)]
        return first, second, basis

    def build(self):
        """Assemble both global curves with their private mutual forecast links."""
        return JointYieldCurves(self.today, *self.strips(), self.dc)

    def leg(self, index, curve, months, frequency, spread=0.0):
        """Rebuild independent floating coupons from public schedule and index APIs."""
        spot = self.advance(self.today, self.settlement_days, "Days")
        maturity = self.advance(spot, months, "Months")
        dates = Schedule(spot, maturity, frequency, self.calendar, BDC.ModifiedFollowing).dates()

        def rate(start, end):
            fixing = index.fixing_date(start)
            if self.dc.year_fraction(fixing, self.today) > 0.0 or (
                fixing == self.today and index.name() in self.fixings
            ):
                return index.fixing(fixing, False)
            value = index.value_date(fixing)
            end_value = index.value_date(index.fixing_date(end))
            return (curve.discount_date(value) / curve.discount_date(end_value) - 1.0) / self.dc.year_fraction(
                value, end_value
            )

        return sum(
            (rate(start, end) + spread) * self.dc.year_fraction(start, end) * self.discount.discount_date(end)
            for start, end in zip(dates, dates[1:])
        )

    def reprice(self, owner):
        """Enforce the upstream FRA relative and swap absolute tolerances."""
        curves = [owner.curve(i) for i in [0, 1]]
        indices = [Euribor(Period(months, "Months"), curve, self.settings) for months, curve in zip([3, 6], curves)]
        spot = self.advance(self.today, 2, "Days")
        for i in range(1, 10):
            fra = ForwardRateAgreement(
                indices[0], self.advance(spot, i, "Months"), Position.Long, self.quote.value(), 1.0, curves[0]
            )
            assert abs(fra.forward_rate() - self.quote.value()) <= abs(self.quote.value()) * 1e-12
        for months in ([12, 18] if self.fixing_fixture else [6, 12, 18]) + list(range(24, 121, 12)):
            npv = self.leg(indices[0], curves[0], months, Frequency.Quarterly, self.basis.value())
            npv -= self.leg(indices[1], curves[1], months, Frequency.Semiannual)
            assert abs(npv) < 1e-10
        for years in range(2, 11):
            maturity = self.advance(spot, years, "Years")
            dates = Schedule(spot, maturity, Frequency.Annual, self.calendar, BDC.Following).dates()
            fixed = sum(
                self.quote.value()
                * DayCounter.thirty360_bond_basis().year_fraction(a, b)
                * self.discount.discount_date(b)
                for a, b in zip(dates, dates[1:])
            )
            floating = self.leg(indices[1], curves[1], years * 12, Frequency.Semiannual)
            assert abs(fixed - floating) < 1e-10


def sample(owner):
    """Warm each member at a short and long maturity."""
    return [owner.curve(i).discount(t) for i in [0, 1] for t in [0.7, 7.0]]


def test_joint_quantlib_repricing_and_template_contract():
    """All four upstream instrument loops pass through exported owner-backed curves."""
    market = Market()
    first, second, basis = market.strips()
    owner = JointYieldCurves(market.today, first, second, basis, market.dc)
    market.reprice(owner)
    assert basis[0].quote_value() == 0.002
    assert market.dc.year_fraction(basis[0].earliest_date(), basis[0].maturity_date()) > 0.0
    with pytest.raises(ItofinError):
        basis[0].implied_quote()
    with pytest.raises(ItofinError, match="already belongs"):
        JointYieldCurves(market.today, first, second, basis, market.dc)


@pytest.mark.parametrize("change", ["basis", "quote", "discount", "date"])
def test_warmed_joint_recalculates_like_fresh(change):
    """Live market changes propagate across both members of the warmed solve."""
    market = Market()
    owner = market.build()
    before = sample(owner)
    if change == "date":
        market.today = Date(24, 10, 2025)
        market.settings.set_evaluation_date(market.today)
    else:
        quote = {"basis": market.basis, "quote": market.quote, "discount": market.discount_quote}[change]
        quote.set_value(quote.value() + 0.001)
    after = sample(owner)
    assert max(abs(a - b) for a, b in zip(before, after)) > 1e-8
    fresh = JointYieldCurves(Date(23, 10, 2025), *market.strips(), market.dc)
    assert after == pytest.approx(sample(fresh), abs=1e-11, rel=0.0)
    market.reprice(owner)


@pytest.mark.parametrize("member", [0, 1])
def test_exported_curve_survives_owner_inputs_and_other_member(member):
    """A pending recalculation remains valid after Python owners are collected."""
    market = Market()
    owner = market.build()
    curve = owner.curve(member)
    before = curve.discount(7.0)
    quote = market.basis
    quote.set_value(0.0025)
    fresh = market.build().curve(member).discount(7.0)
    del owner, market
    gc.collect()
    assert abs(curve.discount(7.0) - before) > 1e-8
    assert curve.discount(7.0) == pytest.approx(fresh, abs=1e-11, rel=0.0)
    del quote
    gc.collect()
    assert curve.discount(7.0) == pytest.approx(fresh, abs=1e-11, rel=0.0)


@pytest.mark.parametrize("accuracy", [0.0, -1.0, float("nan"), float("inf")])
def test_invalid_joint_accuracy_is_ordinary_error(accuracy):
    """Reject malformed numerical options before assigning any helper."""
    market = Market()
    with pytest.raises(ItofinError, match="accuracy"):
        JointYieldCurves(market.today, *market.strips(), market.dc, accuracy)


def test_invalid_membership_and_settings():
    """Reject unsupported membership, duplicate helpers and mismatched index stores."""
    market = Market()
    first, second, basis = market.strips()
    with pytest.raises(ItofinError, match="both members"):
        JointYieldCurves(market.today, first, second, basis[:9], market.dc)
    with pytest.raises(ItofinError, match="duplicate"):
        JointYieldCurves(market.today, first + first[:1], second, basis, market.dc)
    owner = JointYieldCurves(market.today, first, second, basis, market.dc)
    with pytest.raises(ItofinError, match="member"):
        owner.curve(2)
    foreign = Market().indices[1]
    with pytest.raises(ItofinError, match="settings"):
        market.template(12, False, [market.indices[0], foreign])
    with pytest.raises(ItofinError):
        market.template(0, False)


@pytest.mark.parametrize("side", [0, 1])
def test_todays_fixings_notify_both_bootstrap_roles(side):
    """Each original index feeds a kept leg and a cloned fitted leg internally."""
    market = Market(fixing_fixture=True)
    owner = market.build()
    before = sample(owner)
    market.indices[side].add_fixing(market.today, 0.031 + side * 0.002)
    market.fixings.append(market.indices[side].name())
    after = sample(owner)
    assert max(abs(a - b) for a, b in zip(before, after)) > 1e-8
    assert after == pytest.approx(sample(market.build()), abs=1e-11, rel=0.0)
    market.reprice(owner)


def test_missing_past_fixings_recover_and_do_not_poison_assembly():
    """A failed lazy query can recover after the shared index histories arrive."""
    market = Market(settlement_days=0, fixing_fixture=True)
    market.discount = FlatForward.from_quote(market.today, market.discount_quote, market.dc)
    owner = market.build()
    with pytest.raises(ItofinError, match="fixing"):
        sample(owner)
    fixing_date = market.indices[0].fixing_date(market.today)
    for i, index in enumerate(market.indices):
        index.add_fixing(fixing_date, 0.031 + i * 0.002)
    assert sample(owner) == pytest.approx(sample(market.build()), abs=1e-11, rel=0.0)
    for index in market.indices:
        assert index.fixing(fixing_date, False) > 0.0
    with pytest.raises(ItofinError):
        market.indices[0].add_fixing(fixing_date, 0.5)
    with pytest.raises(ItofinError, match="finite"):
        market.indices[0].add_fixing(market.today, float("nan"))


@pytest.mark.parametrize("tenor", [0, -1, 2147483647])
def test_swap_helper_schedule_failures_are_ordinary_errors(tenor):
    """Invalid exogenous-discount helper schedules cannot escape as Rust panics."""
    market = Market()
    with pytest.raises(ItofinError):
        SwapRateHelper(
            market.quote,
            Period(tenor, "Years"),
            market.calendar,
            Frequency.Annual,
            BDC.Following,
            DayCounter.thirty360_bond_basis(),
            market.indices[1],
            market.discount,
        )
    market.reprice(market.build())
