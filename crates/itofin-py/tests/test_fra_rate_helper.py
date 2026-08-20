"""FraRateHelper and the Pillar enum (#549).

Ports the core oracles in
crates/libitofin/src/termstructures/yields/ratehelpers.rs. PyEuribor does not
face the index inspectors the core tests reconstruct the schedule from
(value_date/fixing_date/tenor/business_day_convention/end_of_month/fixing_calendar,
deferred as #553), so every expected date is rebuilt here from Calendar.advance
(the faced, independent oracle the deposit and futures tests use) plus the fixed
Euribor-3M conventions: TARGET fixing calendar, 2 fixing days (Following), tenor
3M, ModifiedFollowing, end-of-month True. The Actual360 year fraction tau is
integer days / 360 (PyDate has no date-minus-date and DayCounter.year_fraction is
unfaced, #553), the exact fraction the core bootstrap feeds.

The reconstruction shares the core Calendar::advance path with the helper, so a
bug inside advance itself is not caught here (the deposit and futures tests accept
the same limitation); the Rust oracles pin that math directly.
"""

# standard library
import datetime

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, FraRateHelper, PiecewiseLogLinearDiscount, Pillar
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Period

# The core FRA oracles fix the evaluation date at 13-May-2026: the fixture date
# was chosen so the business-day roll separates the from-spot maturity from the
# chained reading (ratehelpers.rs:1994). Any other date risks a degenerate guard
# that passes vacuously, so both relative-window tests reuse it exactly.
EVAL = Date(13, 5, 2026)
MODIFIED_FOLLOWING = BDC.ModifiedFollowing
FOLLOWING = BDC.Following


def _same(got: Date, expected: Date) -> bool:
    return (got.year, got.month, got.day) == (
        expected.year,
        expected.month,
        expected.day,
    )


def _fra_window(eval_date: Date) -> tuple[Date, Date, Date]:
    """Reconstruct (earliest, maturity_from_spot, chained) off TARGET and the
    Euribor-3M conventions, mirroring FraRateHelper::initialize_dates
    (ratehelpers.rs:634-656) and reconstruct_window (:2080-2090). spot is the
    value date (fixing days = 2, Following) off the adjusted evaluation date;
    earliest advances period_to_start (3M) from spot, the faithful maturity the
    combined period_to_start + tenor (6M) from spot, and the chained reading
    advances the tenor (3M) off earliest - the plausible-wrong roll the guard
    rejects. The chained value is also index.maturity_date(earliest), i.e. the
    latest-relevant/pillar date the LastRelevantDate helper pins."""
    calendar = Calendar.target()
    reference = calendar.adjust(eval_date, FOLLOWING)
    spot = calendar.advance(reference, 2, "Days", FOLLOWING, False)
    earliest = calendar.advance(spot, 3, "Months", MODIFIED_FOLLOWING, True)
    maturity_from_spot = calendar.advance(spot, 6, "Months", MODIFIED_FOLLOWING, True)
    chained = calendar.advance(earliest, 3, "Months", MODIFIED_FOLLOWING, True)
    return earliest, maturity_from_spot, chained


def _actual360(start: Date, end: Date) -> float:
    days = (
        datetime.date(end.year, end.month, end.day)
        - datetime.date(start.year, start.month, start.day)
    ).days
    return days / 360.0


def _settings(eval_date: Date) -> Settings:
    settings = Settings()
    settings.set_evaluation_date(eval_date)
    return settings


def test_from_dates_pins_explicit_window_and_ignores_eval_change():
    """Port of ratehelpers.rs:2010: the explicit-date constructor fixes its
    window at construction; a later evaluation-date change does not move it. Par
    mode with the MaturityDate pillar pins the latest-relevant date and pillar at
    the explicit end date."""
    settings = _settings(Date(15, 6, 2026))
    index = Euribor(Period(3, "Months"), None, settings)
    start = Date(15, 9, 2026)
    end = Date(15, 12, 2026)
    helper = FraRateHelper.from_dates(
        SimpleQuote(0.03), start, end, index, False, Pillar.MaturityDate
    )

    assert _same(helper.earliest_date(), start)
    assert _same(helper.maturity_date(), end)
    assert _same(helper.latest_relevant_date(), end)
    assert _same(helper.pillar_date(), end)

    settings.set_evaluation_date(Date(15, 6, 2026) + 90)
    assert _same(helper.earliest_date(), start), (
        "an explicit-date FRA must not shift on an evaluation-date change"
    )
    assert _same(helper.maturity_date(), end)


def test_initialize_dates_advances_maturity_from_spot_not_chained():
    """Port of ratehelpers.rs:1969: the earliest date is period_to_start past spot
    and the maturity is the combined period_to_start + tenor past spot, advanced
    from spot and NOT chained off the earliest date. The degeneracy guard asserts
    the faithful (from-spot) maturity really differs from the chained reading on
    this evaluation date; without the difference the pin would not discriminate
    the trap (ratehelpers.rs:1992)."""
    settings = _settings(EVAL)
    index = Euribor(Period(3, "Months"), None, settings)
    helper = FraRateHelper.from_rate(
        0.03, Period(3, "Months"), index, True, Pillar.LastRelevantDate
    )

    earliest, maturity_from_spot, chained = _fra_window(EVAL)
    assert not _same(maturity_from_spot, chained), (
        "degenerate fixture: the roll must separate the two maturities"
    )
    # Absolute-date pins (typo guard on the reconstruction): the derived window is
    # earliest = 17-Aug-2026, from-spot maturity = 16-Nov-2026, and the chained
    # reading = index.maturity_date(earliest) = 17-Nov-2026.
    assert _same(earliest, Date(17, 8, 2026))
    assert _same(maturity_from_spot, Date(16, 11, 2026))
    assert _same(chained, Date(17, 11, 2026))

    assert _same(helper.earliest_date(), earliest)
    assert _same(helper.maturity_date(), maturity_from_spot)
    # LastRelevantDate pins the node at index.maturity_date(earliest), the chained
    # advance - distinct from the from-spot maturity above (ratehelpers.rs:1999).
    assert _same(helper.latest_relevant_date(), chained)
    assert _same(helper.pillar_date(), chained)
    assert _same(helper.latest_date(), chained)


def test_par_bootstrap_reprices_the_input_rate():
    """Port of ratehelpers.rs:2140: a par-mode FRA (use_indexed_coupon=False) plus
    a 3M deposit anchor bootstrap a Discount/LogLinear curve; recomputing the
    simple forward (disc(start)/disc(end) - 1)/tau off the curve over an
    independently reconstructed window reproduces the 0.03 input to 1e-9. The
    forward is rebuilt from the curve discounts, NOT from implied_quote (which is
    the bootstrap's own root and would pass tautologically). The second degeneracy
    guard asserts the window end differs from the chained advance (:2176)."""
    settings = _settings(EVAL)
    calendar = Calendar.target()
    settlement = calendar.advance(EVAL, 2, "Days", FOLLOWING, False)

    deposit_index = Euribor(Period(3, "Months"), None, settings)
    deposit = DepositRateHelper.from_rate(0.02, deposit_index)
    fra_index = Euribor(Period(3, "Months"), None, settings)
    fra = FraRateHelper.from_rate(
        0.03, Period(3, "Months"), fra_index, False, Pillar.LastRelevantDate
    )
    curve = PiecewiseLogLinearDiscount(
        settlement, [deposit, fra], DayCounter.actual360()
    )

    start, end, chained_end = _fra_window(EVAL)
    assert not _same(end, chained_end), (
        "degenerate fixture: the maturity trap does not bite on this window, so "
        "the par reprice would not discriminate it"
    )

    tau = _actual360(start, end)
    discount_start = curve.discount_date(start)
    discount_end = curve.discount_date(end)
    estimated = (discount_start / discount_end - 1.0) / tau
    assert abs(estimated - 0.03) <= 1.0e-9, f"par reprice {estimated} vs 0.03"


def test_new_retains_the_caller_quote_and_bootstraps_in_a_strip():
    """The primary constructor (the one the mixed strip uses) retains the caller's
    SimpleQuote so a set_value re-drives the bootstrap, and the default arguments
    (use_indexed_coupon=True, pillar=LastRelevantDate) construct a working strip
    node. Par-mode repricing lives in the test above; this pins the retained-quote
    wiring and the default-argument path."""
    settings = _settings(EVAL)
    index = Euribor(Period(3, "Months"), None, settings)
    quote = SimpleQuote(0.03)
    helper = FraRateHelper(quote, Period(3, "Months"), index)

    assert helper.quote_value() == 0.03
    quote.set_value(0.035)
    assert helper.quote_value() == 0.035
