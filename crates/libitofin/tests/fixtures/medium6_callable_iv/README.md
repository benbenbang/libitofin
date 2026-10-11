# Callable implied-volatility reference

`generate.py` requires QuantLib 1.43 and writes `reference.tsv` without invoking
Rust. The sixteen cases cover fixed/zero products, issuer calls/holder puts,
flat/nonflat curves, three faces, settlement lags 0/2/7/17 and three volatilities.

- `npv`, `dirty`, `clean`, `native_iv_*` are direct native Black-engine values.
- `corrected_*` use independent native cash-flow NPV, forward yield, modified
  duration, Black formula and Brent inversion.
- The corrected forward includes `discount(settlement)`, missing in the native
  engine's positive-lag calculation. At zero lag, the values coincide.
- Production inversion uses quoted-unit residuals. Native oracle Brent residuals
  retain currency units for the ordinary financial faces in this fixture.

The tests intentionally distinguish native defective positive-lag prices from
corrected prices instead of treating their difference as a tolerance issue.
