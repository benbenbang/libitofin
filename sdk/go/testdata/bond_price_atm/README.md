# Independent price-fed bond ATM fixtures

- `bond_atm_from_price_reference.csv`: 242 finite rows, tracked LF SHA256
  `4c0dc66d7202e736dfedb04788b831765231f864f77e00e47f74c321843e4a97`.
- `bond_atm_from_price_errors.csv`: 52 required domain errors, tracked LF
  SHA256 `e0223bd55db5e5c234dc271a6a1db7456f1aa56745d5d14c8f107c49b1adccea`.
- Python fixture copies retain the identical tracked LF bytes. The approved
  conversion changes only CRLF to LF, with every CSV cell string unchanged.
- Acceptance was declared before candidate observation: ATM absolute 2e-13,
  accrued per 100 absolute 2e-12, current notional exact. NaN/Inf never pass.

The original generator queried a separately compiled QuantLib 1.43 wheel, not a
build claimed to match local source revision 9863b578. Its 242 results comprise
fixed 180, amortizing 60 and zero-BPS matching-target 2. Errors comprise fixed 36,
amortizing 12 and zero-BPS nonmatching-target 4. Original executed source, native
loader/byte pins, command exits and unformatted provenance remain in permanent
wave7 evidence. Raw CRLF SHA256 values are `16f7b522af2a8f1c8e827b3dedd61cb29f3903b9e56641899f07cc65a71af7a5`
and `060438b59d5bb4dbb0717da31fe83b576c4a5f799e5d184f005d8742009b4a3e`;
these remain the raw observation hashes in `provenance.json`. The tracked CSVs
are explicitly qualified LF-only successors. `provenance.json` is a formatted
key-order/whitespace-only successor, with identical typed data, not an original-byte-hash claim.

`cases.jsonl` is genuinely parsed JSON Lines: one metadata record followed by 49
ordered case records. Values/types/numbers match the original case JSON exactly;
its predeclaration status describes input creation, not a later execution result.
`independent_bond_atm_laws.py` retains the exact original 100-digit Decimal laws.

The portable adaptation has no candidate dependency or private interpreter/path
assumption. In a separate QuantLib 1.43 environment run:

```sh
python sdk/go/testdata/bond_price_atm/generate.py --output /a/new/output-directory
```

It uses the same queries/laws, captures its actual native module SHA and writes
strict results/error tables. An existing output is rejected. It is not labeled
as the original executed absolute-path producer. Default input is the adjacent
JSONL; `--cases` can explicitly select another declared input.

Isolated price-helper C/Go/Python tests construct only existing fixed/zero
facades: 182 finite rows and 40 domain errors. Generic direct-core/native-handle
tests cover all 242/52, including current-notional amortization/payment/ex-coupon
boundaries. Public amortizing factory combinations require integrated factory
coverage, not inference from the helper's isolated branch.
