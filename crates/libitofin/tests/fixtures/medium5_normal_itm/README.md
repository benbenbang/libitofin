# Normal-model ITM references

- `generate.py` calls native Python QuantLib **1.43** directly for 30 rows.
- Local source oracle: `QuantLib/ql/pricingengines/blackformula.cpp:950`,
  pinned source commit `9863b578af0caa4cecabf697196533e84a8308b6`.
- The native wheel and local source checkout are distinct provenance sources.
- Two extreme rows use an independent **90-digit Decimal** normal-density
  integral series, not QuantLib or the Rust implementation. Exact binary64
  `MAX / MAX - (-MAX / MAX)` gives a standardized distance of two.
- Native QuantLib overflows the unscaled difference in those two cases and
  instead returns one/zero. The Rust function deliberately retains the finite
  mathematical ratio. This is not a claim of native bitwise parity there.
- Inputs are emitted using round-trippable Python float representations.
- TSV is UTF-8 with LF endings and regenerated deterministically.

Regenerate with QuantLib 1.43 installed:

```sh
python crates/libitofin/tests/fixtures/medium5_normal_itm/generate.py
```
