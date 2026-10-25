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
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_adx.c -Ltarget/release -litofin_ffi -o target/c-chart-adx
./target/c-chart-adx
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/chart_adx.c -Ltarget/release -litofin_ffi -o target/cpp-chart-adx
./target/cpp-chart-adx
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/callable_oas.c -Ltarget/release -litofin_ffi -o target/c-callable-oas
./target/c-callable-oas
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/callable_oas.c -Ltarget/release -litofin_ffi -o target/cpp-callable-oas
./target/cpp-callable-oas
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/callable_risk.c -Ltarget/release -litofin_ffi -o target/c-callable-risk
./target/c-callable-risk
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/callable_risk.c -Ltarget/release -litofin_ffi -o target/cpp-callable-risk
./target/cpp-callable-risk
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/amortizing_fixed_bond.c -Ltarget/release -litofin_ffi -o target/c-amortizing-fixed-bond
./target/c-amortizing-fixed-bond
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/amortizing_fixed_bond.c -Ltarget/release -litofin_ffi -o target/cpp-amortizing-fixed-bond
./target/cpp-amortizing-fixed-bond
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/bond_forward.c -Ltarget/release -litofin_ffi -o target/c-bond-forward
./target/c-bond-forward
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/bond_forward.c -Ltarget/release -litofin_ffi -o target/cpp-bond-forward
./target/cpp-bond-forward
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/float_float_swap.c -Ltarget/release -litofin_ffi -o target/c-float-float-swap
./target/c-float-float-swap
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/float_float_swap.c -Ltarget/release -litofin_ffi -o target/cpp-float-float-swap
./target/cpp-float-float-swap
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/asset_swap.c -Ltarget/release -litofin_ffi -o target/c-asset-swap
./target/c-asset-swap
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/asset_swap.c -Ltarget/release -litofin_ffi -o target/cpp-asset-swap
./target/cpp-asset-swap
cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/g2_process.c -Ltarget/release -litofin_ffi -lm -o target/c-g2-process
./target/c-g2-process
c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
  crates/libitofin-ffi/tests/g2_process.c -Ltarget/release -litofin_ffi -lm -o target/cpp-g2-process
./target/cpp-g2-process
for probe in c_currency black_delta black_calculator bond_risk bond_price_atm fx_forward gsr_process cash_digital percentage_strike gap_option gap_option_native mf_state_process hw_forward_process xccy_basis asset_digital amortizing_floating_bond; do
  cc -std=c11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
    "crates/libitofin-ffi/tests/$probe.c" -Ltarget/release -litofin_ffi -lm -o "target/c-$probe"
  "./target/c-$probe"
  c++ -x c++ -std=c++11 -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
    "crates/libitofin-ffi/tests/$probe.c" -Ltarget/release -litofin_ffi -lm -o "target/cpp-$probe"
  "./target/cpp-$probe"
done
for compiler in cc c++; do
  if [[ "$compiler" == cc ]]; then standard=c11; else standard=c++11; fi
  "$compiler" -x "${standard%%11}" -std="$standard" -Wall -Wextra -Werror -Icrates/libitofin-ffi/include \
    crates/libitofin-ffi/tests/amortizing_floating_oracle.c -Ltarget/release -litofin_ffi -lm -o "target/$compiler-amortizing-floating-oracle"
  "./target/$compiler-amortizing-floating-oracle" scripts/fixtures/amortizing_floating_bond/values.tsv
done
cargo build -p libitofin-ffi --release --features optimization-method-oracle
cd sdk/go
go vet ./...
go vet -tags optimization_oracle ./...
go test -tags optimization_oracle -race -count=1 -run "^TestOptimizationMethodsQuantLibParabola$" ./...
go test -race -count=1 -coverprofile=../../target/go-coverage.out ./...
go run ./examples/european_option
go run ./examples/portfolio
go run ./examples/g2_process
go run ./examples/gsr_process
go run ./examples/mf_state_process
go run ./examples/hw_forward_process
go run ./examples/gap_option
go run ./examples/percentage_strike

go run ./examples/drawdown
go run ./examples/benchmark_beta

go run ./examples/performance_ratios
go run ./examples/chart_adx

go run ./examples/callable-oas

go run ./examples/callable-risk

go run ./examples/bond-forward

go run ./examples/float-float-swap

go run ./examples/xccy_basis
go run ./examples/amortizing_floating_bond
