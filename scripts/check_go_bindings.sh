#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
go_version=$(go env GOVERSION)
if [[ "$go_version" != go1.27.1 ]]; then
  printf 'Go 1.27.1 is required; found %s. Install the version pinned in sdk/go/go.mod.\n' "$go_version" >&2
  exit 1
fi
cargo test -p libitofin-ffi --release
cargo build -p libitofin-ffi --release
ITOFIN_EXPECTED_VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
print(next(package["version"] for package in json.load(sys.stdin)["packages"]
           if package["name"] == "libitofin-ffi"))
')
test -n "$ITOFIN_EXPECTED_VERSION"
export ITOFIN_EXPECTED_VERSION
python3 -m unittest discover -s scripts -p 'check_go_coverage_test.py'
python3 scripts/check_go_coverage.py --strict --baseline
python3 scripts/check_go_coverage.py --strict
export LD_LIBRARY_PATH="$PWD/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export DYLD_LIBRARY_PATH="$PWD/target/release${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export GOEXPERIMENT=cgocheck2
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/c_smoke.c -Ltarget/release -litofin_ffi -o target/c-smoke
./target/c-smoke
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/c_smoke.c -Ltarget/release -litofin_ffi -o target/cpp-smoke
./target/cpp-smoke
cd sdk/go
go vet ./...
go test -race -count=1 -coverprofile=../../target/go-coverage.out ./...
go run ./examples/european_option
go run ./examples/portfolio
