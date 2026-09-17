#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

output=${1:-target/go-native}
mkdir -p "$output"
output=$(cd "$output" && pwd)
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-amd64; host=x86_64-unknown-linux-gnu; library=libitofin_ffi.so ;;
  Darwin-arm64) platform=darwin-arm64; host=aarch64-apple-darwin; library=libitofin_ffi.dylib ;;
  *) echo 'Supported native package hosts: Linux amd64 and macOS arm64' >&2; exit 1 ;;
esac
if [[ $(cbindgen --version) != 'cbindgen 0.29.2' ]]; then
  echo 'Install cbindgen 0.29.2: cargo install cbindgen --version 0.29.2 --locked' >&2
  exit 1
fi
metadata=$(cargo metadata --locked --no-deps --format-version 1)
version=$(printf '%s' "$metadata" | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "libitofin-ffi"))')
target_dir=$(printf '%s' "$metadata" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
name="itofin-native-$version-$platform"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
package="$stage/$name"
mkdir -p "$package/include" "$package/lib"
cbindgen --config crates/libitofin-ffi/cbindgen.toml --crate libitofin-ffi --output "$package/include/itofin.h"
cmp crates/libitofin-ffi/include/itofin.h "$package/include/itofin.h"
cargo build --locked -p libitofin-ffi --release --target "$host"
cp "$target_dir/$host/release/$library" "$package/lib/"
if [[ $platform == darwin-arm64 ]]; then
  install_name_tool -id "@rpath/$library" "$package/lib/$library"
  codesign --force --sign - "$package/lib/$library"
fi
abi=$(python3 - "$package/lib/$library" "$version" <<'PYTHON'
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
{
  printf 'version=%s\nabi=%s\nplatform=%s\n' "$version" "$abi" "$platform"
  printf 'revision=%s\n' "$(git rev-parse HEAD)"
  printf 'rustc=%s\n' "$(rustc --version)"
} > "$package/VERSION"
(
  cd "$package"
  shasum -a 256 include/itofin.h "lib/$library" LICENSE VERSION > SHA256SUMS
)
COPYFILE_DISABLE=1 tar -czf "$output/$name.tar.gz" -C "$stage" "$name"
(
  cd "$output"
  shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256"
)
printf '%s\n' "$output/$name.tar.gz"
