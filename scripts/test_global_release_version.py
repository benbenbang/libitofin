"""Keep the global optimizer dependency aligned with release version bumps."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]


class GlobalReleaseVersionTests(unittest.TestCase):
    def test_minor_and_patch_bumps_update_workspace_dependency(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copyfile(ROOT / "Cargo.toml", root / "Cargo.toml")
            shutil.copyfile(ROOT / "Cargo.lock", root / "Cargo.lock")
            for version in ("0.37.0", "0.37.1"):
                subprocess.run(
                    ["sh", str(ROOT / "scripts/set-version.sh"), version],
                    cwd=root,
                    check=True,
                    capture_output=True,
                )
                manifest = tomllib.loads((root / "Cargo.toml").read_text())
                self.assertEqual(manifest["workspace"]["package"]["version"], version)
                dependency = manifest["workspace"]["dependencies"]["itofin-optimize"]
                self.assertEqual(dependency["version"], version)
                self.assertEqual(dependency["path"], "crates/itofin-optimize")
                packages = tomllib.loads((root / "Cargo.lock").read_text())["package"]
                self.assertTrue(
                    all(p["version"] == version for p in packages if "source" not in p)
                )
                self.assertTrue(
                    any(p["version"] != version for p in packages if "source" in p)
                )

    def test_core_uses_shared_dependency(self):
        manifest = tomllib.loads((ROOT / "crates/libitofin/Cargo.toml").read_text())
        self.assertEqual(
            manifest["dependencies"]["itofin-optimize"], {"workspace": True}
        )


if __name__ == "__main__":
    unittest.main()
