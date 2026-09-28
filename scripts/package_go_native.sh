#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

output=${1:-target/go-native}
mkdir -p "$output"
output=$(cd "$output" && pwd)
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-amd64; host=x86_64-unknown-linux-gnu; library=libitofin_ffi.so ;;
  Linux-aarch64) platform=linux-arm64; host=aarch64-unknown-linux-gnu; library=libitofin_ffi.so ;;
  Darwin-arm64) platform=darwin-arm64; host=aarch64-apple-darwin; library=libitofin_ffi.dylib ;;
  Darwin-x86_64) platform=darwin-amd64; host=x86_64-apple-darwin; library=libitofin_ffi.dylib ;;
  MINGW*-x86_64) platform=windows-amd64; host=x86_64-pc-windows-gnu; library=itofin_ffi.dll; import_library=libitofin_ffi.dll.a ;;
  *) echo 'Supported native package hosts: Linux amd64/arm64, macOS amd64/arm64, and Windows GNU amd64' >&2; exit 1 ;;
esac
if command -v python3 >/dev/null 2>&1; then
  python=python3
else
  python=python
fi
write_checksums() {
  "$python" - "$@" <<'PYTHON'
import hashlib
from pathlib import Path
import sys

for name in sys.argv[1:]:
    print(f"{hashlib.sha256(Path(name).read_bytes()).hexdigest()}  {name}")
PYTHON
}
if [[ $(cbindgen --version) != 'cbindgen 0.29.2' ]]; then
  echo 'Install cbindgen 0.29.2: cargo install cbindgen --version 0.29.2 --locked' >&2
  exit 1
fi
metadata=$(cargo metadata --locked --no-deps --format-version 1)
version=$(printf '%s' "$metadata" | "$python" -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "libitofin-ffi"))')
target_dir=$(printf '%s' "$metadata" | "$python" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
if [[ $platform == windows-* ]]; then
  target_dir=$(cygpath -u "$target_dir")
fi
name="itofin-native-$version-$platform"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
package="$stage/$name"
mkdir -p "$package/include" "$package/lib"
cbindgen --config crates/libitofin-ffi/cbindgen.toml --crate libitofin-ffi --output "$package/include/itofin.h"
cmp crates/libitofin-ffi/include/itofin.h "$package/include/itofin.h"
cargo build --locked -p libitofin-ffi --release --target "$host"
cp "$target_dir/$host/release/$library" "$package/lib/"
if [[ $platform == windows-* ]]; then
  cp "$target_dir/$host/release/$import_library" "$package/lib/"
fi
if [[ $platform == darwin-* ]]; then
  install_name_tool -id "@rpath/$library" "$package/lib/$library"
  codesign --force --sign - "$package/lib/$library"
fi
abi=$("$python" - "$package/lib/$library" "$version" <<'PYTHON'
import ctypes
import sys

native = ctypes.CDLL(sys.argv[1])
native.itofin_version.restype = ctypes.c_char_p
native.itofin_version.argtypes = []
native.itofin_abi_version.restype = ctypes.c_uint32
native.itofin_abi_version.argtypes = []
if native.itofin_version().decode() != sys.argv[2]:
    raise SystemExit("Native library version does not match workspace version")
print(native.itofin_abi_version())
PYTHON
)
cp LICENSE "$package/"
mkdir -p "$package/licenses/libitofin"
cp crates/libitofin/{THIRD_PARTY_NOTICES.md,QUANTLIB_LICENSE.txt} "$package/licenses/libitofin/"
{
  printf 'version=%s\nabi=%s\nplatform=%s\n' "$version" "$abi" "$platform"
  printf 'revision=%s\n' "$(git rev-parse HEAD)"
  printf 'rustc=%s\n' "$(rustc --version)"
} > "$package/VERSION"
(
  cd "$package"
  libraries=("lib/$library")
  if [[ -n ${import_library:-} ]]; then
    libraries+=("lib/$import_library")
  fi
  write_checksums include/itofin.h "${libraries[@]}" LICENSE VERSION licenses/libitofin/* > SHA256SUMS
)
COPYFILE_DISABLE=1 tar -czf "$output/$name.tar.gz" -C "$stage" "$name"
(
  cd "$output"
  write_checksums "$name.tar.gz" > "$name.tar.gz.sha256"
)
"$python" scripts/check_release_attribution.py "$output/$name.tar.gz" >&2
printf '%s\n' "$output/$name.tar.gz"
