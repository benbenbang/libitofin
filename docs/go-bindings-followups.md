# Go binding follow-ups

Review branch: `feat/go-bindings-followups`, starting at `4c48647e`.
Tracker: [#1000](https://github.com/benbenbang/libitofin/issues/1000).
Changes are assembled as sequential commits from isolated feature branches.

## Scope

- Distinct `itofin_ffi` native artifacts allow Python and C/Go builds to coexist.
- A scoped root pre-commit hook runs the native and Go validation script.
- Stateful pseudorandom and low-discrepancy generators extend the original
  stateless portfolio simulation API without changing its draw order.
- Country/market calendars, joint calendars, queries, and comparable keys
  extend the time facade. Existing core holiday-table bounds are exposed as
  constants for checked native input validation; numerical algorithms are unchanged.
- Independent calibration and CDS repricing oracles, inflation metadata, and
  retained-dependency tests address selected behavioral gaps. The remaining
  cases are enumerated in [the test triage](go-binding-test-gaps.md).
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

## Delivery limits

The new release workflow validates Linux amd64 and macOS arm64 packages and
prepares a draft release only for a matching `bindings/go/vVERSION` tag.
Feature-branch validation does not publish a release. No release tag is created
by this implementation. First published-tag resolution and downstream application
migration remain release/consumer acceptance tasks.

Direct calendar queries check tabulated bounds. Core instruments or calendars
returned by existing index inspectors can still encounter core limitations;
panic containment and session poisoning remain part of the contract. Related
inflation dependencies still require the intended shared Settings identity.

`AGENTS.md` and `CLAUDE.md` in this local checkout are excluded via local Git
configuration, so they are not part of the review commits. The tracked README,
binding contract, installation guide, and issue tracker are the current shared
development references.
