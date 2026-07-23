import itofin
import pytest

TOL = 1e-10


def _market_row_1():
    s = itofin.Settings()
    s.set_evaluation_date(itofin.Date(15, 6, 2026))
    dc = itofin.DayCounter.actual360()
    proc = itofin.BlackScholesProcess(60.0, 0.08, 0.0, 0.30, itofin.Date(15, 6, 2026), dc)
    opt = itofin.VanillaOption(
        itofin.OptionType.Call, 65.0, itofin.Date(15, 6, 2026) + 90, s
    )
    opt.set_engine(proc)
    return s, opt


def test_gate_row_1_npv_and_greeks_match_at_1e_10():
    _s, opt = _market_row_1()
    assert opt.npv() == pytest.approx(2.1333684449161985, abs=TOL)
    assert opt.delta() == pytest.approx(0.3724827979619727, abs=TOL)
    assert opt.gamma() == pytest.approx(0.042042755753785174, abs=TOL)
    assert opt.theta() == pytest.approx(-8.428174386737366, abs=TOL)
    assert opt.vega() == pytest.approx(11.351544053521998, abs=TOL)
    assert opt.rho() == pytest.approx(5.053899858200554, abs=TOL)
    assert opt.dividend_rho() == pytest.approx(-5.587241969429603, abs=TOL)


def test_gate_row_4_npv_and_greeks_match_at_1e_10():
    s = itofin.Settings()
    s.set_evaluation_date(itofin.Date(15, 6, 2026))
    dc = itofin.DayCounter.actual360()
    proc = itofin.BlackScholesProcess(100.0, 0.10, 0.10, 0.15, itofin.Date(15, 6, 2026), dc)
    opt = itofin.VanillaOption(
        itofin.OptionType.Call, 100.0, itofin.Date(15, 6, 2026) + 36, s
    )
    opt.set_engine(proc)
    assert opt.npv() == pytest.approx(1.8733445727649416, abs=TOL)
    assert opt.delta() == pytest.approx(0.5043916397384094, abs=TOL)
    assert opt.rho() == pytest.approx(4.856581940107595, abs=TOL)
    assert opt.dividend_rho() == pytest.approx(-5.043916397384089, abs=TOL)


def test_gate_row_5_put_arm_matches_at_1e_10():
    s = itofin.Settings()
    s.set_evaluation_date(itofin.Date(15, 6, 2026))
    dc = itofin.DayCounter.actual360()
    proc = itofin.BlackScholesProcess(100.0, 0.10, 0.10, 0.15, itofin.Date(15, 6, 2026), dc)
    opt = itofin.VanillaOption(
        itofin.OptionType.Put, 100.0, itofin.Date(15, 6, 2026) + 36, s
    )
    opt.set_engine(proc)
    assert opt.npv() == pytest.approx(1.8733445727649416, abs=TOL)
    assert opt.delta() == pytest.approx(-0.4856581940107596, abs=TOL)
    assert opt.rho() == pytest.approx(-5.043916397384088, abs=TOL)
    assert opt.dividend_rho() == pytest.approx(4.856581940107593, abs=TOL)


def test_unset_evaluation_date_raises():
    s = itofin.Settings()
    dc = itofin.DayCounter.actual360()
    proc = itofin.BlackScholesProcess(60.0, 0.08, 0.0, 0.30, itofin.Date(15, 6, 2026), dc)
    opt = itofin.VanillaOption(
        itofin.OptionType.Call, 65.0, itofin.Date(15, 6, 2026) + 90, s
    )
    opt.set_engine(proc)
    with pytest.raises(itofin.ItofinError, match="no evaluation date set"):
        opt.npv()
