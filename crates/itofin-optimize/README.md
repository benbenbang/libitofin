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

The first publication is a maintainer bootstrap, not a new workspace release.
Do not merge the optimizer publish job or the core dependency until this bootstrap
and its trusted-publisher configuration are verified ([#1091](https://github.com/benbenbang/libitofin/issues/1091)).

1. Review and merge the license/bootstrap-only setup commit, still version
   **0.36.0**. Its optimizer source must match **v0.36.0**; the pristine tag lacks
   the bundled BSD license text. Do not use the differential evolution draft.
   Set `BOOTSTRAP_COMMIT` to that reviewed commit, then use a clean checkout:

   ```sh
   git diff --exit-code v0.36.0 "$BOOTSTRAP_COMMIT" -- Cargo.toml Cargo.lock \
     crates/itofin-optimize/Cargo.toml crates/itofin-optimize/src crates/itofin-optimize/tests
   git worktree add --detach /private/tmp/itofin-optimize-bootstrap "$BOOTSTRAP_COMMIT"
   cd /private/tmp/itofin-optimize-bootstrap
   python3 scripts/check_release_version.py v0.36.0
   cargo test -p itofin-optimize --locked
   cmp LICENSE crates/itofin-optimize/LICENSE
   cargo publish -p itofin-optimize --locked --dry-run
   ```

2. Review the package and obtain explicit publication approval. Authenticate
   locally with a maintainer crates.io token using `cargo login`, then run
   `cargo publish -p itofin-optimize --locked`. Never put the token in a command,
   repository file, log or GitHub secret. Verify version 0.36.0 on crates.io.
3. In that crate's crates.io settings, add a GitHub trusted publisher with owner
   `benbenbang`, repository `libitofin`, and workflow `semantic-release.yml`.
   Leave the environment field empty: the Rust publish jobs have no environment.
4. Keep the CI workflow activation in a separate, unmerged checkpoint until
   bootstrap and configuration are verified. Then merge the reviewed CI job.
   It publishes `itofin-optimize` before `libitofin`, from the same released tag,
   with the same version validator and OIDC authentication. Optimizer failure
   blocks core publication; Python and Go remain independently gated by release.
5. Verify a CI dry run performs no uploads, then observe both crate publications
   on the next authorized release before closing #1091. Do not rerun the old
   v0.36.0 coordinated release just to bootstrap this crate.

[Trusted Publishing](https://crates.io/docs/trusted-publishing) requires an
initial manual publication. The [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
explains dry-run package validation.

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
