#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test "$(go env GOVERSION)" = go1.27.1
cargo test -p libitofin-ffi --release
cargo build -p libitofin-ffi --release
python3 -m unittest discover -s scripts -p 'check_go_coverage_test.py'
python3 scripts/check_go_coverage.py --strict --baseline
export LD_LIBRARY_PATH="$PWD/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export DYLD_LIBRARY_PATH="$PWD/target/release${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export GOEXPERIMENT=cgocheck2
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/c_smoke.c -Ltarget/release -litofin -o target/c-smoke
./target/c-smoke
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/c_smoke.c -Ltarget/release -litofin -o target/cpp-smoke
./target/cpp-smoke
cd bindings/go
go vet ./...
go test -race -coverprofile=../../target/go-coverage.out ./...
go run ./examples/portfolio
