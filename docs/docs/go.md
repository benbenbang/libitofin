# Go SDK

The Go package calls the Rust core through cgo. Import it with an explicit name:

```go
import itofin "github.com/benbenbang/libitofin/sdk/go"
```

`itofin` is the package name; `go` is only the final directory in the module path.
The [Go API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go)
provides the exported types and methods.

## Install in an application

Use Go 1.27.1, a C compiler, and the native package from the **same release** as
the Go module. `go get` downloads Go source; it does not install native headers
or libraries. Published native packages support Linux amd64 (Ubuntu 24.04,
glibc 2.39 or compatible newer systems) and macOS arm64 (macOS 14 or newer).

The following commands pin the v0.24.0 release. With the GitHub CLI
installed, run them in your application's module directory. For a new project,
first run `go mod init example.com/pricing`.

=== "macOS arm64"

    ```sh
    version=0.24.0
    platform=darwin-arm64
    ```

=== "Linux amd64"

    ```sh
    version=0.24.0
    platform=linux-amd64
    ```

Download, verify, and extract the native package:

```sh
gh release download "v$version" --repo benbenbang/libitofin \
  --pattern "itofin-native-$version-$platform.tar.gz*"
shasum -a 256 -c "itofin-native-$version-$platform.tar.gz.sha256"
tar -xzf "itofin-native-$version-$platform.tar.gz"
export ITOFIN_NATIVE="$PWD/itofin-native-$version-$platform"
(cd "$ITOFIN_NATIVE" && shasum -a 256 -c SHA256SUMS)
export CGO_ENABLED=1
export CGO_CFLAGS="\"-I$ITOFIN_NATIVE/include\""
export CGO_LDFLAGS="\"-L$ITOFIN_NATIVE/lib\" -litofin_ffi \"-Wl,-rpath,$ITOFIN_NATIVE/lib\""
go get "github.com/benbenbang/libitofin/sdk/go@v$version"
```

Save this as `main.go`:

```go
package main

import (
    "fmt"
    "log"

    itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
    session, err := itofin.NewSession()
    if err != nil {
        log.Fatal(err)
    }
    defer session.Close()
    fmt.Println("Native version:", itofin.Version())
}
```

Use `itofin_external` for every build, run, test, and vet of an external consumer:

```sh
go run -tags itofin_external .
go test -tags itofin_external ./...
go vet -tags itofin_external ./...
go build -tags itofin_external .
```

The linker configuration embeds the absolute native-library directory. If you
move that directory, rebuild with its new path. Keep the header and library
together. For relocatable bundles and source-built native packages, see the
[native distribution guide](https://github.com/benbenbang/libitofin/blob/main/docs/go-distribution.md).

## Run examples from a source checkout

From the repository root, build the native library using the pinned Rust
toolchain. These examples use the matching Go source in the checkout and do
not need the external build tag.

```sh
cargo build --locked -p libitofin-ffi --release
```

Set the runtime library path in the same shell:

=== "macOS"

    ```sh
    export DYLD_LIBRARY_PATH="$PWD/target/release${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
    ```

=== "Linux"

    ```sh
    export LD_LIBRARY_PATH="$PWD/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    ```

Then run:

```sh
cd sdk/go
go run ./examples/european_option
go run ./examples/portfolio
```

The [European option walkthrough](getting-started.md#price-a-european-option)
uses the same inputs as Python and returns NPV `2.1333684449`.

## Portfolio simulation

This runnable example simulates two correlated assets with synthetic initial
allocations of 6,000 and 4,000 over one year. It sums their terminal values
without applying allocation weights a second time. The fixed seed makes the
run reproducible; its stream is not promised to match NumPy.

```go title="sdk/go/examples/portfolio/main.go"
--8<-- "sdk/go/examples/portfolio/main.go"
```

With v0.24.0, it prints `native 0.24.0; 1000 paths; mean terminal portfolio 10525.33`.

## Sessions and errors

A session owns the native object graph and runs its calls on one OS thread.
Defer `session.Close()` after successful creation; no finalizer releases the
session for you. Closing a session releases all its objects. You may also
close individual handles earlier; dependent native objects retain what they
need. Do not mix objects from different sessions or copy session values.

Calls from multiple goroutines are serialized within a session. Use separate
sessions for independent graphs. Calls into the same session during a
bootstrap callback return `ErrCallbackReentry`, including `Close`; close the
session after the callback returns.

Check returned errors before using results. Native errors expose a code and
message through `*itofin.Error`, accessible with `errors.As`. See the
[SDK guide](https://github.com/benbenbang/libitofin/blob/main/sdk/go/README.md)
for ownership, simulation layout, and validation details.
