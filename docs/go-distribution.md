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

The module is `github.com/benbenbang/libitofin/sdk/go`. Choose a release with
a matching `sdk/go/vVERSION` tag and native assets. Set `version` to that
release number without `v`,
and `platform` to `darwin-arm64` or `linux-amd64`. Download the archive and
checksum from the main LibItoFin release:

```sh
version=VERSION
platform=darwin-arm64
gh release download "v$version" --repo benbenbang/libitofin \
  --pattern "itofin-native-$version-$platform.tar.gz*"
shasum -a 256 -c "itofin-native-$version-$platform.tar.gz.sha256"
tar -xzf "itofin-native-$version-$platform.tar.gz"
export ITOFIN_NATIVE="$PWD/itofin-native-$version-$platform"
(cd "$ITOFIN_NATIVE" && shasum -a 256 -c SHA256SUMS)
export CGO_ENABLED=1
export CGO_CFLAGS="\"-I$ITOFIN_NATIVE/include\""
export CGO_LDFLAGS="\"-L$ITOFIN_NATIVE/lib\" -litofin_ffi \"-Wl,-rpath,$ITOFIN_NATIVE/lib\""
```

For a published coordinated release, run these commands in your application's module:

```sh
go get "github.com/benbenbang/libitofin/sdk/go@v$version"
go test -tags itofin_external ./...
go build -tags itofin_external ./...
```

Before publication, use a local `replace` pointing at the matching checkout's
`sdk/go` directory and require version `v0.0.0`. The Go source may also be
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

## Migration from v0.22.0

The published `bindings/go/v0.22.0` and core `v0.22.0` tags remain unchanged.
Existing applications can keep
`github.com/benbenbang/libitofin/bindings/go@v0.22.0` with its matching native
package. There is no `sdk/go/v0.22.0` release.

When the first SDK release is available, replace imports of
`github.com/benbenbang/libitofin/bindings/go` with
`github.com/benbenbang/libitofin/sdk/go`, require the new module version, and
install its matching native package. Run `go mod tidy` to remove the old module
requirement. The Go package name remains `itofin`; this move does not change its
API. For development before that release, use the local replacement above.

## Verify a consumer

From the repository, exercise an independently compiled consumer against an
extracted package:

```sh
bash scripts/check_go_consumer.sh "$ITOFIN_NATIVE"
```

CI also runs `bash scripts/check_go_install_failures.sh "$ITOFIN_NATIVE"`.
It verifies missing-library failures at link and load time, and exercises the
Go session's incompatible-ABI rejection through a temporary native shim.
All mutations are confined to temporary package copies.

This validates the external installation path and the portfolio simulation
contract. It does not establish compatibility with an unavailable downstream
application or establish a production latency budget.

## Release process

The main `semantic-release.yml` workflow publishes one LibItoFin release at
`vVERSION`. Rust and Python package versions remain unprefixed. The release tag,
committed workspace version, and every local Cargo.lock package must agree;
publication jobs validate those files instead of rewriting them after tagging.

Go publication builds and validates Linux amd64 and macOS arm64 packages from
that exact tag. It attaches both archives and checksums to the same public
release, then creates `sdk/go/vVERSION` at the same commit. This extra tag
is visible under GitHub Tags and resolves the nested Go module; it does not
create another release page. Go documents the prefix in
[Mapping versions to commits](https://go.dev/ref/mod#vcs-version).

After publication, both platforms install the uploaded assets and fetch the Go
module through the public module proxy into a fresh cache, without local
replacements. The check verifies module/native versions, source revision,
checksums, runtime linking, and the portfolio/session acceptance fixture.

First dispatch the main release workflow with `prompt=true`, `dry_run=true` to
check the next version and notes. Dispatch with `dry_run=false` to publish.
During the one-time prefix migration, `v0.21.0` aliases the existing `0.21.0`
commit; the original tag and release remain unchanged.

For recovery, dispatch `go-release.yml` with an existing coordinated core
`release_tag`. For the legacy v0.22.0 release, select its original workflow with
`gh workflow run go-release.yml --ref v0.22.0 -f release_tag=v0.22.0`; the current
workflow targets `sdk/go`. Runs for that tag are serialized. Existing native
archives are downloaded and verified, then preserved even if a rebuild differs
byte-for-byte. An interrupted archive-only upload can have its checksum repaired;
conflicting Go tags, invalid archives, or mismatched checksums fail without an
overwrite. The workflow never creates a second Go-specific release.

PRs call `go-package.yml` and include both platforms in `workflow-success`.
PR validation does not publish tags or assets. Native Linux compatibility must
be checked on deployment targets; these are not manylinux or musl packages.
Private application migration and production budgets remain consumer work.
Legacy v0.22.0 published-release evidence is recorded in
[#1025](https://github.com/benbenbang/libitofin/issues/1025).
