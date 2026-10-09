# Exchange-rate chaining oracle

- Source reference: QuantLib commit `9863b578af0caa4cecabf697196533e84a8308b6`,
  `ql/exchangerate.cpp` and `ql/exchangerate.hpp`.
- Independent execution: the compiled PyPI **QuantLib 1.43** wheel.
  This wheel is not asserted to be built from the inspected source revision.
- The generator checks both versions and records SHA-256 hashes of inspected
  sources, four orientations and their reversed arguments, equal/opposite pairs,
  nested chains, intermediate rounding and aggregate overflow/underflow.
- Rust integration tests bake these observations into literals. They do not
  require Python, QuantLib or an untracked JSON file during normal tests.

```sh
python -m venv /tmp/fx-oracle
/tmp/fx-oracle/bin/pip install QuantLib==1.43
/tmp/fx-oracle/bin/python generate.py \
  --source-root /path/to/pinned/QuantLib \
  --output /tmp/fx-chain-observations.json
```

## Checked-contract differences

The parent Rust direct-rate API rejects invalid rates and non-finite amounts or
conversion results. Chaining preserves that policy: invalid inputs and stored
rates that overflow or underflow to zero are errors. Intermediate conversion
infinity is also an error, where QuantLib returns infinity. Intermediate finite
underflow to zero remains zero, including its sign. No tolerance is relaxed to
replace the original sequential conversion with a flattened numeric rate.
