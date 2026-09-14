# Go bindings

The Go package calls the same Rust core as the Python package through a C ABI.
It requires **Go 1.27.1**, cgo, a C compiler, and the native library. Build from
this repository checkout; there are no prebuilt native artifacts or Go release
tags for these bindings yet.

```sh
cargo build -p libitofin-ffi --release
export LD_LIBRARY_PATH="$PWD/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
# On macOS, use DYLD_LIBRARY_PATH instead.
cd bindings/go
go test ./...
go run ./examples/portfolio
```

The native artifacts are `target/release/libitofin.so` (Linux),
`libitofin.dylib` (macOS), and `libitofin.a`. The header is
`crates/libitofin-ffi/include/itofin.h`. The cgo flags use paths relative to this
checkout. An application using a published Go module would also need a native
distribution and suitable `CGO_CFLAGS`, `CGO_LDFLAGS`, and runtime loader paths;
the current supported workflow is a local checkout with a Go `replace` directive.

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

The Python `.pyi` files define the API coverage target. Reviewed mappings live
in `docs/go-coverage/`; run `python3 scripts/check_go_coverage.py --strict` from
the repository root. This measures declared API symbols, including enum members
and deduplicated overloads, and verifies referenced exports/identifiers/tests
exist. It is separate from line coverage or exhaustive numerical validation.

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
