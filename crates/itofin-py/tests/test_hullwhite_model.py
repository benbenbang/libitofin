import itofin
import pytest


def _flat_curve():
    dc = itofin.DayCounter.actual365_fixed()
    return itofin.FlatForward(itofin.Date(15, 1, 2026), 0.03, dc)


def _hull_white():
    return itofin.HullWhite(_flat_curve(), 0.05, 0.01)


def test_ctor_round_trips_params():
    hw = _hull_white()
    assert hw.a() == pytest.approx(0.05, abs=1e-12)
    assert hw.sigma() == pytest.approx(0.01, abs=1e-12)


def test_r0_matches_flat_zero_rate():
    hw = _hull_white()
    assert hw.r0() == pytest.approx(0.03, abs=1e-12)


def test_discount_bond_option_finite_nonnegative():
    hw = _hull_white()
    value = hw.discount_bond_option(itofin.OptionType.Call, 0.9, 1.0, 3.0)
    assert value >= 0.0
    assert value == value


def test_discount_bond_option_monotone_in_strike():
    hw = _hull_white()
    itm = hw.discount_bond_option(itofin.OptionType.Call, 0.8, 1.0, 3.0)
    otm = hw.discount_bond_option(itofin.OptionType.Call, 1.1, 1.0, 3.0)
    assert itm > otm


def test_constraint_violation_raises():
    with pytest.raises(itofin.ItofinError):
        itofin.HullWhite(_flat_curve(), 0.05, -0.01)


def test_euribor_six_months_builds():
    settings = itofin.Settings()
    settings.set_evaluation_date(itofin.Date(15, 1, 2026))
    index = itofin.Euribor.six_months(_flat_curve(), settings)
    assert index is not None
