"""The generic IborIndex facade and the re-typed deposit/swap helpers (#815).

The discriminating oracle is an equivalence pin: an IborIndex spelled out with
the Euribor 3M conventions (euribor.rs:53-58 plus the tenor-dependent
euribor_convention/euribor_eom at :126-141, whose Months arms give
ModifiedFollowing and end-of-month) must bootstrap the same curve as the Euribor
facade itself. Nothing is read back off the index; the two curves are compared
against each other.

A mid-month evaluation date leaves the convention and end-of-month flags inert -
the roll never crosses a month boundary, so a wrong value for either would still
pass. The pin therefore runs at three evaluation dates, two of them chosen so
those flags bite, and test_wrong_conventions_do_diverge proves they bite by
showing a deliberately mis-specified index diverging at exactly those dates.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import Currency, Euribor, IborIndex
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, PiecewiseLogLinearDiscount, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period

DEPOSIT_RATE = 0.04557

# A plain mid-month date, one whose value date is a month end (so end-of-month
# bites), and one whose 3M roll crosses a month boundary (so ModifiedFollowing
# bites, parting from Following).
PLAIN_DATE = Date(15, 6, 2026)
END_OF_MONTH_DATE = Date(25, 2, 2026)
MONTH_CROSSING_DATE = Date(25, 11, 2026)
EVALUATION_DATES = [PLAIN_DATE, END_OF_MONTH_DATE, MONTH_CROSSING_DATE]


def _settings_on(evaluation_date, calendar):
    today = calendar.adjust(evaluation_date, BusinessDayConvention.Following)
    settings = Settings()
    settings.set_evaluation_date(today)
    return settings, today


def _euribor_3m_conventions(settings, calendar):
    """The Euribor 3M configuration spelled out field by field."""
    return IborIndex(
        "Euribor",
        Period(3, "Months"),
        2,
        Currency.eur(),
        calendar,
        BusinessDayConvention.ModifiedFollowing,
        True,
        DayCounter.actual360(),
        None,
        settings,
    )


def _deposit_curve(index, settlement_date):
    helper = DepositRateHelper(SimpleQuote(DEPOSIT_RATE), index)
    return PiecewiseLogLinearDiscount(
        settlement_date, [helper], DayCounter.actual365_fixed()
    )


def _settlement_and_probe(calendar, today):
    settlement = calendar.advance(
        today, 2, "Days", BusinessDayConvention.Following, False
    )
    probe = calendar.advance(
        settlement, 2, "Months", BusinessDayConvention.Following, False
    )
    return settlement, probe


@pytest.mark.parametrize("evaluation_date", EVALUATION_DATES)
def test_generic_index_bootstraps_the_same_curve_as_euribor(evaluation_date):
    calendar = Calendar.target()
    settings, today = _settings_on(evaluation_date, calendar)
    settlement, probe = _settlement_and_probe(calendar, today)

    generic = _euribor_3m_conventions(settings, calendar)
    euribor = Euribor(Period(3, "Months"), None, settings)

    generic_df = _deposit_curve(generic, settlement).discount_date(probe)
    euribor_df = _deposit_curve(euribor, settlement).discount_date(probe)

    assert generic_df == pytest.approx(euribor_df, abs=1e-12)


@pytest.mark.parametrize(
    ("evaluation_date", "convention", "end_of_month"),
    [
        (END_OF_MONTH_DATE, BusinessDayConvention.ModifiedFollowing, False),
        (MONTH_CROSSING_DATE, BusinessDayConvention.Following, True),
    ],
)
def test_wrong_conventions_do_diverge(evaluation_date, convention, end_of_month):
    """The equivalence pin is not vacuous: at these dates a single wrong
    convention field moves the bootstrapped curve, so the pin above would have
    caught it."""
    calendar = Calendar.target()
    settings, today = _settings_on(evaluation_date, calendar)
    settlement, probe = _settlement_and_probe(calendar, today)

    wrong = IborIndex(
        "Euribor",
        Period(3, "Months"),
        2,
        Currency.eur(),
        calendar,
        convention,
        end_of_month,
        DayCounter.actual360(),
        None,
        settings,
    )
    euribor = Euribor(Period(3, "Months"), None, settings)

    wrong_df = _deposit_curve(wrong, settlement).discount_date(probe)
    euribor_df = _deposit_curve(euribor, settlement).discount_date(probe)

    assert wrong_df != pytest.approx(euribor_df, abs=1e-12)


def test_isda_style_index_bootstraps_a_deposit_and_swap_strip():
    """A build-check for the USD-3M weekends-only index the ISDA CDS curve needs.

    The Markit cached values are #816's oracle, not this one; all that is pinned
    here is that a non-Euribor index reaches a bootstrapped curve at all.
    """
    calendar = Calendar.weekends_only()
    settings, today = _settings_on(PLAIN_DATE, calendar)
    settlement, probe = _settlement_and_probe(calendar, today)

    index = IborIndex(
        "IsdaIbor",
        Period(3, "Months"),
        2,
        Currency.usd(),
        calendar,
        BusinessDayConvention.ModifiedFollowing,
        False,
        DayCounter.actual360(),
        None,
        settings,
    )
    deposit = DepositRateHelper(SimpleQuote(DEPOSIT_RATE), index)
    swap = SwapRateHelper(
        SimpleQuote(0.05),
        Period(2, "Years"),
        calendar,
        Frequency.Annual,
        BusinessDayConvention.Unadjusted,
        DayCounter.thirty360_bond_basis(),
        index,
    )
    curve = PiecewiseLogLinearDiscount(
        settlement, [deposit, swap], DayCounter.actual365_fixed()
    )

    discount = curve.discount_date(probe)
    assert 0.0 < discount < 1.0


def test_euribor_is_an_ibor_index():
    calendar = Calendar.target()
    settings, _ = _settings_on(PLAIN_DATE, calendar)
    assert isinstance(Euribor(Period(3, "Months"), None, settings), IborIndex)


def test_deposit_helper_still_takes_a_euribor():
    calendar = Calendar.target()
    settings, today = _settings_on(PLAIN_DATE, calendar)
    settlement, probe = _settlement_and_probe(calendar, today)

    euribor = Euribor(Period(3, "Months"), None, settings)
    curve = _deposit_curve(euribor, settlement)
    assert 0.0 < curve.discount_date(probe) < 1.0


def test_generic_index_fixing_forecasts_off_its_curve():
    """The forwarding-curve arm of the constructor, which every other test here
    passes None for: an index reading the curve a deposit of DEPOSIT_RATE
    bootstrapped forecasts that same rate back over the deposit's own window."""
    calendar = Calendar.target()
    settings, today = _settings_on(PLAIN_DATE, calendar)
    settlement, _ = _settlement_and_probe(calendar, today)

    bootstrapped = _deposit_curve(
        _euribor_3m_conventions(settings, calendar), settlement
    )
    forecasting = IborIndex(
        "Euribor",
        Period(3, "Months"),
        2,
        Currency.eur(),
        calendar,
        BusinessDayConvention.ModifiedFollowing,
        True,
        DayCounter.actual360(),
        bootstrapped,
        settings,
    )
    assert forecasting.fixing(today, False) == pytest.approx(DEPOSIT_RATE, abs=1e-10)
