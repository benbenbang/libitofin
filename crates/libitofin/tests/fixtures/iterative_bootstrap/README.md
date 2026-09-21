# Iterative bootstrap robustness oracle

`oracle.py` runs QuantLib 1.43 independently; `oracle.txt` records its output.
Run with a Python environment containing `QuantLib==1.43`.

- One-year deposits use 2026-06-15, zero settlement days, NullCalendar,
  Unadjusted and Actual365Fixed. Positive and negative narrow bounds fail after
  two attempts and succeed after three. Both sides of sign-aware widening matter.
- Explicit fallback scans include both endpoints and preserve the first minimum.
  A one-evaluation cap forces a scan whose best point is interior. QL writes only
  the chosen pillar after scanning; for zero yields node zero retains the final
  scan value. The tests preserve this behavior rather than flattening both nodes.
- A live deposit update from 1% to 40% lies outside the cached rate bounds but
  inside the fresh bounds. Default strict mode recovers without opt-in retries.
- The independent mixed deposit/future/FRA/swap strip under CubicZero (Kruger) at
  accuracy `1e-20` reaches the 99-iteration limit. Strict mode errors; explicit
  fallback returns the final pass. Rust compares finite nodes at `1e-12`, the
  existing bootstrap numerical tolerance. Fallback is approximate, not repricing
  success at the requested `1e-20` tolerance.

Rust integration tests also cover invalid options, invalid then restored quotes,
failed cached solves followed by valid quotes, and evaluation-date updates.
The shared driver remains generic across yield, credit, and inflation curves;
new configurable binding factories are yield-only.

Go keeps the same seven numeric cases in
`sdk/go/testdata/iterative_bootstrap_oracle.txt` so published module tests are
self-contained. Regenerate the canonical output with `oracle.py`, copy it to
that path, and compare both files before committing.
