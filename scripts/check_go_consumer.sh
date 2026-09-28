#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
native_root="$(cd "${1:?usage: check_go_consumer.sh EXTRACTED_NATIVE_DIR [GO_SOURCE_DIR]}" && pwd)"
go_source="$(cd "${2:-$repo_root/sdk/go}" && pwd)"
consumer_root="$(mktemp -d "${TMPDIR:-/tmp}/itofin-consumer.XXXXXX")"
trap 'rm -rf "$consumer_root"' EXIT
test -f "$native_root/include/itofin.h"
test -f "$native_root/VERSION"
ITOFIN_EXPECTED_VERSION=$(sed -n 's/^version=//p' "$native_root/VERSION")
test -n "$ITOFIN_EXPECTED_VERSION"
export ITOFIN_EXPECTED_VERSION
cp "$repo_root/scripts/fixtures/go-consumer/"* "$consumer_root/"
cp -R "$go_source" "$consumer_root/binding"
export GOWORK=off GOPROXY=off CGO_ENABLED=1
link_root=$native_root
binding_root=$consumer_root/binding
if [[ $(uname -s) == MINGW* ]]; then
  test -f "$native_root/lib/itofin_ffi.dll"
  test -f "$native_root/lib/libitofin_ffi.dll.a"
  link_root=$(cygpath -m "$native_root")
  binding_root=$(cygpath -m "$binding_root")
  export PATH="$native_root/lib:$PATH"
  export CGO_LDFLAGS="\"-L$link_root/lib\" -litofin_ffi"
else
  export CGO_LDFLAGS="\"-L$link_root/lib\" -litofin_ffi \"-Wl,-rpath,$link_root/lib\""
fi
export CGO_CFLAGS="\"-I$link_root/include\""
unset LD_LIBRARY_PATH DYLD_LIBRARY_PATH
cd "$consumer_root"
go version
go mod edit -replace="github.com/benbenbang/libitofin/sdk/go=$binding_root"
go vet -tags itofin_external ./...
GOEXPERIMENT=cgocheck2 go test -tags itofin_external -race -count=1 -v ./...
case "$(uname -s)" in
  Darwin)
    go test -tags itofin_external -c -o "$consumer_root/consumer.test" .
    /usr/bin/time -l "$consumer_root/consumer.test" -test.run '^$' -test.bench . -test.benchtime=1x -test.benchmem ;;
  Linux)
    go test -tags itofin_external -c -o "$consumer_root/consumer.test" .
    /usr/bin/time -v "$consumer_root/consumer.test" -test.run '^$' -test.bench . -test.benchtime=1x -test.benchmem ;;
  MINGW*)
    go test -tags itofin_external -c -o "$consumer_root/consumer.test.exe" .
    "$consumer_root/consumer.test.exe" -test.run '^$' -test.bench . -test.benchtime=1x -test.benchmem
    printf '%s\n' 'Windows: external RSS measurement skipped; Go benchmark allocations reported' ;;
  *) echo 'Consumer memory measurement supports Linux, macOS, and Windows GNU' >&2; exit 1 ;;
esac
