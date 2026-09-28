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
  MINGW*) library=itofin_ffi.dll; import_library=libitofin_ffi.dll.a ;;
  *) echo 'Installation failure checks support Linux, macOS, and Windows GNU' >&2; exit 1 ;;
esac
test -f "$native_root/lib/$library"
if [[ -n ${import_library:-} ]]; then
  test -f "$native_root/lib/$import_library"
fi
test -f "$native_root/include/itofin.h"
cp -R "$native_root" "$work/native"
cp -R "$go_source" "$work/binding"
cp "$repo_root/scripts/fixtures/go-install-failures/"{go.mod,main.go} "$work/"
export GOWORK=off GOPROXY=off CGO_ENABLED=1 GOFLAGS=
link_root=$work/native
binding_root=$work/binding
executable=consumer
if [[ -n ${import_library:-} ]]; then
  link_root=$(cygpath -m "$link_root")
  binding_root=$(cygpath -m "$binding_root")
  executable=consumer.exe
  export PATH="$work/native/lib:$PATH"
  export CGO_LDFLAGS="\"-L$link_root/lib\" -litofin_ffi"
else
  export CGO_LDFLAGS="\"$link_root/lib/$library\" \"-Wl,-rpath,$link_root/lib\""
fi
export CGO_CFLAGS="\"-I$link_root/include\""
unset CGO_CPPFLAGS CGO_CXXFLAGS LIBRARY_PATH CPATH C_INCLUDE_PATH
unset LD_LIBRARY_PATH LD_PRELOAD DYLD_LIBRARY_PATH DYLD_INSERT_LIBRARIES DYLD_FALLBACK_LIBRARY_PATH
cd "$work"
go mod edit -replace="github.com/benbenbang/libitofin/sdk/go=$binding_root"
test "$(go list -tags itofin_external -f '{{.CgoCFLAGS}} {{.CgoLDFLAGS}}' github.com/benbenbang/libitofin/sdk/go)" = '[] []'
go build -tags itofin_external -o "$executable" .
test "$(./$executable)" = 'native session ready'
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
  elif [[ $label == missing-at-load && -n ${import_library:-} && $status == 53 ]]; then
    :
  elif ! grep -F -- "$diagnostic" "$work/$label.log" |
    grep -Ei 'no such file|cannot find|not found|not loaded|cannot open shared object file|specified module'; then
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
if [[ -n ${import_library:-} ]]; then
  mv "$work/native/lib/$import_library" "$work/saved-import-library"
fi
if [[ -n ${import_library:-} ]]; then
  expect_failure missing-at-build itofin_ffi go build -a -tags itofin_external -o missing-consumer.exe .
else
  expect_failure missing-at-build "$library" go build -tags itofin_external -o missing-consumer .
fi
if [[ -n ${import_library:-} ]]; then
  mv "$work/saved-import-library" "$work/native/lib/$import_library"
fi
expect_failure missing-at-load "$library" "./$executable"
mv "$work/saved-library" "$work/native/lib/$library"

shim_source="$repo_root/scripts/fixtures/go-install-failures/shim/abi.c"
if [[ -n ${import_library:-} ]]; then
  if command -v python3 >/dev/null 2>&1; then
    python=python3
  else
    python=python
  fi
  objdump -p "$work/native/lib/$library" > "$work/exports.txt"
  "$python" - "$work/exports.txt" "$work/abi-shim.def" <<'PYTHON'
import pathlib
import re
import sys

exports = pathlib.Path(sys.argv[1]).read_text()
marker = "[Ordinal/Name Pointer] Table"
_, found, table = exports.partition(marker)
if not found:
    raise SystemExit("Native DLL has no readable export table")
names = set(re.findall(r"\b(itofin_[A-Za-z_0-9]+)\b", table))
required = {"itofin_abi_version", "itofin_context_new"}
if not required <= names:
    excerpt = "\n".join(table.splitlines()[:24])
    raise SystemExit(
        f"Native DLL exports do not include the session ABI: missing {sorted(required - names)}; "
        f"found {len(names)} itofin symbols\nExport-table excerpt:\n{excerpt}"
    )
lines = ["LIBRARY itofin_ffi", "EXPORTS", "    itofin_abi_version"]
lines.extend(f"    {name}=itofin_ffi_real.{name}" for name in sorted(names - {"itofin_abi_version"}))
pathlib.Path(sys.argv[2]).write_text("\n".join(lines) + "\n")
PYTHON
  mv "$work/native/lib/$library" "$work/native/lib/itofin_ffi_real.dll"
  gcc -std=c11 -Wall -Wextra -Werror -shared "$shim_source" "$work/abi-shim.def" \
    -o "$work/native/lib/$library"
  expect_failure incompatible-abi 'itofin: incompatible native ABI version' "./$executable"
  mv "$work/native/lib/itofin_ffi_real.dll" "$work/native/lib/$library"
elif [[ $library == *.dylib ]]; then
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
test "$(./$executable)" = 'native session ready'
printf '%s\n' 'PASS: valid native installation still works without the ABI shim'
