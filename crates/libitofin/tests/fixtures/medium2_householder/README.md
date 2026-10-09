# Original Householder fixtures

`generate.py` compiles the actual original `householder.cpp` and `errors.cpp`
using the source checkout pinned to:

`9863b578af0caa4cecabf697196533e84a8308b6`

The classes are not exported by the separately installed QuantLib 1.43 Python
wheel. No wheel-backed Householder result is claimed.

Regenerate the `ORACLES` constant at the bottom of `medium2_householder.rs`:

```sh
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk \
python3 crates/libitofin/tests/fixtures/medium2_householder/generate.py \
  --quantlib /path/to/QuantLib --boost-include /path/to/boost/include
```

The generated constant is formatted by `cargo fmt`; numeric literals remain
unchanged. The eleven vectors cover ordinary, orthogonal, positive/negative
parallel, tiny-angle positive/negative and non-axis direction cases. Both
original reflection vectors and original applied outputs are recorded.

The checked Rust zero-vector explicit matrix is identity. This is an intentional
extension of the original nonfinite matrix case, not a native fixture value.
Large/tiny scaling and malformed-input assertions are separate Rust contract
tests. Existing decompositions and APIs are unchanged.
