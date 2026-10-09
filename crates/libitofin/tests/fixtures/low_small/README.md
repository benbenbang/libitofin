# Small LOW independent references

- Source pin: QuantLib `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled reference: PyPI `QuantLib==1.43`, not claimed to share that revision.
- `generate.py` uses exact `Fraction` elimination for nine matrix cases and
  compares inverses with compiled QuantLib, including pivots and extreme scales.
- The wheel lacks the three legacy Lm/Lfm classes. Their four observations use
  independent 80-digit `Decimal.exp` evaluations of the inspected source laws.
  Compiled QuantLib spectral factor products supply a separate numerical check.
- Rust regression tests bake the independently generated values in
  `low_small_matrix.rs` and `low_small_legacy.rs`. They do not require Python or
  QuantLib at runtime. Factor orientation is not an entrywise contract.

Run in a Python environment with `QuantLib==1.43` installed:

```sh
PYTHONDONTWRITEBYTECODE=1 python generate.py \
  --source-root /path/to/pinned/QuantLib --output /tmp/low-small-evidence.json
```

The generated JSON is an optional audit artifact, not a test input. It includes
reference source hashes, exact rational results, compiled results and limits.
Inspect its source revision before attributing evidence to the pin above.
