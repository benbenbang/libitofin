#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python -c 'import sys; assert sys.prefix != sys.base_prefix, "activate a Python venv with maturin, numpy, and pytest"'
command -v maturin >/dev/null
scratch=$(mktemp -d "${TMPDIR:-/tmp}/itofin-mixed-builds.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
export PYO3_PYTHON
PYO3_PYTHON=$(command -v python)
unset CARGO_BUILD_TARGET

build_ffi() {
  cargo build --locked -p libitofin-ffi
}

build_python() {
  maturin develop --locked --manifest-path crates/itofin-py/Cargo.toml
  python crates/itofin-py/scripts/gen_stubs.py --check
}

check_both() {
  case "$(uname -s)" in
    Darwin) test -f "$CARGO_TARGET_DIR/debug/libitofin_ffi.dylib" ;;
    Linux) test -f "$CARGO_TARGET_DIR/debug/libitofin_ffi.so" ;;
    *) echo "Unsupported mixed-build host" >&2; exit 1 ;;
  esac
  python crates/itofin-py/scripts/gen_stubs.py --check
  python -m pytest -q crates/itofin-py/tests/test_time_eq_hash.py
  cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
    crates/libitofin-ffi/tests/c_smoke.c -L"$CARGO_TARGET_DIR/debug" -litofin_ffi -o "$scratch/c-smoke"
  c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
    crates/libitofin-ffi/tests/c_smoke.c -L"$CARGO_TARGET_DIR/debug" -litofin_ffi -o "$scratch/cpp-smoke"
  LD_LIBRARY_PATH="$CARGO_TARGET_DIR/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    DYLD_LIBRARY_PATH="$CARGO_TARGET_DIR/debug${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" "$scratch/c-smoke"
  LD_LIBRARY_PATH="$CARGO_TARGET_DIR/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    DYLD_LIBRARY_PATH="$CARGO_TARGET_DIR/debug${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" "$scratch/cpp-smoke"
}

for order in ffi-first python-first; do
  export CARGO_TARGET_DIR="$scratch/$order"
  for phase in clean incremental; do
    printf '\nMixed build: %s (%s)\n' "$order" "$phase"
    if [[ $order == ffi-first ]]; then
      build_ffi
      build_python
    else
      build_python
      build_ffi
    fi
    check_both
  done
  cargo build --locked --workspace 2>&1 | tee "$scratch/workspace.log"
  if grep -qi 'output filename collision' "$scratch/workspace.log"; then
    echo 'Workspace emitted an output filename collision' >&2
    exit 1
  fi
  check_both
done
