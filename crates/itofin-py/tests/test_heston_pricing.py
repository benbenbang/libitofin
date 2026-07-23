import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import HestonModel
from itofin.processes import HestonProcess
from itofin.time import Date, DayCounter


def _heston_arm1():
    s = Settings()
    s.set_evaluation_date(Date(27, 12, 2004))
    dc = DayCounter.actual_actual_isda()
    proc = HestonProcess(
        0.0225,
        0.02,
        1.0,
        0.1,
        3.16,
        0.09,
        0.4,
        -0.2,
        Date(27, 12, 2004),
        dc,
    )
    model = HestonModel(proc)
    opt = VanillaOption(OptionType.Call, 1.05, Date(28, 3, 2005), s)
    opt.set_heston_engine(model, 64)
    return opt


def test_arm1_cached_analytic_price_order_64():
    opt = _heston_arm1()
    assert opt.npv() == pytest.approx(0.0404774515, abs=1e-8)


def test_heston_path_greeks_not_provided():
    opt = _heston_arm1()
    opt.npv()
    with pytest.raises(ItofinError):
        opt.delta()


def test_model_constraint_violation_raises():
    proc = HestonProcess(
        0.0225,
        0.02,
        1.0,
        -0.1,
        3.16,
        0.09,
        0.4,
        -0.2,
        Date(27, 12, 2004),
        DayCounter.actual_actual_isda(),
    )
    with pytest.raises(ItofinError):
        HestonModel(proc)
