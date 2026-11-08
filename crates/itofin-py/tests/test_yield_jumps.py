"""PiecewiseYieldCurve discount jumps (#1320).

QuantLib takes jumps on every PiecewiseYieldCurve constructor
(ql/termstructures/yield/piecewiseyieldcurve.hpp:74-84,101-112) and applies
each jump strictly after its date (yieldtermstructure.cpp:59-95). The oracle is
closed form: one 3M deposit pins the total discount at its pillar t1, so a
log-linear discount curve with a jump J at tj < t1 re-solves its node to
plain / J, and the jumped over plain discount ratio is J**(-t / t1) before the
jump and J**(1 - t / t1) after it. A dropped jump gives a ratio of 1.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, FlatForward, IterativeBootstrapOptions, PiecewiseYieldCurve
from itofin.time import Calendar, Date, DayCounter
from itofin.time import Period as P

JUMP = 0.99
JUMP_DATE = Date(1, 8, 2026)
T_JUMP, T_PILLAR = 45 / 365, 92 / 365

MODES = {
    "iterative": {},
    "iterative-options": {"iterative_options": IterativeBootstrapOptions(accuracy=1e-14)},
    "global": {"bootstrap": "global"},
    "global-callbacks": {"bootstrap": "global", "additional_penalties": lambda times, data: []},
}


def _settings():
    settings = Settings()
    settings.set_evaluation_date(Date(15, 6, 2026))
    return settings


def _helpers(settings):
    return [DepositRateHelper(SimpleQuote(0.03), Euribor(P(3, "Months"), None, settings))]


def _curve(settings, moving, jumps=None, jump_dates=None, **kwargs):
    dc = DayCounter.actual365_fixed()
    if jumps is None:
        if moving:
            return PiecewiseYieldCurve.moving(2, Calendar.target(), _helpers(settings), dc, settings, **kwargs)
        return PiecewiseYieldCurve(Date(17, 6, 2026), _helpers(settings), dc, **kwargs)
    if moving:
        return PiecewiseYieldCurve.moving_with_jumps(
            2, Calendar.target(), _helpers(settings), dc, settings, jumps, jump_dates, **kwargs
        )
    return PiecewiseYieldCurve.with_jumps(Date(17, 6, 2026), _helpers(settings), dc, jumps, jump_dates, **kwargs)


@pytest.mark.parametrize("moving", [False, True], ids=["fixed", "moving"])
@pytest.mark.parametrize("mode", MODES)
def test_jumps_enter_the_bootstrap_on_every_mode(mode, moving):
    settings = _settings()
    plain = _curve(settings, moving, **MODES[mode])
    quote = SimpleQuote(JUMP)
    jumped = _curve(settings, moving, [quote], [JUMP_DATE], **MODES[mode])
    assert jumped.jump_dates() == [JUMP_DATE]
    assert jumped.jump_times() == pytest.approx([T_JUMP], abs=1e-15)
    for t in (0.02, 0.1, T_JUMP + 0.01, 0.2, T_PILLAR):
        power = -t / T_PILLAR if t < T_JUMP else 1 - t / T_PILLAR
        ratio = jumped.discount(t) / plain.discount(t)
        assert abs(ratio - JUMP**power) <= 1e-10, (t, ratio)
    before = jumped.discount(0.2)
    quote.set_value(0.98)
    after = jumped.discount(0.2)
    assert abs(after - before) > 1e-5
    assert abs(after / plain.discount(0.2) - 0.98 ** (1 - 0.2 / T_PILLAR)) <= 1e-10


def test_moving_jumps_generate_year_ends_and_roll_like_a_fresh_curve():
    settings = _settings()
    quotes = [SimpleQuote(0.995), SimpleQuote(0.99)]
    curve = _curve(settings, True, quotes)
    assert curve.jump_dates() == [Date(31, 12, 2026), Date(31, 12, 2027)]
    settings.set_evaluation_date(Date(16, 6, 2026))
    assert curve.reference_date() == Date(18, 6, 2026)
    fresh = PiecewiseYieldCurve.with_jumps(
        Date(18, 6, 2026), _helpers(settings), DayCounter.actual365_fixed(), quotes, curve.jump_dates()
    )
    assert curve.jump_times() == pytest.approx(fresh.jump_times(), abs=1e-15)
    for t in (0.05, 0.2, 0.25):
        assert abs(curve.discount(t) - fresh.discount(t)) <= 1e-12


def test_unset_evaluation_date_raises_rather_than_reading_a_clock():
    helpers = _helpers(_settings())
    unset = Settings()
    dc = DayCounter.actual365_fixed()
    with pytest.raises(ItofinError):
        PiecewiseYieldCurve.moving_with_jumps(2, Calendar.target(), helpers, dc, unset, [SimpleQuote(JUMP)])
    curve = PiecewiseYieldCurve.moving_with_jumps(
        2, Calendar.target(), helpers, dc, unset, [SimpleQuote(JUMP)], [JUMP_DATE]
    )
    with pytest.raises(ItofinError):
        curve.discount(0.1)


def test_bad_jump_arguments_raise_and_jump_free_curves_report_none():
    settings = _settings()
    with pytest.raises(ItofinError):
        _curve(settings, False, [SimpleQuote(JUMP)], [JUMP_DATE, Date(1, 3, 2027)])
    with pytest.raises(ItofinError):
        _curve(settings, False, [], [JUMP_DATE])
    with pytest.raises(ItofinError):
        _curve(settings, False, [SimpleQuote(JUMP)], [JUMP_DATE], bootstrap="global", interpolation="Cubic")
    assert _curve(settings, False).jump_dates() == []
    flat = FlatForward(Date(17, 6, 2026), 0.03, DayCounter.actual365_fixed())
    assert flat.jump_dates() == [] and flat.jump_times() == []
