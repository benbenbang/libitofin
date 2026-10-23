# Cross-currency binding fixtures

`xccy_basis_native.csv` is an exact byte copy of the existing core fixture
`crates/libitofin/tests/fixtures/medium3_xccy_basis/native.csv`. The generator and
source/license provenance remain at that original tracked location. It uses
the actual independent compiled QuantLib1.43 whole-class instrument and engine,
not libitofin output. Source audit9863b578 is separate from package revision.

Absolute tolerances remain2e-9 for values and2e-13 for fair spread; expired fair
spread is an error and its placeholder fixture0 is never asserted as a value.
The Go testdata copy is byte-identical too. No tolerance change or new oracle
claim is made by copying this existing dataset for standalone binding test runs.
