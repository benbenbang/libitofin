"""Standalone currency storage and retained index dependencies."""

import gc

import pytest

from itofin import Settings
from itofin.indexes import Currency, IborIndex
from itofin.time import BusinessDayConvention as BDC, Calendar, DayCounter, Period


def test_custom_currency_stored_fields_and_retained_index():
    """Each stored field survives UTF-8/NUL transport and index retention."""
    fields = ("臺灣\x00 dollar", "TWD", 901, "NT$\x00元", "分\x00", 100)
    currency = Currency(*fields)
    assert (currency.name(), currency.code(), currency.numeric_code(), currency.symbol(), currency.fraction_symbol(), currency.fractions_per_unit()) == fields
    assert repr(currency) == "Currency(TWD)"
    settings = Settings()
    index = IborIndex("custom-currency", Period(3, "Months"), 0, currency, Calendar.null_calendar(), BDC.Unadjusted, False, DayCounter.actual360(), None, settings)
    del currency
    gc.collect()
    retained = index.currency()
    assert (retained.name(), retained.code(), retained.numeric_code(), retained.symbol(), retained.fraction_symbol(), retained.fractions_per_unit()) == fields


@pytest.mark.parametrize("code", ["", "TW", "TWDD", "台幣", "TW\x00", "T1D"])
def test_custom_currency_rejects_invalid_code(code):
    """Code validation protects the existing fixed-size C/Go code getter."""
    with pytest.raises(ValueError, match="three ASCII letters"):
        Currency("Name", code, 901, "NT$", "", 100)


@pytest.mark.parametrize("factory,fields", [
    (Currency.eur, ("European Euro", "EUR", 978, "", "", 100)),
    (Currency.usd, ("U.S. dollar", "USD", 840, "$", "¢", 100)),
    (Currency.gbp, ("British pound sterling", "GBP", 826, "£", "p", 100)),
    (Currency.jpy, ("Japanese yen", "JPY", 392, "¥", "", 100)),
])
def test_existing_currency_factories_remain_unchanged(factory, fields):
    """The additive constructor does not replace or change existing factories."""
    currency = factory()
    assert (currency.name(), currency.code(), currency.numeric_code(), currency.symbol(), currency.fraction_symbol(), currency.fractions_per_unit()) == fields


def test_custom_currency_preserves_empty_signed_and_lowercase_fields():
    """This boundary is a stored-field constructor, not an ISO registry lookup."""
    currency = Currency("", "abc", -1, "", "", -1)
    assert (currency.name(), currency.code(), currency.numeric_code(), currency.symbol(), currency.fraction_symbol(), currency.fractions_per_unit()) == ("", "abc", -1, "", "", -1)
    for value in [-2**31 - 1, 2**31]:
        with pytest.raises(OverflowError):
            Currency("Name", "TWD", value, "", "", 100)
        with pytest.raises(OverflowError):
            Currency("Name", "TWD", 901, "", "", value)
