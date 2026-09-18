# Go native distribution

The external Go module uses a matching native C ABI package. The Go module does
not bundle native binaries or headers. Go 1.27.1, cgo, and a C compiler are
required. Native releases initially target Linux amd64 (Ubuntu 24.04, glibc 2.39
or a compatible newer system) and macOS arm64 (macOS 14 or newer). Windows,
Linux musl, and other architectures are not release targets yet.

## Build a native package

From a source checkout with the pinned Rust toolchain and Python 3 installed:

```sh
cargo install cbindgen --version 0.29.2 --locked
bash scripts/package_go_native.sh /tmp/itofin-native
```

The script builds for the current supported host and creates
`itofin-native-VERSION-PLATFORM.tar.gz` and its `.sha256` checksum. The package
contains `include/itofin.h`, the shared library under `lib/`, `LICENSE`, a
`VERSION` manifest with the ABI version, source revision, and Rust version,
and `SHA256SUMS`. The package and ABI versions are checked by loading the built
library and reading its exported identity functions.
It regenerates and verifies the header before building. It does not package
the Python extension or promise static linking support.

The Go source and native package must come from the same revision. ABI version
checks reject incompatible ABI generations; they do not detect every mismatch
between releases that add symbols. Keep the header and library together.

## Use from an external Go module

The commands below use `0.20.0` as an example. Choose the version of the archive
you built or downloaded, and set `platform` to `darwin-arm64` or `linux-amd64`.
Run these commands from the directory containing the archive and checksum:

```sh
version=0.20.0
platform=darwin-arm64
shasum -a 256 -c "itofin-native-$version-$platform.tar.gz.sha256"
tar -xzf "itofin-native-$version-$platform.tar.gz"
export ITOFIN_NATIVE="$PWD/itofin-native-$version-$platform"
(cd "$ITOFIN_NATIVE" && shasum -a 256 -c SHA256SUMS)
export CGO_ENABLED=1
export CGO_CFLAGS="\"-I$ITOFIN_NATIVE/include\""
export CGO_LDFLAGS="\"-L$ITOFIN_NATIVE/lib\" -litofin_ffi \"-Wl,-rpath,$ITOFIN_NATIVE/lib\""
```

Once a Go release tag exists, run these commands in your application's module:

```sh
go get "github.com/benbenbang/libitofin/bindings/go@v$version"
go test -tags itofin_external ./...
go build -tags itofin_external ./...
```

Before publication, use a local `replace` pointing at the matching checkout's
`bindings/go` directory and require version `v0.0.0`. The Go source may also be
copied into its own directory; with `itofin_external`, no paths to the Rust
checkout are compiled into the Go package. The tag is required for every Go
build, test, and vet command using the external package. Without it, the
package keeps its existing source-checkout build flags.

The absolute runtime search path above avoids loader environment variables.
If moving the native package, rebuild the executable with its new path. For a
relocatable application bundle, copy the library into `app/lib`, build the
executable into `app/bin`, and instead set its linker runtime search path to
`@executable_path/../lib` on macOS or `$ORIGIN/../lib` on Linux. Preserve the
literal `$ORIGIN` when constructing the environment variable. The packaged
macOS library uses `@rpath/libitofin_ffi.dylib` and is signed ad hoc after changing
its install name; production application signing remains the consumer's step.

## Verify a consumer

From the repository, exercise an independently compiled consumer against an
extracted package:

```sh
bash scripts/check_go_consumer.sh "$ITOFIN_NATIVE"
```

This validates the external installation path and the portfolio simulation
contract. It does not establish compatibility with an unavailable downstream
application or establish a production latency budget.

## Release process

`.github/workflows/go-package.yml` builds native archives on explicit
`ubuntu-24.04` and `macos-14` runners, checks both archive and file checksums,
and runs the external consumer. Pull requests call it from `pre-commit.yml`
and include both platforms in the required `workflow-success` check.
`go-release.yml` calls it for tags and manual runs; pull requests and manual
runs upload workflow artifacts without creating a release. Linux runtime compatibility must be
verified on deployment targets; these are not manylinux or musl packages.

A maintainer releases the nested Go module using a tag named
`bindings/go/vVERSION`, where `VERSION` matches the workspace package version.
The workflow rejects mismatched versions and creates a **draft** GitHub release
with the two archives and checksums only after both platforms pass. Review the
artifacts and publish that draft separately. The tag itself makes the Go module
resolvable independently of whether the GitHub release is still a draft.
No release tag or published native artifact is created by adding this workflow.

Go documents the required subdirectory tag prefix in
[Mapping versions to commits](https://go.dev/ref/mod#vcs-version).
