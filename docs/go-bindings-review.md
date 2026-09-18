# Go / C ABI review

This document records the original implementation and its historical Linux
validation. For the subsequent review branch, current coverage, and new local/CI
evidence, see [Go binding follow-ups](go-bindings-followups.md). Counts and
publication notes below describe the original delivery, not the current tree.

Baseline: `bf6c5d640c1a0aac3184d8a24d77897e2df2b5ae` (libitofin 0.20.0).
Review branch: `feat/go-cabi-python-parity`. The branch contains a linear stack
of feature, test and integration commits; no PR or merge to main was created.

## Original result

The sibling `libitofin-ffi` crate exports 223 C entry points, and the Go package
(originally `bindings/go`, now `sdk/go`) uses Go 1.27.1. The existing Rust core and Python source are
unchanged. Explicit session workers preserve the core's thread confinement and
observer ownership model. Batched seeded Gaussian/GBM simulation provides a
usable portfolio path through one native call, with a runnable synthetic example.

All 744 declared Python API symbols have reviewed C/Go mappings: 139 types,
534 methods, 67 enum members, three functions and the version attribute.
Overloads are deduplicated; inherited methods count at their declaration;
containing types may be inferred from mapped members. Go representations adapt
Python operators, constructors and properties to methods, config fields and
comparable keys. This is API surface coverage, not proof of every behavior.
578 symbols explicitly reference tests; unmapped symbols and stale C/Go/test
references fail `scripts/check_go_coverage.py --strict`.

## Original validation on Linux

| Gate | Result |
| --- | --- |
| Rust core release tests | 2,788 unit tests, four integration tests, seven doctests passed; four doctests ignored by the existing suite |
| Native FFI release tests | 30 passed |
| Go 1.27.1 | 63 tests passed with race detection and `GOEXPERIMENT=cgocheck2` |
| Go package statement coverage | 79.5%; the runnable example is executed separately |
| Python API mappings | 744 / 744 |
| C and C++ callers | Generated header compiles and smoke programs execute |
| Workspace checks | Check, Clippy with warnings denied, and formatting passed |
| Go checks | Vet and formatting passed |
| Header | Reproducible with cbindgen 0.29.2 |

Numerical gates include original cached QuantLib pricing values, bootstrap
repricing, calibration observability, cap/floor identities, volatility stripping
and cube grids, ISDA credit values, inflation swaps and K-volatility slices.
The simulation tests separately verify deterministic draw order, terminal/full
path consistency, analytic moments and requested correlations. Existing numerical
tolerances were preserved. Lifecycle tests cover concurrent calls and closure,
foreign or released handles, retained dependencies, explicit settings, detached
results, panic poisoning, integer ranges and UTF-8 metadata.

## Review order

1. `docs/go-binding-contract.md`, `boundary.rs`, `session.go`: pointer, handle,
   thread and error contracts.
2. `simulation_kernel.rs`, `simulation_api.rs`, `simulation.go`: batching,
   correlation, reproducibility and output limits.
3. Time/settings/results, then market/curves/helpers/indexes.
4. Options/models/Monte Carlo and volatility grids/calibration.
5. Rate instruments/cashflows, credit, then inflation foundations and products.
6. Coverage mappings, numerical tests, build instructions and CI workflow.

All native source paths above are under `crates/libitofin-ffi/src`; Go paths are
under `sdk/go`. See `sdk/go/README.md` for build and ownership examples.

## Practical limits

- This is a source-checkout build. No native binaries, Go release tags or package
  releases were published. Go callers must build the native library and configure
  the dynamic loader path.
- Local execution was Linux only. The added CI workflow also defines macOS
  validation; remote CI was not part of the local validation recorded here.
- API coverage and broad numerical tests do not imply exhaustive argument,
  branch or calibration-variant coverage. Manifests retain specific test gaps.
- The existing core can panic on extreme schedule date/rule combinations. The
  boundary catches unwinds and poisons that session; callers must close it.
  Invalid raw C pointers and process-wide allocation failure are outside this
  recovery guarantee.
- Related inflation dependencies must share the intended `Settings` object,
  matching the Python contract; same-session checks do not enforce that identity.
- The batch simulation is ready for a consumer to call. An application/service
  migration, orchestration changes and optimization beyond the existing Python
  API are separate work. No private consumer source or data was copied.

Publication target: `bitbrew-dev/libitofin`, branch
`feat/go-cabi-python-parity`. The original target rejected writes with HTTP 403;
the owner supplied this writable fork. The final implementation tree matches
`79925a630b8f7a73a0f6c610edf579c2c4af10e0` from the validated local stack.

The published history preserves the first 41 development commits and consolidates
the remaining work into five integration commits, followed by this documentation
update. The original 139-commit development history remains in the portable review
bundle. No PR or merge to main was created.
