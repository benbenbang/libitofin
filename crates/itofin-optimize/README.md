# itofin-optimize

SciPy-inspired numerical optimization in Rust, over plain `f64` slices.

## Scope

A general-purpose `minimize` for Nelder-Mead, BFGS, L-BFGS-B and SLSQP, with an
`Objective` trait carrying its own error type, explicit budgets, a cancellation
hook and validated inputs. All four Rust solvers use the single `minimize`
entry point. L-BFGS-B bindings are tracked in issue #1086.

## Independent of libitofin

This crate is finance-independent and never depends on `libitofin`, in either
direction of the workspace. Its only runtime dependency is `thiserror`. The
QuantLib-ported optimizers in `libitofin::math::optimization` are a separate,
untouched code path.

## Acceptance policy

A solver is accepted on objective quality, feasibility and optimality conditions
within stated tolerances. It is never accepted on reproducing a SciPy trajectory
step for step: identical iterate sequences are not a goal, and a test asserting
one would be testing SciPy's arithmetic order rather than this implementation.

Reference values come from `scripts/fixtures/optimize/gen_fixtures.py`, which
records the SciPy version and the generation date into every fixture it writes.

## Release

The initial **0.36.0** publication is available on
[crates.io](https://crates.io/crates/itofin-optimize/0.36.0). Bootstrap and the
GitHub trusted-publisher configuration are verified:

- Owner `benbenbang`, repository `libitofin`, workflow `semantic-release.yml`;
  no GitHub environment is configured for the Rust publish jobs.
- Future coordinated releases publish `itofin-optimize` before `libitofin`, using
  the same released tag, committed-version checks and OIDC authentication.
  Optimizer publication failure blocks core publication.
- Python and Go release jobs remain independently gated by the release; their
  existing artifacts bundle optimizer code without an additional runtime install.
- CI dry runs perform no publication. The first successful future CI publication
  is still pending; [#1091](https://github.com/benbenbang/libitofin/issues/1091)
  remains open until it is verified. That issue retains historical bootstrap details.

See [Trusted Publishing](https://crates.io/docs/trusted-publishing) for authentication
and the [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
for package validation.

## Citations

* Nelder, J. A. and Mead, R. (1965), "A simplex method for function
  minimization", The Computer Journal 7(4), 308-313.
* Gao, F. and Han, L. (2012), "Implementing the Nelder-Mead simplex algorithm
  with adaptive parameters", Computational Optimization and Applications 51(1),
  259-277.
* Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
  Springer.
* Byrd, R. H., Lu, P., Nocedal, J. and Zhu, C. (1995), "A limited memory
  algorithm for bound constrained optimization", SIAM Journal on Scientific
  Computing 16(5), 1190-1208.
* Kraft, D. (1988), "A software package for sequential quadratic programming",
  DFVLR-FB 88-28, DLR German Aerospace Center.
* Lawson, C. L. and Hanson, R. J. (1974), Solving Least Squares Problems,
  Prentice-Hall.

Provenance and the notices for the crates inspected for design comparison are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## License

BSD-3-Clause, as for the rest of the workspace.
