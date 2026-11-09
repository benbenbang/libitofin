"""Discount jumps on the node curves (#1388).

QuantLib takes jumps on every interpolated node curve constructor and applies
each jump strictly after its date (yieldtermstructure.cpp:85). Node curves keep
their nodes under jumps, so the oracle is exact: the jumped discount is the
plain discount times the jumps strictly before the query date. A jump on the
reference date never applies, nor does a jump at the query date itself.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError
from itofin.quotes import SimpleQuote
from itofin.termstructures import DiscountCurve, ForwardCurve, ZeroCurve
from itofin.time import Calendar, Date, DayCounter

NODES = [Date(17, 6, 2026), Date(17, 9, 2026), Date(17, 12, 2026), Date(17, 6, 2027), Date(17, 6, 2028)]
RATES = [0.03, 0.031, 0.032, 0.033, 0.034]
DISCOUNTS = [1.0, 0.992, 0.984, 0.967, 0.935]
JUMP_DATES = [NODES[0], Date(1, 8, 2026), Date(1, 3, 2027)]
JUMPS = [0.9, 0.995, 0.99]
QUERIES = [NODES[0], Date(20, 7, 2026), JUMP_DATES[1], Date(2, 8, 2026), JUMP_DATES[2], Date(1, 6, 2027), Date(1, 1, 2029)]



def _key(date):
    return (date.year, date.month, date.day)


KINDS = {
    "linear-zero": lambda **kw: ZeroCurve(NODES, RATES, DayCounter.actual365_fixed(), "Linear", **kw),
    "cubic-zero": lambda **kw: ZeroCurve(NODES, RATES, DayCounter.actual365_fixed(), "Cubic", **kw),
    "loglinear-discount": lambda **kw: DiscountCurve(
        NODES, DISCOUNTS, DayCounter.actual365_fixed(), Calendar.target(), "LogLinear", **kw
    ),
    "cubic-discount": lambda **kw: DiscountCurve(
        NODES, DISCOUNTS, DayCounter.actual365_fixed(), Calendar.target(), "Cubic", **kw
    ),
    "backward-flat-forward": lambda **kw: ForwardCurve(NODES, RATES, DayCounter.actual365_fixed(), **kw),
}


@pytest.mark.parametrize("kind", KINDS)
def test_jumped_discount_is_plain_discount_times_crossed_jumps(kind):
    plain = KINDS[kind]()
    quotes = [SimpleQuote(v) for v in JUMPS]
    jumped = KINDS[kind](jumps=quotes, jump_dates=JUMP_DATES)
    assert jumped.jump_dates() == JUMP_DATES
    for q in QUERIES:
        product = 1.0
        for d, v in zip(JUMP_DATES, JUMPS):
            if _key(NODES[0]) < _key(d) < _key(q):
                product *= v
        p, j = plain.discount_date(q, True), jumped.discount_date(q, True)
        assert j == pytest.approx(p * product, abs=1e-15)
        if q == JUMP_DATES[1]:
            assert j == p
    before = jumped.discount(1.5, True)
    assert before == pytest.approx(plain.discount(1.5, True) * 0.995 * 0.99, abs=1e-15)
    quotes[2].set_value(0.98)
    assert jumped.discount(1.5, True) == pytest.approx(before * 0.98 / 0.99, abs=1e-15)


@pytest.mark.parametrize("kind", KINDS)
def test_empty_jump_dates_generate_year_ends_from_the_first_node(kind):
    curve = KINDS[kind](jumps=[SimpleQuote(0.995), SimpleQuote(0.99)])
    assert curve.jump_dates() == [Date(31, 12, 2026), Date(31, 12, 2027)]


@pytest.mark.parametrize("kind", KINDS)
def test_no_jumps_is_the_shipped_curve(kind):
    plain, default = KINDS[kind](), KINDS[kind](jumps=None, jump_dates=None)
    assert default.jump_dates() == []
    for q in QUERIES:
        assert default.discount_date(q, True) == plain.discount_date(q, True)


@pytest.mark.parametrize("kind", KINDS)
def test_jump_argument_mismatch_raises(kind):
    with pytest.raises(ItofinError, match="mismatch"):
        KINDS[kind](jumps=[SimpleQuote(0.99)], jump_dates=JUMP_DATES)
    with pytest.raises(ItofinError, match="mismatch"):
        KINDS[kind](jump_dates=[JUMP_DATES[1]])
