"""Value equality and hashing for Period and DayCounter (#864).

Both facades wrap a core type that defines equality, but neither exposed it, so
Python compared them by identity and neither could key a dict or join a set.

The oracle is the core semantics, not a field comparison. Period equality is
decidable-ordering equality (period.rs:315-320): `partial_cmp` returns None for
a pair no calendar can resolve, which equality reads as not equal. So 7 Days
equals 1 Week and 12 Months equals 1 Year, while 30 Days against 1 Month is not
equal. DayCounter equality is by convention name (daycounter.rs:131-141).

The discriminating cases are the cross-unit ones. A naive hash of the raw
(length, unit) pair satisfies every equality assertion here and still breaks the
hash contract: it would separate 7 Days from 1 Week in a set although they
compare equal. Each cross-unit equality below is therefore paired with a hash
assertion, and the set and dict cases turn that contract into observable
behaviour.

Omitted visibly: the arm reading a DayCounter back off an object that built one
(the from_inner path). The only such getter is the year-on-year coupon's
`day_counter()`, which needs a bootstrapped inflation curve to reach; equality
delegates to the same core PartialEq either way, so the fixture is not worth its
cost here.
"""

from itofin.time import DayCounter, Period


def test_days_and_weeks_compare_and_hash_across_units():
    assert Period(7, "Days") == Period(1, "Weeks")
    assert hash(Period(7, "Days")) == hash(Period(1, "Weeks"))


def test_a_multi_week_length_normalizes_too():
    assert Period(14, "Days") == Period(2, "Weeks")
    assert hash(Period(14, "Days")) == hash(Period(2, "Weeks"))


def test_months_and_years_compare_and_hash_across_units():
    assert Period(12, "Months") == Period(1, "Years")
    assert hash(Period(12, "Months")) == hash(Period(1, "Years"))


def test_every_zero_length_is_the_same_period():
    assert Period(0, "Days") == Period(0, "Months")
    assert Period(0, "Days") == Period(0, "Years")
    assert Period(0, "Days") == Period(0, "Weeks")
    assert hash(Period(0, "Days")) == hash(Period(0, "Months"))
    assert hash(Period(0, "Days")) == hash(Period(0, "Years"))
    assert hash(Period(0, "Days")) == hash(Period(0, "Weeks"))


def test_an_undecidable_pair_is_not_equal():
    """A month is 28 to 31 days, so 30 Days and 1 Month overlap without
    resolving; the core returns None and equality reads that as not equal."""
    assert Period(30, "Days") != Period(1, "Months")


def test_an_exactly_convertible_pair_that_differs_is_not_equal():
    assert Period(2, "Weeks") != Period(15, "Days")


def test_equal_periods_collapse_in_a_set():
    assert len({Period(7, "Days"), Period(1, "Weeks")}) == 1
    assert len({Period(7, "Days"), Period(1, "Months")}) == 2


def test_a_period_keys_a_dict_by_value():
    tenors = {Period(12, "Months"): "annual"}
    assert tenors[Period(1, "Years")] == "annual"


def test_a_period_is_not_equal_to_a_non_period():
    """pyo3 returns False for an argument it cannot extract rather than raising,
    matching Date.__eq__ (time.rs:100)."""
    assert Period(1, "Days") != 1
    assert Period(1, "Days") != "1D"


def test_independently_built_day_counters_compare_and_hash_equal():
    assert DayCounter.actual360() == DayCounter.actual360()
    assert hash(DayCounter.actual360()) == hash(DayCounter.actual360())


def test_different_conventions_are_not_equal():
    assert DayCounter.actual360() != DayCounter.actual365_fixed()
    assert DayCounter.actual_actual_isda() != DayCounter.thirty360_bond_basis()


def test_a_day_counter_keys_a_dict_by_value():
    curves = {DayCounter.actual365_fixed(): "discount"}
    assert curves[DayCounter.actual365_fixed()] == "discount"
    assert DayCounter.actual360() not in curves


def test_a_day_counter_is_not_equal_to_a_non_day_counter():
    assert DayCounter.actual360() != "Actual/360"
