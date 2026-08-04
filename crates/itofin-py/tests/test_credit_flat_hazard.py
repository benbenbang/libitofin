"""FlatHazardRate and the DefaultProbabilityTermStructure base (#738).

Self-contained closed-form oracle, mirroring the Rust one in
crates/libitofin/src/termstructures/credit/flathazardrate.rs
(`flat_hazard_rate_reproduces_the_closed_form_default_probability`, itself
`testFlatHazardRate` in defaultprobabilitycurves.cpp:118-149). A flat curve at
hazard rate h answers survival exp(-h t), default probability 1 - exp(-h t),
default density h exp(-h t) and hazard rate h at every maturity, so every faced
method is checked against arithmetic computed here rather than against a value
read back off the curve.

Times come out of Actual360, so a 360-day offset from the reference date is the
year fraction 1.0 exactly and the date-form methods are comparable to the same
closed form.
"""

import math

import pytest
from itofin import ItofinError, Settings
from itofin.instruments import ProtectionSide
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatHazardRate
from itofin.time import Calendar, Date, DayCounter

HAZARD_RATE = 0.02
REFERENCE = Date(15, 6, 2026)
TIMES = (0.5, 1.0, 5.0)
TOLERANCE = 1e-12


def _day_counter() -> DayCounter:
    return DayCounter.actual360()


def test_rate_form_reproduces_the_closed_form():
    curve = FlatHazardRate.with_rate(REFERENCE, HAZARD_RATE, _day_counter())

    for t in TIMES:
        survival = math.exp(-HAZARD_RATE * t)
        assert abs(curve.survival_probability(t) - survival) <= TOLERANCE
        assert abs(curve.default_probability(t) - (1.0 - survival)) <= TOLERANCE
        assert abs(curve.default_density(t) - HAZARD_RATE * survival) <= TOLERANCE
        assert abs(curve.hazard_rate(t) - HAZARD_RATE) <= TOLERANCE


def test_quote_form_reproduces_the_closed_form_and_tracks_the_quote():
    quote = SimpleQuote(HAZARD_RATE)
    curve = FlatHazardRate(REFERENCE, quote, _day_counter())

    for t in TIMES:
        survival = math.exp(-HAZARD_RATE * t)
        assert abs(curve.survival_probability(t) - survival) <= TOLERANCE
        assert abs(curve.default_probability(t) - (1.0 - survival)) <= TOLERANCE
        assert abs(curve.default_density(t) - HAZARD_RATE * survival) <= TOLERANCE
        assert abs(curve.hazard_rate(t) - HAZARD_RATE) <= TOLERANCE

    quote.set_value(0.05)
    for t in TIMES:
        assert abs(curve.survival_probability(t) - math.exp(-0.05 * t)) <= TOLERANCE
        assert abs(curve.hazard_rate(t) - 0.05) <= TOLERANCE


def test_date_forms_agree_with_the_year_fraction_forms():
    curve = FlatHazardRate.with_rate(REFERENCE, HAZARD_RATE, _day_counter())
    one_year = REFERENCE + 360
    survival = math.exp(-HAZARD_RATE * 1.0)

    assert abs(curve.survival_probability_date(one_year) - survival) <= TOLERANCE
    assert abs(curve.default_probability_date(one_year) - (1.0 - survival)) <= TOLERANCE
    assert abs(curve.default_density_date(one_year) - HAZARD_RATE * survival) <= TOLERANCE
    assert abs(curve.hazard_rate_date(one_year) - HAZARD_RATE) <= TOLERANCE


def test_moving_curve_errors_before_an_evaluation_date_is_set():
    settings = Settings()
    curve = FlatHazardRate.moving_with_rate(
        2, Calendar.target(), HAZARD_RATE, _day_counter(), settings
    )

    with pytest.raises(ItofinError):
        curve.survival_probability_date(REFERENCE + 360)
    with pytest.raises(ItofinError):
        curve.survival_probability(1.0)


def test_moving_curve_answers_once_the_evaluation_date_is_set():
    settings = Settings()
    quote = SimpleQuote(HAZARD_RATE)
    curve = FlatHazardRate.moving(
        2, Calendar.target(), quote, _day_counter(), settings
    )
    settings.set_evaluation_date(REFERENCE)

    for t in TIMES:
        survival = math.exp(-HAZARD_RATE * t)
        assert abs(curve.survival_probability(t) - survival) <= TOLERANCE
        assert abs(curve.hazard_rate(t) - HAZARD_RATE) <= TOLERANCE


def test_moving_curve_reference_date_honours_the_settlement_days():
    settings = Settings()
    spot = FlatHazardRate.moving_with_rate(
        0, Calendar.target(), HAZARD_RATE, _day_counter(), settings
    )
    settled = FlatHazardRate.moving_with_rate(
        2, Calendar.target(), HAZARD_RATE, _day_counter(), settings
    )
    settings.set_evaluation_date(REFERENCE)
    one_year = REFERENCE + 360

    assert abs(spot.survival_probability_date(one_year) - math.exp(-HAZARD_RATE)) <= TOLERANCE
    assert settled.survival_probability_date(one_year) > spot.survival_probability_date(one_year)


def test_protection_side_variants_are_distinct():
    assert ProtectionSide.Buyer == ProtectionSide.Buyer
    assert ProtectionSide.Buyer != ProtectionSide.Seller
