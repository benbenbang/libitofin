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
ITOFIN_HCAL_CORE_ORACLE=$(cargo run --quiet --release -p libitofin-ffi --example calibration_constraints_oracle)
export ITOFIN_HCAL_CORE_ORACLE
cargo run --quiet --release -p libitofin-ffi --example optimization_methods_oracle > target/optimization-methods-oracle.json
python3 -m json.tool target/optimization-methods-oracle.json > /dev/null
export ITOFIN_OPTIMIZATION_METHODS_CORE_ORACLE_JSON="$PWD/target/optimization-methods-oracle.json"
cargo run --quiet --release -p libitofin --example fd_binding_oracle > target/fd-binding-oracle.json
export ITOFIN_FD_ORACLE_JSON="$PWD/target/fd-binding-oracle.json"
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
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_vwap_obv.c -Ltarget/release -litofin_ffi -o target/c-chart-vwap_obv
./target/c-chart-vwap_obv
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_vwap_obv.c -Ltarget/release -litofin_ffi -o target/cpp-chart-vwap_obv
./target/cpp-chart-vwap_obv
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_atr.c -Ltarget/release -litofin_ffi -o target/c-chart-atr
./target/c-chart-atr
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_atr.c -Ltarget/release -litofin_ffi -o target/cpp-chart-atr
./target/cpp-chart-atr
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_keltner.c -Ltarget/release -litofin_ffi -o target/c-chart-keltner
./target/c-chart-keltner
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_keltner.c -Ltarget/release -litofin_ffi -o target/cpp-chart-keltner
./target/cpp-chart-keltner
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_williams.c -Ltarget/release -litofin_ffi -o target/c-chart-williams
./target/c-chart-williams
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_williams.c -Ltarget/release -litofin_ffi -o target/cpp-chart-williams
./target/cpp-chart-williams
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/drawdown.c -Ltarget/release -litofin_ffi -o target/c-drawdown
./target/c-drawdown
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/drawdown.c -Ltarget/release -litofin_ffi -o target/cpp-drawdown
./target/cpp-drawdown
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/benchmark_beta.c -Ltarget/release -litofin_ffi -o target/c-benchmark-beta
./target/c-benchmark-beta
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/benchmark_beta.c -Ltarget/release -litofin_ffi -o target/cpp-benchmark-beta
./target/cpp-benchmark-beta
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/performance_ratios.c -Ltarget/release -litofin_ffi -o target/c-performance-ratios
./target/c-performance-ratios
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/performance_ratios.c -Ltarget/release -litofin_ffi -o target/cpp-performance-ratios
./target/cpp-performance-ratios
cargo build -p libitofin-ffi --release --features optimization-method-oracle
cd sdk/go
go vet ./...
go vet -tags optimization_oracle ./...
go test -tags optimization_oracle -race -count=1 -run "^TestOptimizationMethodsQuantLibParabola$" ./...
go test -race -count=1 -coverprofile=../../target/go-coverage.out ./...
go run ./examples/european_option
go run ./examples/portfolio

go run ./examples/drawdown
go run ./examples/benchmark_beta

go run ./examples/performance_ratios
