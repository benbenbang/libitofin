"""Independently derived covered-interest-parity and lifecycle checks."""
import gc
import math

import pytest
from itofin import ItofinError
from itofin.indexes import Currency
from itofin.instruments import FxForward, Position
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter


def fixture():
    """Use different curve references to expose settlement normalization."""
    dc = DayCounter.actual365_fixed()
    quote = SimpleQuote(.05)
    base = FlatForward(Date(1, 1, 2025), .03, dc)
    counter = FlatForward.from_quote(Date(2, 1, 2025), quote, dc)
    args = [Currency.eur(), Currency.usd(), 1.1, base, counter, 1_000_000.,
            1.12, Date(3, 1, 2025), Date(3, 1, 2026), Position.Long]
    return args, quote


@pytest.mark.parametrize("short", [False, True])
def test_fx_forward_analytical_and_fields(short):
    """Flat-curve algebra is independent of production pricing code."""
    args, _ = fixture()
    args[-1] = Position.Short if short else Position.Long
    forward = FxForward(*args)
    fair = 1.1 * math.exp(.02)
    expected = 1_000_000 * (fair - 1.12) * math.exp(-.05 * 366 / 365)
    assert forward.fair_forward_rate() == pytest.approx(fair, rel=2e-15)
    assert forward.npv(Date(1, 1, 2025)) == pytest.approx(-expected if short else expected, abs=1e-9)
    assert forward.npv(Date(3, 1, 2026)) == forward.npv(Date(1, 1, 2025))
    assert not forward.is_expired(Date(3, 1, 2026))
    assert forward.is_expired(Date(4, 1, 2026))
    assert forward.npv(Date(4, 1, 2026)) == 0.
    assert forward.fair_forward_rate() == pytest.approx(fair, rel=2e-15)
    assert forward.spot() == 1.1
    assert forward.strike() == 1.12
    assert forward.base_notional() == 1_000_000.
    assert forward.spot_settlement_date() == args[7]
    assert forward.delivery_date() == args[8]
    assert forward.position() == args[9]
    assert forward.base_currency().code() == "EUR"
    assert forward.quote_currency().code() == "USD"


def test_fx_forward_retained_live_curves():
    """Retain currencies/curves and observe quote changes without a spot observer."""
    args, quote = fixture()
    forward = FxForward(*args)
    before = forward.fair_forward_rate()
    del args
    gc.collect()
    quote.set_value(.08)
    assert forward.fair_forward_rate() == pytest.approx(1.1 * math.exp(.05), rel=2e-15)
    assert forward.fair_forward_rate() != before
    assert forward.spot() == 1.1
    assert forward.base_currency().name() == Currency.eur().name()
    with pytest.raises((AttributeError, TypeError)):
        setattr(forward, "spot", 2.)


@pytest.mark.parametrize("field,value", [(2, 0.), (2, math.nan), (2, math.inf),
                                         (5, -1.), (5, 1e308), (6, 0.),
                                         (6, math.inf), (8, Date(2, 1, 2025)),
                                         (1, Currency.eur())])
def test_fx_forward_invalid_construction(field, value):
    """Core domain errors remain exceptions, including quote-notional overflow."""
    args, _ = fixture()
    args[field] = value
    if field == 5 and value == 1e308:
        args[6] = 2.
    with pytest.raises(ItofinError):
        FxForward(*args)


def test_fx_forward_invalid_reference_and_expiry():
    """Expiry avoids invalid curve domains; fair rate deliberately does not."""
    args, _ = fixture()
    args[3] = FlatForward(Date(4, 1, 2025), .03, DayCounter.actual365_fixed())
    forward = FxForward(*args)
    with pytest.raises(ItofinError):
        forward.fair_forward_rate()
    with pytest.raises(ItofinError):
        forward.npv(Date(3, 1, 2026))
    assert forward.npv(Date(4, 1, 2026)) == 0.
    with pytest.raises(TypeError):
        getattr(forward, "is_expired")(None)
    with pytest.raises(TypeError):
        getattr(forward, "npv")(None)
