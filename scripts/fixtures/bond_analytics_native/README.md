# Python bond analytics native oracle

- Producer: `generate.py` here, independent calls to the separately installed
  compiled QuantLib Python wheel, not the Rust implementation. It reads each
  case's construction inputs from
  `crates/itofin-py/tests/fixtures/bond_analytics_native.json` and rewrites only
  the expectations, the `oracle` label and `source_generator_sha256`.
- Wheels: QuantLib 1.43 produced the file before #1405; with `--any-version`
  under the 1.43 wheel this generator reproduces that file byte for byte except
  the generator hash. The current expectations come from the QuantLib 1.44 wheel
  (CPython 3.13.15, macOS arm64, `_QuantLib.abi3.so` SHA-256
  `59239903c2bd3f831040ebdad4f8a6870c82ec482e370e18c2f48f7097c0dd3e`).
- QuantLib 1.44 no longer computes curve Z-spreads under `SimpleThenCompounded`
  or `CompoundedThenSimple`; those expectations are written as null and the
  Python test counts them.

Regenerate from the repository root with an interpreter containing QuantLib 1.44:

```sh
python scripts/fixtures/bond_analytics_native/generate.py
```
