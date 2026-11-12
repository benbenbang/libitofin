# Normal-model ITM references

- `generate.py` calls native Python QuantLib **1.43** directly for 30 rows;
  three of them are re-pinned to v1.44 (see below).
- Source oracle: `ql/pricingengines/blackformula.cpp:962` at QuantLib v1.44.
- The native wheel and local source checkout are distinct provenance sources.
- Two extreme rows use an independent **90-digit Decimal** normal-density
  integral series, not QuantLib or the Rust implementation. Exact binary64
  `MAX / MAX - (-MAX / MAX)` gives a standardized distance of two.
- Native QuantLib overflows the unscaled difference in those two cases and
  instead returns one/zero. The Rust function deliberately retains the finite
  mathematical ratio. This is not a claim of native bitwise parity there.
- Rows sourced `v1.44 rule` (`zero_itm` Call, `zero_otm` Put) are hand-pinned
  to QuantLib v1.44's zero-volatility indicator (`blackformula.cpp:970`); the
  1.43 generator emits the old intrinsic value `5.0` there. Native QuantLib 1.44
  returns `1.0` for both.
- The `far_tail` Put row is sourced `QL1.44`: native QuantLib 1.44, whose erf
  saturates to exactly one for `|x| >= 6` (`errorfunction.cpp:126`), returns
  `1.0` where 1.43 returned `0.9999999999999999`.
- Inputs are emitted using round-trippable Python float representations.
- TSV is UTF-8 with LF endings and regenerated deterministically.

Regenerate with QuantLib 1.43 installed:

```sh
python crates/libitofin/tests/fixtures/medium5_normal_itm/generate.py
```
