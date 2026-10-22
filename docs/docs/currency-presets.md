# Currency presets

The existing four presets (EUR, USD, GBP, JPY) remain unchanged. Twenty additive
presets expose the stored Currency fields through Rust, C, Go and Python.

- Rust: `Currency::twd()`, `Currency::chf()`, and the other lowercase codes.
- Python: `itofin.indexes.Currency.twd()`, `.chf()`, and the other lowercase codes.
- TRY uses `Currency::try_()` / `Currency.try_()` because `try` is reserved.
- Go: `session.TWD()`, `session.CHF()`, and the other uppercase codes. Close
  currencies and sessions explicitly; currency handles remain session-owned.
- C: `itofin_currency_new` keeps selectors 0 EUR, 1 USD, 2 GBP, 3 JPY.
  The table below lists additive selectors 4 through 23. Release each handle.

All six fields are core value data. Equality remains name-based, so `Taiwan dollar`
is not interchangeable with the colloquial `New Taiwan Dollar` name. No existing
constructor, inspector, equality, rounding or triangulation behavior is changed.
The Currency constructor and stored-field accessor bindings are a separate additive
slice; these presets simply return those same value objects.

## Exact source provenance

Values were extracted from the local QuantLib source at revision
`9863b578af0caa4cecabf697196533e84a8308b6`, whose `ql/version.hpp` declares
`1.43-dev`. Each of the five source files matched its committed Git blob
byte-for-byte before extraction. An unrelated dirty workflow file was not modified.
This is pinned source data, not a claim of a compiled QuantLib execution or the
latest ISO display/cash convention. No external wheel supplied these fields.

The four identical `currency_presets.tsv` test fixtures retain all six fields,
class names, relative source paths and constructor line numbers. Their SHA256 is
`74cf061619d4600a580e2e782fd13f50b7154a289b0a336d404cb044acc007b5`.
The fixtures live in the core tests, C ABI tests, Go testdata and Python fixtures.

| Source at pinned revision | SHA256 |
| --- | --- |
| `ql/currencies/africa.cpp` | `8b3f99e718636755b68b113b111b4e42153079c9cfa001d6614ccfdc88727a94` |
| `ql/currencies/america.cpp` | `7d8f14ae6ed12fce70640557bb33c87fc96e18551b9878b875d83eff141f22db` |
| `ql/currencies/asia.cpp` | `0c24d9e6c5f651d6bcdc9ed4942edbb869a6bf778be70c6282a0b9f801ca28f5` |
| `ql/currencies/europe.cpp` | `d9e68e91b58c0207a3e7cac96222078e2d00c95186169e264428190bbc1db7e5` |
| `ql/currencies/oceania.cpp` | `12f21215b9713f879f31d7cacf9ab2073d00754a6f6ae9a888f762ef8853ec6f` |

## All six stored fields

Empty fraction symbols below are literal empty strings, not missing data.

| C selector | Name | Code | Numeric code | Symbol | Fraction symbol | Fractions per unit | Constructor source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 4 | Taiwan dollar | TWD | 901 | `NT$` | `""` | 100 | `ql/currencies/asia.cpp:196` |
| 5 | Swiss franc | CHF | 756 | `SwF` | `""` | 100 | `ql/currencies/europe.cpp:52` |
| 6 | Australian dollar | AUD | 36 | `A$` | `""` | 100 | `ql/currencies/oceania.cpp:33` |
| 7 | Canadian dollar | CAD | 124 | `Can$` | `""` | 100 | `ql/currencies/america.cpp:52` |
| 8 | Swedish krona | SEK | 752 | `kr` | `""` | 100 | `ql/currencies/europe.cpp:205` |
| 9 | Norwegian krone | NOK | 578 | `NKr` | `""` | 100 | `ql/currencies/europe.cpp:160` |
| 10 | Danish krone | DKK | 208 | `Dkr` | `""` | 100 | `ql/currencies/europe.cpp:79` |
| 11 | New Zealand dollar | NZD | 554 | `NZ$` | `""` | 100 | `ql/currencies/oceania.cpp:42` |
| 12 | Hong Kong dollar | HKD | 344 | `HK$` | `""` | 100 | `ql/currencies/asia.cpp:52` |
| 13 | Singapore dollar | SGD | 702 | `S$` | `""` | 100 | `ql/currencies/asia.cpp:178` |
| 14 | Chinese yuan | CNY | 156 | `Y` | `""` | 100 | `ql/currencies/asia.cpp:43` |
| 15 | South-Korean won | KRW | 410 | `W` | `""` | 100 | `ql/currencies/asia.cpp:115` |
| 16 | Indian rupee | INR | 356 | `Rs` | `""` | 100 | `ql/currencies/asia.cpp:79` |
| 17 | Brazilian real | BRL | 986 | `R$` | `""` | 100 | `ql/currencies/america.cpp:43` |
| 18 | Mexican peso | MXN | 484 | `Mex$` | `""` | 100 | `ql/currencies/america.cpp:79` |
| 19 | South-African rand | ZAR | 710 | `R` | `""` | 100 | `ql/currencies/africa.cpp:99` |
| 20 | Polish zloty | PLN | 985 | `zl` | `""` | 100 | `ql/currencies/europe.cpp:169` |
| 21 | Czech koruna | CZK | 203 | `Kc` | `""` | 100 | `ql/currencies/europe.cpp:70` |
| 22 | Hungarian forint | HUF | 348 | `Ft` | `""` | 1 | `ql/currencies/europe.cpp:115` |
| 23 | New Turkish lira | TRY | 949 | `YTL` | `""` | 100 | `ql/currencies/europe.cpp:232` |

The deliberately source-exact details include HUF fractions per unit 1, TRY
`New Turkish lira` / `YTL`, CHF `SwF`, KRW `South-Korean won`, and ZAR
`South-African rand`. AUD's stored numeric integer is 36. The old JPY preset
continues to store 100 fractions per unit. None is modernized or normalized.

## Boundaries

- These are 20 selected presets, not the full QuantLib currency catalogue.
- No currency rounding, triangulation, minor-unit-code or exchange-rate-manager
  API is added.
- C/Go contexts and sessions, thread confinement, explicit release and errors
  retain the existing binding contract.
