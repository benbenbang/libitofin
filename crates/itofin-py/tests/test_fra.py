"""ForwardRateAgreement and the Position enum (#954).

Ports the core F2 analytic pin (#933,
crates/libitofin/src/instruments/forwardrateagreement.rs:487): the forwarding
and discount curves are deliberately DIFFERENT flat curves (4% vs 6%) so each
shows up only where it belongs - equal curves cannot discriminate
forwarding-vs-discount wiring. The forward rate must be the hand-computed
(D(v)/D(m) - 1) / yf(v, m) off the FORWARDING curve, the amount
notional * sign * (F - K) * T / (1 + F T) for both positions with K != F (the
only Position coverage), and the NPV amount * D(v) off the DISCOUNT curve,
falling back to the forwarding curve when the discount curve is None.

Every expected number is rebuilt here from math.exp and Actual360 day counts
(integer days / 360, the exact fraction the core feeds), the same faced,
independent oracle the FRA-helper tests use.
"""

# standard library
import datetime
import math

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import ForwardRateAgreement, Position
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter, Period

TODAY = Date(15, 6, 2026)
VALUE_DATE = Date(17, 8, 2026)
MATURITY_DATE = Date(17, 11, 2026)
FORWARD_FLAT = 0.04
DISCOUNT_FLAT = 0.06
STRIKE = 0.02
NOTIONAL = 100.0


def _actual360(start: Date, end: Date) -> float:
    days = (
        datetime.date(end.year, end.month, end.day)
        - datetime.date(start.year, start.month, start.day)
    ).days
    return days / 360.0


def _discount(rate: float, date: Date) -> float:
    return math.exp(-rate * _actual360(TODAY, date))


def _same(got: Date, expected: Date) -> bool:
    return (got.year, got.month, got.day) == (
        expected.year,
        expected.month,
        expected.day,
    )


def _index_and_curves() -> tuple[Euribor, FlatForward]:
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    forwarding = FlatForward(TODAY, FORWARD_FLAT, DayCounter.actual360())
    discounting = FlatForward(TODAY, DISCOUNT_FLAT, DayCounter.actual360())
    return Euribor(Period(3, "Months"), forwarding, settings), discounting


def _expected_forward() -> float:
    return (
        _discount(FORWARD_FLAT, VALUE_DATE) / _discount(FORWARD_FLAT, MATURITY_DATE)
        - 1.0
    ) / _actual360(VALUE_DATE, MATURITY_DATE)


def _expected_amount() -> float:
    forward = _expected_forward()
    t = _actual360(VALUE_DATE, MATURITY_DATE)
    return NOTIONAL * (forward - STRIKE) * t / (1.0 + forward * t)


def test_par_forward_rate_amount_and_npv_match_the_closed_form():
    """The F2 pin (forwardrateagreement.rs:487) over DISTINCT curves: the
    forward rate reads the 4% forwarding curve (a port reading the 6% discount
    curve fails by ~2%), the amount is the closed form with K != F, and the NPV
    discounts the amount on the 6% curve."""
    index, discounting = _index_and_curves()
    fra = ForwardRateAgreement.with_maturity(
        index, VALUE_DATE, MATURITY_DATE, Position.Long, STRIKE, NOTIONAL, discounting
    )

    assert _same(fra.value_date(), VALUE_DATE)
    assert _same(fra.maturity_date(), MATURITY_DATE)

    expected_forward = _expected_forward()
    assert abs(expected_forward - STRIKE) > 0.01, (
        "degenerate fixture: K == F would leave the amount pin vacuous"
    )
    assert abs(fra.forward_rate() - expected_forward) < 1.0e-12

    expected_amount = _expected_amount()
    assert abs(fra.amount() - expected_amount) < 1.0e-12

    expected_npv = expected_amount * _discount(DISCOUNT_FLAT, VALUE_DATE)
    assert abs(fra.npv() - expected_npv) < 1.0e-12


def test_long_and_short_positions_have_opposite_signed_values():
    """Position coverage at K != F (the #262-uncovered sign at K == F): the
    short FRA's amount and NPV are the exact negatives of the long ones."""
    index, discounting = _index_and_curves()
    long_fra = ForwardRateAgreement.with_maturity(
        index, VALUE_DATE, MATURITY_DATE, Position.Long, STRIKE, NOTIONAL, discounting
    )
    short_fra = ForwardRateAgreement.with_maturity(
        index, VALUE_DATE, MATURITY_DATE, Position.Short, STRIKE, NOTIONAL, discounting
    )

    assert long_fra.amount() > 0.0, "K < F must favour the long side"
    assert abs(short_fra.amount() + long_fra.amount()) < 1.0e-12
    assert abs(short_fra.npv() + long_fra.npv()) < 1.0e-12


def test_none_discount_curve_falls_back_to_the_forwarding_curve():
    """An absent discount curve discounts on the forwarding curve
    (forwardrateagreement.rs:311): the NPV moves from D(6%) to D(4%)."""
    index, _ = _index_and_curves()
    fra = ForwardRateAgreement.with_maturity(
        index, VALUE_DATE, MATURITY_DATE, Position.Long, STRIKE, NOTIONAL, None
    )

    expected_npv = _expected_amount() * _discount(FORWARD_FLAT, VALUE_DATE)
    assert abs(fra.npv() - expected_npv) < 1.0e-12


def test_indexed_constructor_derives_maturity_and_rate_from_the_index():
    """The indexed-coupon constructor (forwardrateagreement.rs:83): the
    maturity is the index's own maturity of the value date and the forward
    rate is the forecast fixing, the same par formula over that window."""
    index, discounting = _index_and_curves()
    fra = ForwardRateAgreement(
        index, VALUE_DATE, Position.Long, STRIKE, NOTIONAL, discounting
    )

    index_maturity = index.maturity_date(VALUE_DATE)
    assert _same(fra.maturity_date(), index_maturity)

    expected_forward = (
        _discount(FORWARD_FLAT, VALUE_DATE) / _discount(FORWARD_FLAT, index_maturity)
        - 1.0
    ) / _actual360(VALUE_DATE, index_maturity)
    assert abs(fra.forward_rate() - expected_forward) < 1.0e-12


def test_position_enum_identity():
    """Position is a two-variant enum with value equality."""
    assert Position.Long == Position.Long
    assert Position.Short == Position.Short
    assert Position.Long != Position.Short
