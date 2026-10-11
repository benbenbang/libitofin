# OIS side oracle

- Generator: `generate.py`, native Python QuantLib **1.43**.
- Run: `python crates/libitofin/tests/fixtures/medium5_ois/generate.py`.
- Input/output has no dependency on libitofin.
- Reference date: 2 January 2026; ESTR forecasting on a log-linear discount
  curve distinct from the log-linear discounting curve.
- Node offsets: 0, 180, 500, 1000, 2000 days. Forecast factors:
  1, 0.986, 0.955, 0.905, 0.82; discount factors: 1, 0.992, 0.978, 0.95, 0.90.
- Notional 1,230,000; overnight spread -0.0012; fixed Actual/365 Fixed;
  semiannual payments, 2 business-day lag and Modified Following adjustment.
- Explicit starts: 7 January 2026 or 27 February 2026 with EOM enabled;
  termination: 19 July 2028, introducing a stub. All fixings forecast.
- Six rows cover payer/receiver with fixed rate 0.034, fair-rate fill, and EOM.
- TSV includes signed PV/BPS, fair rate/spread, stored fixed rate, accrual start
  and end serials, and fixed payment count. LF output, 17 significant digits.
- Rust tests use absolute tolerances 2e-9 monetary and 2e-14 rate units.

The local QuantLib source pin is `9863b578af0caa4cecabf697196533e84a8308b6`.
`MakeOIS::withType` assigns `type_`; both the temporary fair-rate swap and
final instrument use that type. The native 1.43 binary is a separate oracle,
not claimed to be compiled from that source pin.
