# Global bootstrap oracle

`global_bootstrap_oracle.csv` is the unchanged independent C++ QuantLib fixture
from Python #981. Reproduce it with the pinned source and instructions in
[`crates/itofin-py/tests/fixtures/global_bootstrap`](../../../crates/itofin-py/tests/fixtures/global_bootstrap/README.md).
The Go test fits curve nodes and an external convexity quote, then changes a
market quote and checks recalibration, retaining the original 1e-9 tolerance.
Callback snapshots, errors, ownership and reentry are tested separately.
