# Asset digital FD fixture provenance

- The tracked Python and Go `asset_fd_reference.csv` files are LF-only representations
  of the independent QuantLib 1.43 wheel export. The original CRLF SHA-256 is
  `a01b0b1e7e88d4327ad7fc5299cd139c3d8d629d487cb10de960b4499b514d1a`;
  the tracked LF SHA-256 is
  `735f397e072e9dad27b3f824cf27ab0c8aa623da59b7e32fcd181c1a7784e44a`.
  Every CSV cell string, including 100-digit reference fields, is unchanged.
- Inputs use 2026-06-15, Actual/360, Douglas, equal time/space grids and no damping.
  Values are binary64 prices, delta, gamma and annual theta; the last column is
  an independent truncated-normal asset moment.
- Of 64 rows, 40 are ordinary positive-volatility nonexpired comparisons and 24
  are separate finite edge diagnostics. Same-grid agreement is not continuum
  precision or a promise that every individual grid ladder is monotonic.
- The generator and independent laws are preserved byte-exact from the actual
  successful independent run. The generator embeds original absolute local
  evidence/oracle paths and is not a portable fixture regeneration command.
- `provenance.json` records the genuine run, input/output hashes, separate source
  revision and compiled wheel identity. The source checkout is not claimed to
  be the source used to compile the separately installed wheel.
- The generator covers asset and gap cases; this feature tracks only the asset
  numeric CSV. Full original observations, zero-vol errors and immutable source
  evidence are retained in the author's local evidence, not silently converted
  into ordinary successful price/Greek fixtures.
