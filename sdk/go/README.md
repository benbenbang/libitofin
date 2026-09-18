# Go bindings

The Go package calls the same Rust core as the Python package through a C ABI.
It requires **Go 1.27.1**, cgo, a C compiler, and the native library. For a source checkout, use the commands below. For an external Go module,
see [native packages and installation](../../docs/go-distribution.md).
The [Go SDK site](https://benbenbang.github.io/libitofin/go/) includes setup,
session guidance, and worked examples.

```sh
cargo build -p libitofin-ffi --release
export LD_LIBRARY_PATH="$PWD/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
# On macOS, use DYLD_LIBRARY_PATH instead.
cd sdk/go
go test ./...
go run ./examples/european_option
go run ./examples/portfolio
```

The native artifacts are `target/release/libitofin_ffi.so` (Linux),
`libitofin_ffi.dylib` (macOS), and `libitofin_ffi.a`. The header is
`crates/libitofin-ffi/include/itofin.h`. The distinct `itofin_ffi` library name
allows C/Go and Python builds to share Cargo target directories. The cgo flags use paths
relative to this checkout. An external Go module uses the `itofin_external` build tag with the native
package headers and library paths described in the installation guide.

## Sessions and ownership

```go
s, err := itofin.NewSession()
if err != nil { return err }
defer s.Close()
quote, err := s.NewSimpleQuote(100)
if err != nil { return err }
defer quote.Close()
```

A session owns a Rust object graph on one OS thread. Go callers may share a
session across goroutines; calls are serialized. Use separate sessions for
independent concurrent graphs. Each session has explicit settings. Objects from
different sessions cannot be mixed. Closing an object releases its external
handle; other native objects retain their dependencies. Closing a session waits
for admitted calls and releases the graph. Close is idempotent. There are no
finalizers; close sessions explicitly, and do not copy session values.

Input slices must not be mutated during a call. Native code never retains Go
memory. Errors have a native status code and message; use `errors.As` with
`*itofin.Error`. A native panic is caught and poisons the session; close it and
create another. This does not recover invalid C pointers or process-wide
allocation failures.

## Portfolio simulation

`SimulateGBM` is a stateless batch call for correlated geometric Brownian motion.
It uses annualized arithmetic drift and volatility, with log increment
`(drift - volatility²/2) * dt + volatility * sqrt(dt) * normal`.
The positive-definite correlation matrix is row-major. A nonzero seed makes
runs reproducible; streams are not promised to match NumPy.

Full paths use `[path, time, asset]` order and include the initial row. Terminal
mode returns `[path, asset]` at the same simulated horizon. Already allocated
asset values should be summed into portfolio values without applying weights a
second time. The example demonstrates this using generic synthetic inputs.

The default output limit is 16 million doubles (128 MiB), with a same-sized
temporary native result. Use smaller batches or terminal mode for larger jobs.
`MaxOutputValues` explicitly overrides the convenience limit.

## Coverage and checks

CI enforces the full-current Python stub inventory and the original
`bf6c5d640c1a0aac3184d8a24d77897e2df2b5ae` implementation baseline. Reviewed
mappings live in `docs/go-coverage/`. Run both audits from the repository root
with baseline history available:

```sh
python3 scripts/check_go_coverage.py --strict
python3 scripts/check_go_coverage.py --strict --baseline
```

Python-only nonconstructible enum declarations are explicitly classified with
rationale and counted separately from implemented Go mappings. They cannot
replace baseline implementations. Invalid C/Go/test references fail both audits.
This measures declared API symbols, including enum members
and deduplicated overloads, and verifies referenced exports/identifiers/tests
exist. It is separate from line coverage or exhaustive numerical validation.

The root `go-bindings-tests` pre-commit hook runs the same validation for changes
to the Go/C ABI, core, Python stubs, coverage mappings, or build configuration.
It requires Go 1.27.1, the pinned Rust toolchain, Python 3, and C/C++ compilers.
Run it manually with `prek run go-bindings-tests --all-files`.

Run `bash scripts/check_go_bindings.sh` for native tests, strict cgo pointer
checks, race detection, and Go line coverage. The Go tests use cached QuantLib
values, mathematical identities, seeded simulation checks, and ownership and
concurrency tests. Core algorithms retain their existing limitations; exposing
them through Go does not add unsupported numerical behavior.

Regenerate the header with cbindgen 0.29.2:

```sh
cbindgen --config crates/libitofin-ffi/cbindgen.toml \
  --crate libitofin-ffi --output crates/libitofin-ffi/include/itofin.h
```
