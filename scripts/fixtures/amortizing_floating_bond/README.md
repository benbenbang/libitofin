# Independent plain-Ibor amortizing bond observations

The two `amortizing_floating_oracle.jsonl` fixture copies contain the same
53 unchanged floating lines selected from a qualified independent 95-row dataset:
52 complete financial states and one native retained-flow observation.
`manifest.json` records all byte hashes. They are not candidate-generated values.

## Provenance and qualifications

- Compiled oracle: separately identified QuantLib 1.43 Python wheel. It is **not**
  claimed built from the source checkout used for source review.
- Primary-source reference: QuantLib revision
  `9863b578af0caa4cecabf697196533e84a8308b6`, particularly
  `ql/instruments/bonds/amortizingfloatingratebond.cpp`, `ql/instruments/bond.cpp`
  and `ql/cashflows/iborcoupon.cpp`. QuantLib retains its own license; see the
  repository's `QUANTLIB_LICENSE.txt` and third-party notices.
- Actual native V3 producer SHA256:
  `2a9c72aa86d9c219ddc20cf8d0b5a805857b885c7d2f0ec8e99be502373abf75`.
  Its 95 native records had SHA256
  `5a76467cfa2e3774f90efadb0a7f8964c8a4ef7053660d59d3134ecd9dd43297`.
- V1/V2 constructor-overload setup failures and V3 exit 1 remain preserved in
  permanent independent evidence. V3's 31 independent-helper discrepancies used
  future preloaded histories before fixing time. The native amounts were correct.
- The narrow V4 helper correction uses history only when fixing date is no later
  than evaluation date. It did **not** rerun native queries or alter native values,
  tolerances or inputs. Pure re-evaluation passed 2,929 checks over all 95 rows.
- Qualified full corrected dataset SHA256:
  `b128faae6f31cb821e851545b64637aaad3d9706c7267602006eb0e2c2308572`.
  Original and corrected native-only projections are identical. Full producer,
  inputs, corrected helper, interpreter/package/loaded-binary identities and
  before/after source barriers are retained in the wave7 independent handoff.

`prepare.py` uses only the standard library. It verifies the qualified full
dataset hash, copies floating JSONL lines without reserialization, and selects
native values into `values.tsv`. It performs no native import or numerical oracle
recomputation. Error observations retain explicit failure flags, not successful
zero results. The TSV is consumed through the public C ABI in both C and C++.

`native_reference.py` is a separately labelled optional reproduction recipe. It
imports only QuantLib after explicit `--run`, reads construction terms from a
fixture copy and writes a fresh native-only projection with package/binary hashes.
It supplies the required 3M schedule metadata and the handle-only discount-engine
constructor. It is not the actual historical producer and has not been executed
as part of the V3/V4 evidence. Do not overwrite the checked-in observations with
fresh results without reviewing version, source/binary identity and differences.

## C TSV schema

After the header, each line contains variable-length tab-separated numbers:

1. Row index, calendar (0 Null, 1 WeekendsOnly), settlement days, evaluation,
   discount reference, forward reference, discount/forward quotes, at-par flag,
   ex-coupon days, issue date, five schedule dates, four balances, one spread.
2. Fixing count, then fixing date/value pairs.
3. Contractual maturity, settlement date, expired flag.
4. Success flag/value pairs for NPV, settlement value, clean price, dirty price,
   accrued and current notional, in that order.
5. Flow count, then payment date/principal flag/success flag/amount tuples.
6. Redemption count, then date/success flag/amount triples.
7. Notional probe count, then date/success flag/value triples.
8. Success flag/value pairs for clean yield, dirty yield, clean yield inversion,
   dirty yield inversion. Flag -1 means that native query was not present.

Dates are serial days from 1899-12-30. A failed query's numeric placeholder is not
an observation: the success flag requires a native error and unchanged C output.
Finite comparisons use `3e-11 + 3e-13 * abs(expected)`; counts and dates are exact.
Near-equal principal count/order and amount probes discriminate native `close`
coalescing rather than changing it. Historical/future fixing transitions, quote
states, ex-coupon windows, current-notional units, negative spreads, Sunday
contractual maturity and genuine partial-history errors are retained.

## Zero-notional analytics API-layer qualification

The native producer calls `Bond::bondYield` and `Bond::dirtyPrice`. Primary
`ql/instruments/bond.cpp:239-252` short-circuits yield to zero at zero current
notional; dirty-price inversion has the same shortcut. Native clean-price
inversion instead calls BondFunctions and returns a nontradable error. The
existing binding analytics are deliberately BondFunctions routes, documented in
`docs/docs/bond-analytics.md`, and require tradability for all those operations.

The 12 zero-notional financial rows preserve all original native values. Tests
assert the precise inherited typed nontradable error and diagnostic for both
yield kinds and both inversions, plus unchanged C output sentinels. They do not
silently skip these states or accept arbitrary failures. Existing scalar bullet
zero-notional regressions prove this behavior was not introduced by the new
factory. Positive-notional native comparisons and budgets remain unchanged.
The first V2 focused controller's 12 Python failures and C/C++ first-row aborts
are permanently preserved; this is a test/reference qualification, not a repair
to the existing analytics or a mutation of the independent oracle data.
