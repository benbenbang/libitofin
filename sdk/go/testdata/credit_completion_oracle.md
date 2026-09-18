# Credit completion oracle

`credit_completion_oracle.py` generates the adjacent JSON using the independent
QuantLib 1.43 Python wheel:

```sh
uv run --isolated --no-project --with QuantLib==1.43 python \
  sdk/go/testdata/credit_completion_oracle.py \
  > /tmp/credit_completion_oracle.json
diff -u sdk/go/testdata/credit_completion_oracle.json \
  /tmp/credit_completion_oracle.json
```

The trade is 2026-06-15, maturity 2029-06-20, notional 10 million, running spread
1%, recovery 40%, flat discount 3%, and flat hazard 2%. The CDS-rule schedule uses
WeekendsOnly, Following payments and an Unadjusted termination. The builder's
final coupon includes the last day; the explicit Go constructor uses ordinary
Actual360. The generator separately verifies the builder against a fully explicit
QuantLib contract with matching final-coupon and settlement conventions.

The Go tests check both contracts against their own oracle and independently
reconstruct every premium/default payment using exponential survival/discounts
and midpoint default dates. Their NPV difference equals one extra accrual day
in the final regular and accrued-on-default coupon. The Go binding has no CDS
schedule/coupon getter; this validates observable leg values without claiming
such getters exist. Pricing tolerance stays 1e-8; fair quotes and survival use
1e-12, and recovered hazard uses 1e-9.

The nonflat matrix has six hazard/discount nodes, including nodes inside coupon
periods. It checks all four NoFix/Taylor and Flat/Piecewise combinations. The two
forward conventions differ by about 9.30 in NPV; NoFix/Taylor agree for this
well-conditioned fixture. That equality does not claim cancellation-stress
coverage for the numerical fix. Source: QuantLib 1.43 `MakeCreditDefaultSwap`,
`CreditDefaultSwap`, `MidPointCdsEngine`, and `IsdaCdsEngine`.
