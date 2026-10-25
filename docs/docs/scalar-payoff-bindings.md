# Standalone scalar payoffs

Python and Go expose immutable payoff terms and evaluation without constructing
an option, exercise, process or engine. These are additive helpers over the
existing Rust payoffs. They do not expand engine or exercise compatibility.

| Kind | Python factory on `ScalarPayoff` | Go factory on `Session` |
| --- | --- | --- |
| 0: plain vanilla | `plain_vanilla` | `NewPlainVanillaPayoff` |
| 1: cash-or-nothing | `cash_or_nothing` | `NewCashOrNothingPayoff` |
| 2: asset-or-nothing | `asset_or_nothing` | `NewAssetOrNothingPayoff` |
| 3: gap | `gap` | `NewGapPayoff` |
| 4: percentage-strike | `percentage_strike` | `NewPercentageStrikePayoff` |

All factories take an `OptionType` and a strike. Cash-or-nothing additionally
takes a fixed cash amount. Gap additionally takes a payment strike. For
percentage-strike, the stored strike is a **dimensionless ratio**: `0.8` means
80%, not 0.8%, and `80` is not equivalent.

## Evaluation rules

- Vanilla pays the positive part of `price - strike` for calls, or
  `strike - price` for puts.
- Cash and asset digitals use **strict** triggers. At equality they pay zero
  for both calls and puts. Cash amounts may be negative or zero.
- Gap uses an **inclusive** trigger. Calls pay `price - payment_strike` when
  `price >= trigger`; puts pay `payment_strike - price` when `price <= trigger`.
  Negative payouts are not floored. A call with trigger 100 and payment strike
  110 therefore pays -10 at price 100.
- Percentage-strike calls pay `price * max(1 - ratio, 0)`; puts pay
  `price * max(ratio - 1, 0)`. The factor, not the final payout, is floored.
  Negative prices can therefore produce negative payouts.

Terms and evaluated prices must be finite. Signed prices, strikes, ratios and
amounts are allowed. A computed nonfinite result, including overflow from finite
terms, raises an error. These scalar rules must not be confused with stricter
strike/spot requirements in pricing engines and option factories.

## Accessors and ownership

Python imports `ScalarPayoff` and `PayoffKind` from `itofin.instruments`.
`value(price)` evaluates it. `kind()`, `option_type()` and `strike()` return
copied terms. `cash_payoff()` and `second_strike()` return the applicable stored
amount, or `None` for other kinds. There are no mutators and input terms are
copied. The object supports immutable reads from Python threads.

Go uses `Value(price)` and `Parameters()`. The latter returns a copied
`ScalarPayoffParameters` containing `Kind`, `OptionType`, `Strike` and `Extra`.
`Extra` is the cash amount for cash-or-nothing, the payment strike for gap, and
zero otherwise. Editing that returned struct does not mutate the payoff. Objects
belong to their Session, calls are serialized on its native owner thread, and
payoffs and Sessions should be explicitly closed. Released handles fail.

The C ABI uses `itofin_scalar_payoff_new`, `itofin_scalar_payoff_value` and
`itofin_scalar_payoff_info`. The constructor accepts kind selectors 0-4 and
option selectors call=0, put=1. `extra` must be zero for kinds 0, 2 and 4.
`ItofinScalarPayoffInfo` stores those selectors, strike and extra. Failed calls
leave output values unchanged; the general C pointer/context contract still
applies. Unknown kinds and option flags fail without poisoning the context.

`NullPayoff` and the one-input floating-strike payoff are deliberately excluded:
their core scalar evaluation is an intentional failure. There is no generic
payoff-to-engine constructor or promise that all five payoffs work with all
existing engines.

## Native validation and examples

The 384 rows in `sdk/go/testdata/scalar_payoffs.tsv` come from an independent
QuantLib 1.43 compiled package. They cover both option types, adjacent representable
prices around strike, negative/zero inputs and signed gap/percentage payouts.
The recorded pinned source revision is a separate reference, not a claim that it
built that package. Provenance is in `scalar_payoffs_provenance.json` beside the
fixture; `scripts/generate_scalar_payoff_oracle.py` reproduces its numeric rows.
Tests compare these deterministic scalar evaluations exactly. This bounded
agreement does not establish full QuantLib or engine parity.

Examples:

- [`example/python/scalar_payoffs.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/scalar_payoffs.py)
- [`sdk/go/examples/scalar_payoffs/main.go`](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/scalar_payoffs/main.go)
