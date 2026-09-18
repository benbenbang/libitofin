#!/usr/bin/env bash
set -euo pipefail
ulimit -c 0
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
native_root="$(cd "${1:?usage: check_go_install_failures.sh EXTRACTED_NATIVE_DIR [GO_SOURCE_DIR]}" && pwd)"
go_source="$(cd "${2:-$repo_root/sdk/go}" && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/itofin-install-failures.XXXXXX")"
trap 'rm -rf "$work"' EXIT
case "$(uname -s)" in
  Darwin) library=libitofin_ffi.dylib ;;
  Linux) library=libitofin_ffi.so ;;
  *) echo 'Installation failure checks support Linux and macOS' >&2; exit 1 ;;
esac
test -f "$native_root/lib/$library"
test -f "$native_root/include/itofin.h"
cp -R "$native_root" "$work/native"
cp -R "$go_source" "$work/binding"
cp "$repo_root/scripts/fixtures/go-install-failures/"{go.mod,main.go} "$work/"
export GOWORK=off GOPROXY=off CGO_ENABLED=1 GOFLAGS=
export CGO_CFLAGS="\"-I$work/native/include\""
export CGO_LDFLAGS="\"$work/native/lib/$library\" \"-Wl,-rpath,$work/native/lib\""
unset CGO_CPPFLAGS CGO_CXXFLAGS LIBRARY_PATH CPATH C_INCLUDE_PATH
unset LD_LIBRARY_PATH LD_PRELOAD DYLD_LIBRARY_PATH DYLD_INSERT_LIBRARIES DYLD_FALLBACK_LIBRARY_PATH
cd "$work"
go mod edit -replace="github.com/benbenbang/libitofin/sdk/go=$work/binding"
test "$(go list -tags itofin_external -f '{{.CgoCFLAGS}} {{.CgoLDFLAGS}}' github.com/benbenbang/libitofin/sdk/go)" = '[] []'
go build -tags itofin_external -o consumer .
test "$(./consumer)" = 'native session ready'
printf '%s\n' 'PASS: valid native installation creates and closes a session'

expect_failure() {
  local label=$1 diagnostic=$2 status
  shift 2
  if "$@" > "$work/$label.log" 2>&1; then
    echo "FAIL: $label unexpectedly succeeded" >&2
    exit 1
  else
    status=$?
  fi
  if [[ $label == incompatible-abi ]]; then
    if ! grep -Fx -- "$diagnostic" "$work/$label.log"; then
      cat "$work/$label.log" >&2
      echo "FAIL: $label did not report $diagnostic" >&2
      exit 1
    fi
  elif ! grep -F -- "$diagnostic" "$work/$label.log" |
    grep -Ei 'no such file|cannot find|not found|not loaded|cannot open shared object file'; then
    cat "$work/$label.log" >&2
    echo "FAIL: $label did not identify the missing native library" >&2
    exit 1
  fi
  if [[ $label == incompatible-abi && $status != 1 ]]; then
    echo "FAIL: incompatible ABI exited with $status instead of the consumer's error status 1" >&2
    exit 1
  fi
  printf 'PASS: %s rejected (exit %s)\n' "$label" "$status"
}

mv "$work/native/lib/$library" "$work/saved-library"
expect_failure missing-at-build "$library" go build -tags itofin_external -o missing-consumer .
expect_failure missing-at-load "$library" ./consumer
mv "$work/saved-library" "$work/native/lib/$library"

shim_source="$repo_root/scripts/fixtures/go-install-failures/shim/abi.c"
if [[ $library == *.dylib ]]; then
  cc -std=c11 -Wall -Wextra -Werror -dynamiclib "$shim_source" \
    "$work/native/lib/$library" -Wl,-rpath,"$work/native/lib" -o "$work/abi-shim.dylib"
  codesign --force --sign - "$work/abi-shim.dylib"
  expect_failure incompatible-abi 'itofin: incompatible native ABI version' \
    env DYLD_INSERT_LIBRARIES="$work/abi-shim.dylib" ./consumer
else
  cc -std=c11 -Wall -Wextra -Werror -shared -fPIC "$shim_source" -o "$work/abi-shim.so"
  expect_failure incompatible-abi 'itofin: incompatible native ABI version' \
    env LD_PRELOAD="$work/abi-shim.so" ./consumer
fi
test "$(./consumer)" = 'native session ready'
printf '%s\n' 'PASS: valid native installation still works without the ABI shim'
