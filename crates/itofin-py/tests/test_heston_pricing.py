import itofin
import pytest


def _heston_arm1():
    s = itofin.Settings()
    s.set_evaluation_date(itofin.Date(27, 12, 2004))
    dc = itofin.DayCounter.actual_actual_isda()
    proc = itofin.HestonProcess(
        0.0225,
        0.02,
        1.0,
        0.1,
        3.16,
        0.09,
        0.4,
        -0.2,
        itofin.Date(27, 12, 2004),
        dc,
    )
    model = itofin.HestonModel(proc)
    opt = itofin.VanillaOption(itofin.OptionType.Call, 1.05, itofin.Date(28, 3, 2005), s)
    opt.set_heston_engine(model, 64)
    return opt


def test_arm1_cached_analytic_price_order_64():
    opt = _heston_arm1()
    assert opt.npv() == pytest.approx(0.0404774515, abs=1e-8)


def test_heston_path_greeks_not_provided():
    opt = _heston_arm1()
    opt.npv()
    with pytest.raises(itofin.ItofinError):
        opt.delta()


def test_model_constraint_violation_raises():
    proc = itofin.HestonProcess(
        0.0225,
        0.02,
        1.0,
        -0.1,
        3.16,
        0.09,
        0.4,
        -0.2,
        itofin.Date(27, 12, 2004),
        itofin.DayCounter.actual_actual_isda(),
    )
    with pytest.raises(itofin.ItofinError):
        itofin.HestonModel(proc)
