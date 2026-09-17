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
passed against the native archive built from packaging commit `0ea15c1b`.
The extracted package path contained spaces. The copied module had no adjacent
Rust source tree or Cargo target directory. Remote Linux CI was not run here.

The following single-iteration measurements used two assets and 252 steps,
without the race detector. They are smoke measurements under concurrent local
development load, not stable performance budgets or application throughput claims.

| Paths | Output | Time/op | Go-allocated bytes/op |
| --- | --- | ---: | ---: |
| 1,000 | Full paths | 34.1 ms | 4,062,392 |
| 1,000 | Terminal | 29.2 ms | 17,952 |
| 10,000 | Full paths | 401.7 ms | 40,486,640 |
| 10,000 | Terminal | 386.7 ms | 165,408 |

Go's allocation counters exclude native allocations. The separate benchmark
process peaked at 93,552,640 bytes resident across all four cases, measured by
`/usr/bin/time -l`; this includes native memory and the Go runtime. The largest
full-path result itself contains 40,480,000 bytes of doubles. The existing output
limit remains 16 million doubles, with an additional native result buffer.

Application migration, production workload selection, deployment-platform
validation, and latency/memory budgets require acceptance in the downstream
consumer repository. Release-tag installation must also be checked when the
first Go module tag is published; this run used a local module replacement.
