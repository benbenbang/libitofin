# External Go consumer validation (#1007)

The public acceptance fixture uses synthetic portfolio allocations and a separate
Go module. It validates library consumption; no private application has been
migrated or claimed compatible by this test.

Run `bash scripts/check_go_consumer.sh EXTRACTED_NATIVE_DIR` after building and
extracting a [native package](go-distribution.md). The runner copies the Go
sources into a temporary standalone directory, disables module-network access,
and compiles with `itofin_external`. It unsets loader environment variables so
the packaged library must load through the configured runtime search path.

Checks cover native/package version agreement, seeded replay, full/terminal
layouts, retained initial allocations, single-asset growth, summing allocated
holdings without applying weights twice, log-return correlation, output limits,
and independent concurrent sessions with explicit closure. Vet and the acceptance
tests run with race detection and strict cgo pointer checks.

## Local evidence

On 2026-09-17, macOS arm64, Apple M4, Go 1.27.1 and Rust 1.96.0, the fixture
passed against the native archive built from integrated commit `34a5a8bc`.
The extracted package path contained spaces. The copied module had no adjacent
Rust source tree or Cargo target directory. This local run did not establish
Linux validation; fresh platform CI is recorded separately below.

The following single-iteration measurements used two assets and 252 steps,
without the race detector. They are smoke measurements under concurrent local
development load, not stable performance budgets or application throughput claims.

| Paths | Output | Time/op | Go-allocated bytes/op |
| --- | --- | ---: | ---: |
| 1,000 | Full paths | 13.9 ms | 4,062,168 |
| 1,000 | Terminal | 12.8 ms | 17,952 |
| 10,000 | Full paths | 157.5 ms | 40,486,416 |
| 10,000 | Terminal | 132.7 ms | 165,408 |

Go's allocation counters exclude native allocations. The separate benchmark
process peaked at 93,487,104 bytes resident across all four cases, measured by
`/usr/bin/time -l`; this includes native memory and the Go runtime. The largest
full-path result itself contains 40,480,000 bytes of doubles. The existing output
limit remains 16 million doubles, with an additional native result buffer.

## Integrated package CI

Linux amd64 and macOS arm64 package/consumer validation for `b169fabe15d86623b24f3dd76489533d59a3a4da`
passed in the [CI run](https://github.com/benbenbang/libitofin/actions/runs/35302175566). These jobs build matching
archives, verify checksums, and run the standalone fixture. They do not measure
the private application or validate a published Go tag.

Application migration, production workload selection, deployment-platform
validation, and latency/memory budgets require acceptance in the downstream
consumer repository. This run used a local module replacement; published-module
validation is recorded separately below.

## Published-release acceptance

The coordinated release workflow additionally runs
`bash scripts/check_go_release_consumer.sh EXTRACTED_NATIVE_DIR VERSION REVISION`
on Linux amd64 and macOS arm64. It uses downloaded release assets and a fresh
public Go module cache, without a source copy, replace directive, or workspace.
It checks the requested version and revision against module checksums and native
metadata before running the functional fixture. Version queries retry briefly
for public-proxy propagation. Legacy `bindings/go/v0.22.0` passed this acceptance
on both platforms; release evidence is recorded in
[#1025](https://github.com/benbenbang/libitofin/issues/1025). The source module has
since moved to `sdk/go`; its published-release acceptance is recorded in
[#1037](https://github.com/benbenbang/libitofin/issues/1037).
