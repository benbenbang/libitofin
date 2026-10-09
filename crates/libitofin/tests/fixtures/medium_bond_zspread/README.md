# Bond Z-spread oracle

- Inspected QuantLib source: `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled reference: independently installed PyPI `QuantLib==1.43`.
- The wheel does not expose its source revision. Its build is not claimed to
  equal the inspected checkout revision.
- Source file hashes and package/runtime version are written into the output.

Run with a Python environment containing that pinned package:

```sh
python generate.py --source-root /path/to/QuantLib --output /tmp/bond-zspread.json
```

The generator invokes compiled `BondFunctions.dirtyPrice`, `cleanPrice` and
`zSpread` for 48 cases. It also checks maturity is nontradable. Cases use an
Actual360 flat continuous 3% curve referenced to 1 July 2026, 5% coupons,
1000 initial notional, 1000 or 400 subsequent notional, optional six-day
ex-coupon periods, optional irregular October 2027 payment, settlement on
3/7/10 July 2027, continuous +1.25% or semiannual compounded -0.5% spreads.

The Rust test embeds the generator's inputs and clean/dirty prices as numeric
rows. Its default spread inversion tolerance is `1e-10`; an explicit `1e-13`
solver accuracy preserves a separate `1e-10` price roundtrip tolerance.
Generated JSON is an optional audit artifact, not a runtime test dependency.
