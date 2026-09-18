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
export CGO_CFLAGS="\"-I$native_root/include\""
export CGO_LDFLAGS="\"-L$native_root/lib\" -litofin_ffi \"-Wl,-rpath,$native_root/lib\""
unset LD_LIBRARY_PATH DYLD_LIBRARY_PATH
cd "$consumer_root"
go version
go mod edit -replace="github.com/benbenbang/libitofin/sdk/go=$consumer_root/binding"
go vet -tags itofin_external ./...
GOEXPERIMENT=cgocheck2 go test -tags itofin_external -race -count=1 -v ./...
go test -tags itofin_external -c -o "$consumer_root/consumer.test" .
case "$(uname -s)" in
  Darwin) /usr/bin/time -l "$consumer_root/consumer.test" -test.run '^$' -test.bench . -test.benchtime=1x -test.benchmem ;;
  Linux) /usr/bin/time -v "$consumer_root/consumer.test" -test.run '^$' -test.bench . -test.benchtime=1x -test.benchmem ;;
  *) echo 'Consumer memory measurement supports Linux and macOS' >&2; exit 1 ;;
esac
