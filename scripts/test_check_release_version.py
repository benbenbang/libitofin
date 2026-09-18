"""Exercise release identity validation without changing the checkout."""

from pathlib import Path
import tempfile
import unittest

from check_release_version import check_version, release_version


class ReleaseVersionTests(unittest.TestCase):
    def test_stable_tag_forms(self):
        for tag, version in (("0.21.0", "0.21.0"), ("v0.22.0", "0.22.0"),
                             ("bindings/go/v0.22.0", "0.22.0"), ("sdk/go/v0.23.0", "0.23.0")):
            with self.subTest(tag=tag):
                self.assertEqual(release_version(tag), version)

    def test_invalid_tag_forms(self):
        for tag in ("main", "v01.2.3", "v1.2", "v1.2.3-rc.1", "v1.2.3\n", "vv1.2.3",
                    "bindings/go/1.2.3", "sdk/go/1.2.3"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release_version(tag)

    def test_manifest_and_all_local_lockfile_packages_must_match(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.21.0"\n')
            lock = ('version = 4\n[[package]]\nname = "core"\nversion = "0.21.0"\n'
                    '[[package]]\nname = "binding"\nversion = "0.21.0"\n'
                    '[[package]]\nname = "dependency"\nversion = "1.0.0"\nsource = "registry"\n')
            (root / "Cargo.lock").write_text(lock)
            self.assertEqual(check_version(root, "v0.21.0"), "0.21.0")
            with self.assertRaisesRegex(ValueError, "workspace"):
                check_version(root, "v0.22.0")
            (root / "Cargo.lock").write_text(lock.replace('name = "binding"\nversion = "0.21.0"',
                                                        'name = "binding"\nversion = "0.20.0"'))
            with self.assertRaisesRegex(ValueError, "lockfile"):
                check_version(root, "v0.21.0")


if __name__ == "__main__":
    unittest.main()
