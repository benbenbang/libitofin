# Go binding follow-ups

The original `feat/go-bindings-followups` stack started at `4c48647e`, with
reviewed commits integrated sequentially from isolated feature branches. GitHub
rebase-merges rewrite commit IDs; that starting point records assembly history.
Tracker: [#1000](https://github.com/benbenbang/libitofin/issues/1000); binding
strategy: [#46](https://github.com/benbenbang/libitofin/issues/46).

## Scope

- Distinct `itofin_ffi` native artifacts allow Python and C/Go builds to coexist.
- A scoped root pre-commit hook runs the native and Go validation script.
- Stateful pseudorandom and low-discrepancy generators extend the original
  stateless portfolio simulation API without changing its draw order.
- Country/market calendars, joint calendars, queries, and comparable keys
  extend the time facade. Existing core holiday-table bounds are exposed as
  constants for checked native input validation; numerical algorithms are unchanged.
- Independent calibration and CDS repricing oracles, inflation metadata, and
  retained-dependency tests address selected behavioral gaps. The completed
  bounded cases and remaining Rust limitations are in [the behavioral evidence](go-binding-test-gaps.md).
- [Native packaging](go-distribution.md) and [external consumer acceptance](go-consumer-validation.md)
  establish an installation path separate from the Rust checkout.

## Coverage interpretation

The starting tree mapped 744/744 baseline Python declarations, but only 744/855
current declarations. The 111 newer declarations comprised 56 random-number
and 55 calendar symbols. Baseline mode still enforces the original contract;
the full-current audit measures parity against the checked-out Python stubs.

API mappings identify declarations and referenced implementations. They do not
prove every configuration, branch, or numerical behavior. Statement coverage,
independent numerical oracles, and lifecycle tests provide separate evidence.
The [original review](go-bindings-review.md) retains the previous Linux counts.

## Integrated validation

Validated API revision: `b169fabe15d86623b24f3dd76489533d59a3a4da`. The [Linux/macOS CI run](https://github.com/benbenbang/libitofin/actions/runs/35302175566) passed Rust, Python, native/C/C++ checks, Go vet and
race/cgocheck2 tests, header generation, native-package consumer validation,
and the aggregate `workflow-success` gate.

On 2026-09-18, the same file tree passed all local repository hooks on macOS
arm64 with Go 1.27.1 and Rust 1.96.0. The separate full-current strict audit
mapped 855/855 declarations with 713 explicit test references; the baseline
remains 744/744. Go library statement coverage was 83.4%. These measures do
not imply exhaustive behavioral coverage.

## Python API additions after 0.21.0

The 855/855 result above is historical. #1030 now accounts for all 905 current
declarations: 881 implemented mappings and 24 explicitly classified Python-only
nonconstructible enum declarations. The original 744 implementation mappings
remain required. An independent Python 0.22.0 fixture checks integer conversions
for 24 enum types; Go retains typed integer constants and casts.

GlobalBootstrap supports retained external quote variables, fallible penalties,
callable dates, and futures convexity-observation control. Joint fitting and
quote-driven recalibration use the independent #981 QuantLib C++ oracle at the
unchanged 1e-9 tolerance. Callback snapshots, failures, panics, reentry, concurrent
Close and retained-consumer ownership have direct tests. The [binding contract](go-binding-contract.md)
defines these semantics. Full-current and historical audits are both enforced.

#1036 completes the bounded behavioral follow-ups with independent credit, rates,
inflation and calibration fixtures. Rust #1038 subsequently implements Simple
OIS averaging, exercised through the Python and C/Go bindings. See the
[case-by-case evidence](go-binding-test-gaps.md). Local and platform validation
for these additions is recorded in their implementation PRs; the older run above
only establishes the historical tree.

## Historical combined local validation

The integrated tree at `34a5a8bc` passed on macOS arm64 with Go 1.27.1 and
Rust 1.96.0:

- Full-current strict audit: 855/855 declarations mapped, 713 explicit test
  references; baseline audit: 744/744.
- All repository pre-commit checks, including full Cargo tests and Python stub
  synchronization; generated C header matches cbindgen 0.29.2 output.
- 36 native tests, C and C++ smoke clients, Go vet, race detection with
  `cgocheck2`, and the portfolio example. Go package statement coverage: 83.4%.
- Native archive checksums and standalone external-consumer acceptance passed
  with the extracted library under a path containing spaces and no loader
  environment variables.
- Workflow linting and shellcheck passed. This local run did not establish
  Linux validation; platform CI evidence is recorded separately above.

## Delivery limits

The coordinated release workflow validates Linux amd64 and macOS arm64 packages,
attaches them to the main `vVERSION` release, and publishes the matching
`sdk/go/vVERSION` tag. SDK-path publication and platform acceptance are tracked
in [#1037](https://github.com/benbenbang/libitofin/issues/1037). Fresh external
consumers verify the published module and assets without local replacements. Legacy `bindings/go/v0.22.0` remains unchanged; its acceptance
evidence is recorded in
[#1025](https://github.com/benbenbang/libitofin/issues/1025); private application
migration and production budgets remain separate acceptance work.
The Go/C ABI matrix checks both clean and incremental Python/stub/C ABI build
orders on Linux and macOS with `scripts/check_go_mixed_builds.sh`. It uses fresh
Cargo target directories, then verifies Python imports, stubs, and C/C++ callers
after each order and a workspace build. Run it in a Python virtual environment
with maturin, numpy, and pytest installed.
The package matrix checks missing and incompatible native-library failures
with `scripts/check_go_install_failures.sh`.

Direct calendar queries check tabulated bounds, including fixing calendars
retrieved from indexes after their original handles close. Core instrument
calculations can still encounter core calendar limitations; panic containment
and session poisoning remain part of the contract. Related
inflation dependencies still require the intended shared Settings identity.

`AGENTS.md`, `.agents/` guidance, and `CLAUDE.md` in the original checkout are
excluded via local Git configuration, so they are not shared review artifacts.
The tracked README, binding contract, installation guide, and issue tracker
are the current shared development references.
