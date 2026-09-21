# Third-party notices

`itofin-optimize` is an independent implementation written from the published
papers listed in the crate documentation. It has one runtime dependency,
`thiserror`, and adapts no third-party source.

## Inspected for design comparison only

The two crates below were read while choosing this crate's public API shape.
Nothing was copied, adapted or translated from either, and no clean-room
procedure was followed, so no clean-room claim is made here.

| Crate | Version | License | Source |
| --- | --- | --- | --- |
| argmin | 0.11.0 | MIT OR Apache-2.0 | https://github.com/argmin-rs/argmin |
| optimization | 0.2.0 | MIT | https://github.com/b52/optimization-rust |

## SciPy

SciPy is used at development time only, by `scripts/fixtures/optimize/gen_fixtures.py`,
to generate reference values that are checked in as JSON with the SciPy version
recorded. SciPy is never a dependency of this crate, and its SLSQP
implementation (the ACM TOMS 733 Fortran and its C translation) is deliberately
not read or adapted in any language.
