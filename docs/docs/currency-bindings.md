# Standalone currency specifications

Currencies can be constructed once and reused in index constructors in Python
and Go. The six core stored fields are available: name, code, numeric code,
symbol, fraction symbol and fractions per unit. The existing EUR, USD, GBP and
JPY factories are unchanged.

## Python

```python
from itofin.indexes import Currency

currency = Currency("Taiwan dollar", "TWD", 901, "NT$", "", 100)
assert currency.name() == "Taiwan dollar"
assert currency.code() == "TWD"
assert currency.numeric_code() == 901
assert currency.symbol() == "NT$"
assert currency.fraction_symbol() == ""
assert currency.fractions_per_unit() == 100
```

## Go

```go
currency, err := session.NewCurrency(itofin.CurrencyConfig{
    Name: "Taiwan dollar", Code: "TWD", NumericCode: 901,
    Symbol: "NT$", FractionsPerUnit: 100,
})
if err != nil {
    return err
}
defer currency.Close()
name, err := currency.Name()
```

The corresponding methods are `Name`, `Code`, `NumericCode`, `Symbol`,
`FractionSymbol` and `FractionsPerUnit`. Objects belong to one session;
index constructors reject mixed-session dependencies. Releasing the currency
wrapper does not discard a currency already retained by an index. Close objects
and sessions explicitly.

## Validation and identity

- Custom codes must have exactly three ASCII letters. Case is preserved, with
  no registry lookup or normalization. Python rejects invalid codes with
  `ValueError`; C/Go report an invalid-argument error.
- Other text fields preserve UTF-8 and embedded NULs. Numeric fields preserve
  signed 32-bit values, matching the core stored-field constructor.
- Core currency equality compares the **name**, not the code. A custom currency
  using `New Taiwan Dollar` does not equal one named `Taiwan dollar`, even if
  both codes are `TWD`. Language wrappers do not add value-equality operators.
- This is not money arithmetic, currency conversion or complete QuantLib
  currency parity. Rounding, triangulation and minor-unit metadata are absent.

## C ABI

`itofin_currency_custom_new` copies the counted UTF-8 strings from
`ItofinCurrencyConfig`. Existing `itofin_currency_new` selectors remain intact.
`itofin_currency_text` selects name/code/symbol/fraction symbol as fields 0/1/2/3.
Capacity zero queries the byte length. Output is counted UTF-8 without a trailing
NUL, and embedded NULs are data. `itofin_currency_integer` selects numeric code
or fractions per unit as fields 0/1. New accessor outputs remain unchanged on
failure. Existing `itofin_currency_code` still returns the three-letter code plus
NUL with capacity at least four and checks that the actual code fits.

All calls follow the [binding ownership contract](https://github.com/benbenbang/libitofin/blob/main/docs/go-binding-contract.md).
