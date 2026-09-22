"""Artifact contracts for byte-exact QuantLib license and notice delivery."""

import hashlib
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

import check_release_attribution as attribution


class AttributionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.expected = {
            name: f"canonical {name}\n".encode() for name in attribution.NAMES
        }

    def archive(self, kind, missing=False, corrupt=False, omit_hash=False):
        names = {
            "wheel": "itofin.whl",
            "sdist": "itofin.tar.gz",
            "native": "itofin-native-test.tar.gz",
        }
        path = self.root / names[kind]
        prefix = {
            "wheel": attribution.RESOURCE,
            "sdist": f"itofin/crates/itofin-py/python/{attribution.RESOURCE}",
            "native": "itofin/licenses/libitofin",
        }[kind]
        content = {f"{prefix}/{name}": data for name, data in self.expected.items()}
        key = f"{prefix}/{attribution.NAMES[0]}"
        if missing:
            del content[key]
        if corrupt:
            content[key] = b"changed"
        if kind == "native":
            lines = [
                f"{hashlib.sha256(data).hexdigest()}  licenses/libitofin/{name}\n"
                for name, data in self.expected.items()
            ]
            content["itofin/SHA256SUMS"] = (
                "" if omit_hash else "".join(lines)
            ).encode()
        if kind == "wheel":
            with zipfile.ZipFile(path, "w") as archive:
                for name, data in content.items():
                    archive.writestr(name, data)
        else:
            with tarfile.open(path, "w:gz") as archive:
                for name, data in content.items():
                    member = tarfile.TarInfo(name)
                    member.size = len(data)
                    archive.addfile(member, io.BytesIO(data))
        return path

    def test_repository_copies_match_canonical_notices(self):
        attribution.source_notices()

    def test_missing_and_drifted_source_copies_fail(self):
        for name, data in self.expected.items():
            canonical = self.root / "crates/libitofin" / name
            canonical.parent.mkdir(parents=True, exist_ok=True)
            canonical.write_bytes(data)
            copied = self.root / "crates/itofin-py/python" / attribution.RESOURCE / name
            copied.parent.mkdir(parents=True, exist_ok=True)
            copied.write_bytes(data)
        attribution.source_notices(self.root)
        copied.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "differs"):
            attribution.source_notices(self.root)
        copied.unlink()
        with self.assertRaises(FileNotFoundError):
            attribution.source_notices(self.root)

    def test_archive_bytes_and_native_hash_coverage(self):
        for kind in ("wheel", "sdist", "native"):
            with self.subTest(kind=kind):
                attribution.check_archive(self.archive(kind), self.expected)
                for failure in ("missing", "corrupt"):
                    with self.assertRaisesRegex(ValueError, "Missing or changed"):
                        attribution.check_archive(
                            self.archive(kind, **{failure: True}), self.expected
                        )
        with self.assertRaisesRegex(ValueError, "SHA256SUMS"):
            attribution.check_archive(
                self.archive("native", omit_hash=True), self.expected
            )


if __name__ == "__main__":
    unittest.main()
