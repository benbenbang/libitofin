# Standalone inflation principal cash flows

Python and Go support explicit-base CPI principal and live-base zero-inflation
principal. Each can be inspected directly, converted to a generic cash flow,
or valued through the existing Leg NPV functions. These APIs are additive.
They are not coupons, bonds, calibrated models or new pricing engines.

## Two different denominators

| Flow | Payment amount |
| --- | --- |
| `CPICashFlow` | `nominal * (lagged_CPI(observation_date) / supplied_base_CPI - growth_only)` |
| `ZeroInflationCashFlow` | `nominal * (lagged_CPI(end_date) / lagged_CPI(start_date) - growth_only)` |

In these formulas `growth_only` is 1 when enabled and 0 otherwise. It selects
inflation growth versus the full indexed principal, not an initial exchange.
The payment date is supplied explicitly and is not calendar-adjusted.

- Explicit base CPI is fixed at construction. It must be finite and its absolute
  value must exceed `1e-16`, matching the existing explicit-base CPI core.
- The live-base flow reads its denominator from the index at each query. Neither
  its base nor its end CPI is captured at construction. Fixing additions and
  curve relinks remain live. Existing index APIs still reject conflicting stored
  fixings; this feature adds no force-overwrite or history-clear interface.
- Flat and Linear observation interpolation use the native lagged-fixing rules.
  Linear interpolation uses the unlagged observation date's position in its
  inflation period. It does not simply interpolate on the lagged calendar day.
- Nominal is finite and may be signed or zero. This is an individual cash-flow
  building block, not a positive-notional instrument factory.

Both constructors are lazy: valid structural terms do not require fixings or
an inflation forecast curve to exist yet. Value queries fail when required
history or a forecast curve is unavailable. A live zero base fixing, overflow or
another non-finite amount fails rather than returning infinity or NaN.

## Entry points

| Layer | Explicit base | Live base |
| --- | --- | --- |
| Python `itofin.cashflows` | `CPICashFlow(...)` | `ZeroInflationCashFlow(...)` |
| Go | `Session.NewCPICashFlow(CPICashFlowConfig)` | `Session.NewZeroInflationCashFlow(ZeroInflationCashFlowConfig)` |
| C | `itofin_cpi_cashflow_new` | `itofin_zero_inflation_cashflow_new` |

Both Python wrappers expose `amount`, `base_fixing`, `index_fixing`, `date`,
`as_cashflow` and `as_leg`. Both Go constructors return `*InflationCashFlow`,
with `Amount`, `BaseFixing`, `IndexFixing`, `Date`, `AsCashFlow` and `AsLeg`.
Go config `GrowthOnly=false` pays the full ratio; Python defaults it to true.

C scalar selectors are 0 amount, 1 base fixing and 2 index fixing. Conversion
selector 0 returns a generic cash flow, 1 a one-flow leg; other values fail.
Outputs are written only on success. The standard C pointer, context-thread and
handle-lifetime contract still applies.

## Ownership, settings and safe dates

- The typed flow, extracted generic flow and extracted leg each retain the native
  index and its dependencies. Releasing the original wrapper does not invalidate
  a conversion. Go handles remain session-bound and must be closed explicitly.
- `as_leg`/`AsLeg` produces exactly one retained flow, not an arbitrary composition
  API. Leg NPV and spread analytics require the **same Settings instance** as the
  index, not merely another instance with an equal evaluation date.
- Supported dates are 1901-2199 and observation lags are nonnegative Days, Weeks,
  Months or Years. Constructor checks reject observation subtraction overflow.
  Query-time checks also guard mutable evaluation-date publication lags and
  Linear interpolation's next-period dates. These guards stay attached to every
  generic conversion; invalid boundary dates fail without poisoning a C context.
- Existing coupon/leg constructors and their stored-base getter contracts are
  unchanged. Base-date CPI coupon/leg support is a separate extension.

## Validation and examples

`sdk/go/testdata/inflation-principal-native.tsv` records 48 cases and two query
states from an independently compiled QuantLib 1.43 package: fixed/live bases,
Flat/Linear, three monthly lags, growth/full principal and historical/forecast
observations, followed by a curve relink. Its metadata records the native binary
hash and source reference separately. It does not claim that wheel was built
from the pinned source or establish full QuantLib/inflation parity.

Regenerate with a QuantLib 1.43 Python environment:

```sh
python scripts/generate_inflation_principal_oracle.py sdk/go/testdata
```

Runnable examples:

- [Python](https://github.com/benbenbang/libitofin/blob/main/example/python/inflation_principal.py)
- [Go](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/inflation_principal/main.go)
