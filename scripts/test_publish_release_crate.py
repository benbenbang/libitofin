"""Offline contracts for verified, resumable Rust publication."""

import hashlib
import http.client
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch, Mock
import urllib.error

import publish_release_crate as release


NAME, VERSION, REVISION = "libitofin", "0.41.0", "a" * 40
SOURCE = "crates/libitofin"


def archive(changes=None, *, unsafe=None, link=False):
    files = {".cargo_vcs_info.json": json.dumps({"git": {"sha1": REVISION}, "path_in_vcs": SOURCE}).encode(),
             "Cargo.toml": b"normalized manifest", "src/lib.rs": b"source"}
    files.update(changes or {})
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as stream:
        for name, data in files.items():
            member = tarfile.TarInfo(f"{NAME}-{VERSION}/{name}")
            member.size = len(data)
            stream.addfile(member, io.BytesIO(data))
        if unsafe:
            member = tarfile.TarInfo(unsafe)
            if link:
                member.type = tarfile.SYMTYPE
                member.linkname = "/etc/passwd"
            stream.addfile(member, io.BytesIO(b""))
    return output.getvalue()


class RegistryTests(unittest.TestCase):
    def setUp(self):
        self.data = archive()
        self.checksum = hashlib.sha256(self.data).hexdigest()
        self.metadata = {"version": {"crate": NAME, "num": VERSION, "yanked": False, "checksum": self.checksum}}
        self.index = {"name": NAME, "vers": VERSION, "yanked": False, "cksum": self.checksum}
        self.expected = release.archive_files(self.data, NAME, VERSION, REVISION, SOURCE)

    def fetch(self, url, *, absent=False):
        if "api/v1" in url:
            return json.dumps(self.metadata).encode()
        if "index.crates.io" in url:
            return json.dumps(self.index).encode()
        return self.data

    def probe(self):
        return release.existing(NAME, VERSION, REVISION, SOURCE, self.expected, self.fetch)

    def test_exact_existing_package_and_index_confirmed(self):
        self.assertTrue(self.probe())

    def test_only_api_absence_is_unpublished(self):
        self.assertFalse(release.existing(NAME, VERSION, REVISION, SOURCE, self.expected, lambda *a, **k: None))
        with self.assertRaises(release.NotVisible):
            release.existing(NAME, VERSION, REVISION, SOURCE, self.expected,
                             lambda url, **k: self.fetch(url) if "api/v1" in url else None)

    def test_wrong_identity_yank_malformed_flag_and_checksum_rejected(self):
        original = self.metadata["version"].copy()
        for key, value in [("crate", "other"), ("num", "0.40.0"), ("yanked", True),
                           ("yanked", None), ("yanked", 0), ("checksum", "bad")]:
            with self.subTest(key=key, value=value):
                self.metadata["version"] = {**original, key: value}
                with self.assertRaises(ValueError):
                    self.probe()

    def test_index_conflicts_fail_closed(self):
        original = self.index.copy()
        for key, value in [("name", "other"), ("cksum", "b" * 64), ("yanked", True), ("yanked", None)]:
            with self.subTest(key=key):
                self.index = {**original, key: value}
                with self.assertRaises(ValueError):
                    self.probe()

    def test_registry_download_checksum_and_contents_both_verified(self):
        self.data = archive({"src/lib.rs": b"changed"})
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.probe()
        checksum = hashlib.sha256(self.data).hexdigest()
        self.metadata["version"]["checksum"] = self.index["cksum"] = checksum
        with self.assertRaisesRegex(ValueError, "files differ"):
            self.probe()

    def test_wrong_revision_path_dirty_or_archive_alias_rejected(self):
        for vcs in [{"git": {"sha1": "b" * 40}, "path_in_vcs": SOURCE},
                    {"git": {"sha1": REVISION}, "path_in_vcs": "other"},
                    {"git": {"sha1": REVISION, "dirty": True}, "path_in_vcs": SOURCE},
                    {"git": {"sha1": REVISION, "dirty": 0}, "path_in_vcs": SOURCE}]:
            with self.subTest(vcs=vcs), self.assertRaises(ValueError):
                release.archive_files(archive({".cargo_vcs_info.json": json.dumps(vcs).encode()}), NAME, VERSION, REVISION, SOURCE)
        for path in ["/absolute", f"{NAME}-{VERSION}/../escape", f"{NAME}-{VERSION}/src//alias", f"{NAME}-{VERSION}/src/lib.rs"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                release.archive_files(archive(unsafe=path), NAME, VERSION, REVISION, SOURCE)
        with self.assertRaises(ValueError):
            release.archive_files(archive(unsafe=f"{NAME}-{VERSION}/link", link=True), NAME, VERSION, REVISION, SOURCE)

    def test_all_sparse_index_name_lengths(self):
        for name, expected in [("a", "1/a"), ("ab", "2/ab"), ("abc", "3/a/abc"), ("abcd", "ab/cd/abcd")]:
            self.assertEqual(release.index_path(name), expected)


class RecoveryTests(unittest.TestCase):
    def test_completed_publication_never_uploads(self):
        upload = Mock()
        self.assertEqual(release.publish_once(lambda: True, upload), "already-published")
        upload.assert_not_called()

    def test_accepted_but_failed_cargo_confirmation_reconciles_once(self):
        probe = Mock(side_effect=[False, release.NotVisible(), True])
        upload, sleep = Mock(return_value=101), Mock()
        self.assertEqual(release.publish_once(probe, upload, sleep), "reconciled-after-publish-error")
        upload.assert_called_once_with()
        sleep.assert_called_once_with(2)

    def test_success_requires_public_registry_confirmation(self):
        probe = Mock(side_effect=[False, False, True])
        self.assertEqual(release.publish_once(probe, lambda: 0, lambda _: None), "published")

    def test_initial_pending_index_does_not_reupload(self):
        probe, upload = Mock(side_effect=[release.NotVisible(), True]), Mock()
        self.assertEqual(release.publish_once(probe, upload, lambda _: None), "already-published")
        upload.assert_not_called()

    def test_exhausted_confirmation_or_pending_fails_not_success(self):
        upload = Mock(return_value=101)
        with self.assertRaisesRegex(RuntimeError, "not confirmed"):
            release.publish_once(lambda: False, upload, lambda _: None)
        upload.assert_called_once_with()
        upload.reset_mock()
        with self.assertRaisesRegex(RuntimeError, "refusing another upload"):
            release.publish_once(Mock(side_effect=release.NotVisible()), upload, lambda _: None)
        upload.assert_not_called()

    def test_conflict_and_registry_failure_do_not_upload(self):
        for error in [ValueError("conflict"), RuntimeError("registry unavailable")]:
            upload = Mock()
            with self.assertRaises(type(error)):
                release.publish_once(Mock(side_effect=error), upload)
            upload.assert_not_called()


class TransportTests(unittest.TestCase):
    def test_retry_transient_transport_http_and_truncated_reads(self):
        failures = [urllib.error.URLError("offline"), OSError("reset"),
                    http.client.IncompleteRead(b"partial"),
                    urllib.error.HTTPError("url", 429, "rate", None, None),
                    urllib.error.HTTPError("url", 503, "unavailable", None, None)]
        for error in failures:
            with self.subTest(error=error), patch.object(release.urllib.request, "urlopen", side_effect=[error, io.BytesIO(b"ok")]) as fetch, patch.object(release.time, "sleep"):
                self.assertEqual(release.read_url("https://example.test"), b"ok")
                self.assertEqual(fetch.call_count, 2)

    def test_only_explicit_404_can_mean_absent(self):
        for code in [401, 403, 404]:
            with self.subTest(code=code), patch.object(release.urllib.request, "urlopen", side_effect=urllib.error.HTTPError("url", code, "bad", None, None)) as fetch:
                if code == 404:
                    self.assertIsNone(release.read_url("https://example.test", absent=True))
                else:
                    with self.assertRaises(urllib.error.HTTPError):
                        release.read_url("https://example.test", absent=True)
                self.assertEqual(fetch.call_count, 1)

    def test_retry_budget_exhaustion_is_failure(self):
        with patch.object(release.urllib.request, "urlopen", side_effect=OSError("offline")) as fetch, patch.object(release.time, "sleep") as sleep:
            with self.assertRaises(RuntimeError):
                release.read_url("https://example.test")
            self.assertEqual(fetch.call_count, 3)
            self.assertEqual(sleep.call_count, 2)

    def test_verified_package_keeps_locked_compilation_and_source_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            target = root / "target"
            output = target / "package" / f"{NAME}-{VERSION}.crate"
            output.parent.mkdir(parents=True)
            output.write_bytes(archive())
            metadata = {"target_directory": str(target), "packages": [{"name": NAME, "version": VERSION, "manifest_path": str(root / SOURCE / "Cargo.toml")}]}
            with patch.object(release, "check_version", return_value=VERSION), patch.object(release.subprocess, "check_output", side_effect=[REVISION, json.dumps(metadata).encode()]), patch.object(release.subprocess, "run") as run:
                version, source, files = release.prepare(root, NAME, "v" + VERSION, REVISION)
                self.assertEqual((version, source), (VERSION, SOURCE))
                self.assertTrue(files)
                run.assert_called_once_with(["cargo", "package", "-p", NAME, "--locked"], cwd=root, check=True)
            with patch.object(release, "check_version", return_value=VERSION), patch.object(release.subprocess, "check_output", return_value="b" * 40), patch.object(release.subprocess, "run") as run:
                with self.assertRaises(ValueError):
                    release.prepare(root, NAME, "v" + VERSION, REVISION)
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
