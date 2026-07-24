"""FuturesRateHelper, FuturesType and the IMM date functions (#548).

Ports the core oracles in
crates/libitofin/src/termstructures/yields/ratehelpers.rs. Every expectation is
computed independently: schedule dates from the faced IMM functions (never read
off the helper under test), and the Actual360 year fraction tau from Python's
stdlib day count, because DayCounter.year_fraction is not faced (#553) and PyDate
has no date-minus-date operator. tau = (maturity - earliest).days / 360.0 is the
exact Actual360 fraction the core bootstrap uses (integer day count / 360), so it
reproduces the same f64 rather than a rounded literal.
"""

import datetime

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FuturesRateHelper,
    FuturesType,
    PiecewiseYieldCurve,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
)
from itofin.time import Period as P
from itofin.time import is_imm_date, next_imm_date

# A plain reference date; 15-Jun-2026 is not itself an IMM date (the June 2026
# IMM date is the third Wednesday, 17-Jun), so it doubles as the non-IMM start
# fixture below. The futures helper's dates are absolute, so no evaluation date
# (Settings) is needed for the IMM oracles.
REFERENCE = Date(15, 6, 2026)


def _to_datetime(date: Date) -> datetime.date:
    return datetime.date(date.year, date.month, date.day)


def _actual360(earliest: Date, maturity: Date) -> float:
    """The Actual360 year fraction over the window: exact calendar days / 360,
    the same value the core year_fraction_ref feeds the bootstrap."""
    days = (_to_datetime(maturity) - _to_datetime(earliest)).days
    return days / 360.0


def _imm_window() -> tuple[Date, Date]:
    """The (earliest, maturity) the from_end_date(None) helper pins, derived from
    the IMM functions alone: the first IMM date after REFERENCE, and three IMM
    periods past it (ratehelpers.rs determine_maturity :219)."""
    earliest = next_imm_date(REFERENCE, False)
    maturity = next_imm_date(
        next_imm_date(next_imm_date(earliest, False), False), False
    )
    return earliest, maturity


def test_from_end_date_advances_three_imm_periods_and_pins_the_schedule():
    """Port of ratehelpers.rs:1875: with no end date the maturity is three IMM
    periods past the start and every schedule date pins to that window."""
    earliest, expected_maturity = _imm_window()
    assert is_imm_date(earliest, False)

    helper = FuturesRateHelper.from_end_date(
        SimpleQuote(96.0),
        earliest,
        None,
        DayCounter.actual360(),
        None,
        FuturesType.Imm,
    )

    assert helper.earliest_date() == earliest
    assert helper.maturity_date() == expected_maturity
    assert helper.pillar_date() == expected_maturity
    assert helper.latest_date() == expected_maturity
    assert helper.latest_relevant_date() == expected_maturity


def test_bootstrap_reprices_the_futures_quote():
    """Port of ratehelpers.rs:1822: the forward recomputed from the bootstrapped
    discounts reproduces the quoted price through 100*(1 - forward - c) to 1e-9,
    and implied_quote on the fitted curve returns the price."""
    earliest, maturity = _imm_window()
    price = 96.0
    c = 0.001
    helper = FuturesRateHelper.from_end_date(
        SimpleQuote(price),
        earliest,
        None,
        DayCounter.actual360(),
        SimpleQuote(c),
        FuturesType.Imm,
    )
    curve = PiecewiseYieldCurve(
        REFERENCE, [helper], DayCounter.actual365_fixed(), "LogLinear"
    )

    # Force the lazy bootstrap before reading implied_quote (which needs the
    # helper linked to the curve, done during calculate()).
    disc_e = curve.discount_date(earliest)
    disc_m = curve.discount_date(maturity)
    tau = _actual360(earliest, maturity)
    forward = (disc_e / disc_m - 1.0) / tau
    repriced = 100.0 * (1.0 - forward - c)
    assert abs(repriced - price) <= 1.0e-9, f"repriced {repriced} vs {price}"

    implied = helper.implied_quote()
    assert abs(implied - price) <= 1.0e-9, f"implied {implied} vs {price}"


def _bootstrapped_forward(conv_adj: SimpleQuote | None) -> float:
    """Bootstrap a single-future curve and read the forward back off the
    discounts, the shared body of the convexity-shift pin."""
    earliest, maturity = _imm_window()
    helper = FuturesRateHelper.from_end_date(
        SimpleQuote(96.0),
        earliest,
        None,
        DayCounter.actual360(),
        conv_adj,
        FuturesType.Imm,
    )
    curve = PiecewiseYieldCurve(
        REFERENCE, [helper], DayCounter.actual365_fixed(), "LogLinear"
    )
    disc_e = curve.discount_date(earliest)
    disc_m = curve.discount_date(maturity)
    return (disc_e / disc_m - 1.0) / _actual360(earliest, maturity)


def test_convexity_adjustment_shifts_the_bootstrapped_forward():
    """Port of ratehelpers.rs:1769 (the non-circular pin): two curves at the same
    price, convexity 0 vs c, produce forwards differing by exactly c. Stubbing
    convexity_adjustment to zero would collapse both onto one forward and fail."""
    c = 0.001
    forward_0 = _bootstrapped_forward(None)
    forward_c = _bootstrapped_forward(SimpleQuote(c))
    assert abs(forward_c - (forward_0 - c)) < 1.0e-10, (
        f"forward_c {forward_c} vs forward_0 - c {forward_0 - c}"
    )


def test_convexity_adjustment_reads_the_quote_or_zero():
    """The convexity adjustment is the quote's value, or zero for an empty handle
    (ratehelpers.rs:385)."""
    earliest, _ = _imm_window()
    no_adj = FuturesRateHelper.from_end_date(
        SimpleQuote(96.0), earliest, None, DayCounter.actual360(), None, FuturesType.Imm
    )
    assert no_adj.convexity_adjustment() == 0.0

    with_adj = FuturesRateHelper.from_end_date(
        SimpleQuote(96.0),
        earliest,
        None,
        DayCounter.actual360(),
        SimpleQuote(0.001),
        FuturesType.Imm,
    )
    assert with_adj.convexity_adjustment() == pytest.approx(0.001)


def test_new_constructs_over_a_length_in_months_window():
    """The 9-arg constructor (ratehelpers.rs:260): the maturity is the start
    advanced length_in_months on the calendar, checked against an independent
    Calendar.advance."""
    earliest, _ = _imm_window()
    calendar = Calendar.target()
    convention = BusinessDayConvention.ModifiedFollowing
    helper = FuturesRateHelper(
        SimpleQuote(96.0),
        earliest,
        3,
        calendar,
        convention,
        True,
        DayCounter.actual360(),
        None,
        FuturesType.Imm,
    )
    expected_maturity = calendar.advance(earliest, 3, "Months", convention, True)
    assert helper.earliest_date() == earliest
    assert helper.maturity_date() == expected_maturity


def test_from_index_pins_the_start_to_the_index_window():
    """The index constructor (ratehelpers.rs:331): the window follows the index
    conventions. The fixing calendar and advance-by-period are not faced, so this
    pins the one independently-known fact, earliest == start, plus maturity >
    earliest; the full date math is oracled on the Rust side (:1939)."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    earliest, _ = _imm_window()
    index = Euribor(P(3, "Months"), None, settings)
    helper = FuturesRateHelper.from_index(
        SimpleQuote(96.0), earliest, index, None, FuturesType.Imm
    )
    assert helper.earliest_date() == earliest
    maturity_dt = _to_datetime(helper.maturity_date())
    assert maturity_dt > _to_datetime(earliest)


def test_a_non_imm_start_is_rejected_under_the_imm_convention():
    """Port of ratehelpers.rs:1902: an Imm helper rejects a start that is not an
    IMM date (guard-first, so it cannot pass for the wrong reason)."""
    assert not is_imm_date(REFERENCE, False)
    with pytest.raises(ItofinError):
        FuturesRateHelper.from_end_date(
            SimpleQuote(96.0),
            REFERENCE,
            None,
            DayCounter.actual360(),
            None,
            FuturesType.Imm,
        )


def test_a_custom_helper_requires_an_explicit_end_date():
    """Port of ratehelpers.rs:1922 (a documented divergence from C++): a Custom
    helper with no end date is an error, not a null-maturity helper."""
    with pytest.raises(ItofinError):
        FuturesRateHelper.from_end_date(
            SimpleQuote(96.0),
            REFERENCE,
            None,
            DayCounter.actual360(),
            None,
            FuturesType.Custom,
        )
