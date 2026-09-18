#!/usr/bin/env bash
set -euo pipefail
ulimit -c 0
if [[ $# != 3 || ! $2 =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ || ! $3 =~ ^[0-9a-f]{40}$ ]]; then
  echo 'usage: check_go_release_consumer.sh EXTRACTED_NATIVE_DIR VERSION FULL_REVISION' >&2
  exit 1
fi
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
native_root="$(cd "$1" && pwd)"
version=$2 revision=$3
case "$(uname -s)" in
  Darwin) library=libitofin_ffi.dylib ;;
  Linux) library=libitofin_ffi.so ;;
  *) echo 'Release consumer supports Linux and macOS' >&2; exit 1 ;;
esac
python3 - "$native_root/VERSION" "$version" "$revision" <<'PYTHON'
import pathlib
import sys

fields = {}
for line in pathlib.Path(sys.argv[1]).read_text().splitlines():
    key, value = line.split("=", 1)
    if key in fields:
        raise SystemExit(f"Duplicate native metadata: {key}")
    fields[key] = value
for key, expected in zip(("version", "revision"), sys.argv[2:]):
    if fields.get(key) != expected:
        raise SystemExit(f"Native {key} does not match release: {fields.get(key)!r}")
PYTHON
test -f "$native_root/include/itofin.h"
test -f "$native_root/lib/$library"
work="$(mktemp -d "${TMPDIR:-/tmp}/itofin-release-consumer.XXXXXX")"
trap 'chmod -R u+w "$work"; rm -rf "$work"' EXIT
cp -R "$native_root" "$work/native"
mkdir "$work/consumer"
cp "$repo_root/scripts/fixtures/go-consumer/"* "$work/consumer/"
mkdir "$work/consumer/session"
cp "$repo_root/scripts/fixtures/go-install-failures/main.go" "$work/consumer/session/main.go"
export GOWORK=off GOENV=off GOFLAGS="" CGO_ENABLED=1
export GOPROXY=https://proxy.golang.org,direct GOSUMDB=sum.golang.org
export GOPRIVATE="" GONOPROXY="" GONOSUMDB=""
export GOMODCACHE="$work/modcache" GOCACHE="$work/buildcache"
export CGO_CFLAGS="\"-I$work/native/include\""
export CGO_LDFLAGS="\"$work/native/lib/$library\" \"-Wl,-rpath,$work/native/lib\""
export ITOFIN_EXPECTED_VERSION="$version"
unset CGO_CPPFLAGS CGO_CXXFLAGS LIBRARY_PATH CPATH C_INCLUDE_PATH
unset LD_LIBRARY_PATH LD_PRELOAD DYLD_LIBRARY_PATH DYLD_INSERT_LIBRARIES DYLD_FALLBACK_LIBRARY_PATH
cd "$work/consumer"
module=github.com/benbenbang/libitofin/sdk/go
go version
go mod edit -require="$module@v$version"
for query in "v$version" "$revision"; do
  for attempt in 1 2 3 4 5 6; do
    if go mod download -json "$module@$query" > "$work/$query.json"; then
      break
    fi
    if [[ $attempt == 6 ]]; then
      cat "$work/$query.json" >&2
      echo "Published module unavailable after $attempt attempts: $query" >&2
      exit 1
    fi
    echo "Waiting 20 seconds for published module $query (attempt $attempt/6)" >&2
    sleep 20
  done
done
python3 - "$work/v$version.json" "$work/$revision.json" "$module" "v$version" "$revision" <<'PYTHON'
import json
import sys

records = [json.load(open(path)) for path in sys.argv[1:3]]
for record in records:
    if record.get("Error") or record.get("Replace"):
        raise SystemExit("Published module download contains an error or replacement")
    if record.get("Path") != sys.argv[3] or record.get("Version") != sys.argv[4]:
        raise SystemExit("Published module path/version does not match release")
    origin = record.get("Origin", {})
    if origin.get("Hash", sys.argv[5]) != sys.argv[5]:
        raise SystemExit("Published module origin does not match release revision")
for field in ("Sum", "GoModSum"):
    value = records[0].get(field, "")
    if not value.startswith("h1:") or records[1].get(field) != value:
        raise SystemExit(f"Published version and revision disagree: {field}")
print(f"Verified {sys.argv[3]}@{sys.argv[4]} at {sys.argv[5]}")
PYTHON
go mod tidy
go mod edit -json > "$work/go-mod.json"
go list -m -json "$module" > "$work/resolved.json"
python3 - "$work/go-mod.json" "$work/resolved.json" "$module" "v$version" <<'PYTHON'
import json
import sys

manifest, resolved = (json.load(open(path)) for path in sys.argv[1:3])
if manifest.get("Replace") or resolved.get("Replace"):
    raise SystemExit("Published consumer must not use a replace directive")
if resolved.get("Path") != sys.argv[3] or resolved.get("Version") != sys.argv[4]:
    raise SystemExit("Consumer resolved a different module version")
PYTHON
go mod verify
test "$(go list -tags itofin_external -f '{{.CgoCFLAGS}} {{.CgoLDFLAGS}}' "$module")" = '[] []'
go vet -tags itofin_external ./...
GOEXPERIMENT=cgocheck2 go test -tags itofin_external -race -count=1 -v ./...
go build -tags itofin_external -o "$work/session" ./session
test "$("$work/session")" = 'native session ready'
printf 'PASS: published Go v%s and native package at %s\n' "$version" "$revision"
