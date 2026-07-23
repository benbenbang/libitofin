import itofin
import pytest


def test_simple_quote_holds_and_updates_value():
    q = itofin.SimpleQuote(60.0)
    assert q.value() == 60.0
    q.set_value(61.0)
    assert q.value() == 61.0


def test_black_scholes_process_builds_on_gate_row_1_market():
    ref_date = itofin.Date(15, 6, 2026)
    dc = itofin.DayCounter.actual360()
    process = itofin.BlackScholesProcess(60.0, 0.08, 0.0, 0.30, ref_date, dc)
    assert process.risk_free_rate() == pytest.approx(0.08)
    assert process.dividend_yield() == pytest.approx(0.0)


def test_arg_order_pins_risk_free_and_dividend_not_swapped():
    ref_date = itofin.Date(15, 6, 2026)
    dc = itofin.DayCounter.actual360()
    process = itofin.BlackScholesProcess(60.0, 0.08, 0.02, 0.30, ref_date, dc)
    assert process.risk_free_rate() == pytest.approx(0.08)
    assert process.dividend_yield() == pytest.approx(0.02)
