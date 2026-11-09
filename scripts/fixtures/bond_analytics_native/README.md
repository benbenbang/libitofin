# Python bond analytics native oracle

- Producer: `generate.py` here, independent calls to the separately installed
  compiled QuantLib Python wheel, not the Rust implementation. It reads each
  case's construction inputs from
  `crates/itofin-py/tests/fixtures/bond_analytics_native.json` and rewrites only
  the expectations, the `oracle` label and `source_generator_sha256`.
- Wheels: QuantLib 1.43 produced the file before this generator was committed;
  with `--any-version` under the 1.43 wheel the generator reproduces that file
  byte for byte except the generator hash.
- From QuantLib 1.44 on, curve Z-spreads under `SimpleThenCompounded` or
  `CompoundedThenSimple` are not computed; those expectations are written as
  null.

Regenerate from the repository root with an interpreter containing QuantLib 1.44:

```sh
python scripts/fixtures/bond_analytics_native/generate.py
```
