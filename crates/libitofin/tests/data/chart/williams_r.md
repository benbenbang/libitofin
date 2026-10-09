# Williams percent R independent fixture

Issue #1180. Nine ordered HLC bars, inclusive trailing period 3, first-valid 2.
Generated independently with Python standard-library `fractions.Fraction`, not
with the Rust core or a binding. Exact valid results are `-600/7`, `-200/7`,
`-100/3`, `-100/3`, `0`, `-250/3`, `-100`. Warmup placeholders are zero.

For each valid row, scan the last three supplied bars for highest high H and
lowest low L, then evaluate `-100*(H-close)/(H-L)` as an exact rational number.
The fixture covers extrema expiring, a new high, the high/low boundaries and
nonintegral output. The flat neutral value -50 is a separate explicit contract.
