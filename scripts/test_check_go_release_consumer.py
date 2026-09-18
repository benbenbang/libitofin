import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("check_go_release_consumer.sh")
REVISION = "a" * 40
MODULE = "github.com/benbenbang/libitofin/sdk/go"
FAKE_GO = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
mode = os.environ.get("TEST_MODE", "")
with open(os.environ["TEST_LOG"], "a") as log:
    log.write(json.dumps({"args": args, "env": dict(os.environ), "cwd": os.getcwd()}) + "\n")
if args[:2] == ["mod", "download"]:
    query = args[-1].split("@")[-1]
    if mode == "unavailable":
        print(json.dumps({"Error": "not yet published"}))
        sys.exit(1)
    record = {"Path": "github.com/benbenbang/libitofin/sdk/go", "Version": "v0.23.0",
              "Sum": "h1:source", "GoModSum": "h1:manifest"}
    if mode == "origin":
        record["Origin"] = {"Hash": "b" * 40}
    if mode == "version":
        record["Version"] = "v0.21.0"
    if mode == "legacy-path":
        record["Path"] = "github.com/benbenbang/libitofin/bindings/go"
    if mode == "checksum" and query == "a" * 40:
        record["Sum"] = "h1:different"
    print(json.dumps(record))
elif args == ["mod", "edit", "-json"]:
    print(json.dumps({"Replace": [{"Old": {"Path": "bad"}}]} if mode == "replace" else {}))
elif args[:3] == ["list", "-m", "-json"]:
    print(json.dumps({"Path": "github.com/benbenbang/libitofin/sdk/go",
                      "Version": "v0.23.0"}))
elif args and args[0] == "list":
    print("[] []")
elif args and args[0] == "build":
    output = Path(args[args.index("-o") + 1])
    output.write_text("#!/bin/sh\nprintf 'native session ready\\n'\n")
    output.chmod(0o755)
'''


class ReleaseConsumerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.native = self.root / "native package"
        (self.native / "include").mkdir(parents=True)
        (self.native / "lib").mkdir()
        (self.native / "include/itofin.h").touch()
        for library in ("libitofin_ffi.dylib", "libitofin_ffi.so"):
            (self.native / "lib" / library).touch()
        self.metadata = self.native / "VERSION"
        self.metadata.write_text(f"version=0.23.0\nrevision={REVISION}\n")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name, content in (("go", FAKE_GO), ("sleep", "#!/bin/sh\nexit 0\n")):
            tool = self.bin / name
            tool.write_text(content)
            tool.chmod(0o755)
        self.log = self.root / "calls.jsonl"
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        TEST_LOG=str(self.log), GOPROXY="off", GOSUMDB="off",
                        GOWORK="/unwanted/go.work", GOFLAGS="-modfile=unwanted.mod",
                        GOPRIVATE="*", GONOPROXY="*", GONOSUMDB="*")

    def run_consumer(self, version="0.23.0", revision=REVISION, mode=""):
        return subprocess.run(["bash", str(SCRIPT), str(self.native), version, revision],
                              env=dict(self.env, TEST_MODE=mode), capture_output=True,
                              text=True, timeout=30)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_valid_metadata_and_isolated_public_consumer(self):
        result = self.run_consumer()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        downloads = [call["args"][-1] for call in calls if call["args"][:2] == ["mod", "download"]]
        self.assertEqual(downloads, [f"{MODULE}@v0.23.0", f"{MODULE}@{REVISION}"])
        for call in calls:
            env = call["env"]
            self.assertEqual(env["GOPROXY"], "https://proxy.golang.org,direct")
            self.assertEqual(env["GOSUMDB"], "sum.golang.org")
            self.assertEqual(env["GOWORK"], "off")
            self.assertEqual(env["GOENV"], "off")
            self.assertEqual(env["CGO_ENABLED"], "1")
            for variable in ("GOFLAGS", "GOPRIVATE", "GONOPROXY", "GONOSUMDB"):
                self.assertEqual(env[variable], "")
            self.assertNotIn("-replace", " ".join(call["args"]))
            self.assertFalse(Path(env["GOMODCACHE"]).exists())
            self.assertFalse(Path(call["cwd"]).exists())
        self.assertTrue(any(call["args"][:2] == ["mod", "verify"] for call in calls))
        self.assertTrue(any("-race" in call["args"] for call in calls))

    def test_invalid_input_rejected_before_go(self):
        for version, revision in (("v0.23.0", REVISION), ("00.23.0", REVISION),
                                  ("0.23.0-beta.1", REVISION), ("0.23.0", "short")):
            with self.subTest(version=version, revision=revision):
                result = self.run_consumer(version, revision)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("usage:", result.stderr)
                self.assertFalse(self.log.exists())

    def test_native_metadata_must_match(self):
        for metadata in (f"version=0.21.0\nrevision={REVISION}\n",
                         "version=0.23.0\nrevision=wrong\n",
                         f"version=0.23.0\nversion=0.23.0\nrevision={REVISION}\n"):
            with self.subTest(metadata=metadata):
                self.metadata.write_text(metadata)
                self.assertNotEqual(self.run_consumer().returncode, 0)
                self.assertFalse(self.log.exists())

    def test_remote_mismatches_and_replace_are_rejected(self):
        for mode, diagnostic in (("origin", "origin"), ("version", "path/version"),
                                 ("legacy-path", "path/version"),
                                 ("checksum", "disagree"), ("replace", "replace directive")):
            with self.subTest(mode=mode):
                result = self.run_consumer(mode=mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)
                self.assertFalse(any("-race" in call["args"] for call in self.calls()))

    def test_retry_budget_is_bounded(self):
        result = self.run_consumer(mode="unavailable")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unavailable after 6 attempts", result.stderr)
        self.assertEqual(sum(call["args"][:2] == ["mod", "download"] for call in self.calls()), 6)


if __name__ == "__main__":
    unittest.main()
