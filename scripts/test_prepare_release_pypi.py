"""Offline identity, retry and partial-publication contracts for PyPI releases."""

import contextlib
from http.client import IncompleteRead
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError
import zipfile

import prepare_release_pypi as release


VERSION = "0.41.0"
TAG = f"v{VERSION}"
PLATFORMS = (
    "manylinux_2_17_x86_64.manylinux2014_x86_64",
    "manylinux_2_17_aarch64.manylinux2014_aarch64",
    "macosx_11_0_arm64",
    "macosx_10_12_x86_64",
)


def write_wheel(directory, platform, *, name="itofin", version=VERSION, filename_version=VERSION, extra_metadata=False):
    path = directory / f"itofin-{filename_version}-cp310-abi3-{platform}.whl"
    with zipfile.ZipFile(path, "w") as archive:
        metadata = f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"
        archive.writestr(f"itofin-{filename_version}.dist-info/METADATA", metadata)
        if extra_metadata:
            archive.writestr("other.dist-info/METADATA", metadata)
        archive.writestr("itofin.so", b"native bytes")
    return path


def write_sdist(directory, *, name="itofin", version=VERSION):
    path = directory / f"itofin-{VERSION}.tar.gz"
    with tarfile.open(path, "w:gz") as archive:
        data = f"Name: {name}\nVersion: {version}\n".encode()
        member = tarfile.TarInfo(f"itofin-{VERSION}/PKG-INFO")
        member.size = len(data)
        archive.addfile(member, io.BytesIO(data))
    return path


def http_error(code):
    return HTTPError("https://pypi.org/", code, "failure", {}, None)


class PreparationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.dist = self.root / "dist"
        self.dist.mkdir()
        for platform in PLATFORMS:
            write_wheel(self.dist, platform)
        write_sdist(self.dist)
        self.requests = []
        self.delays = []
        self.outcomes = []

    def remote(self, paths=None):
        files = []
        for path in paths if paths is not None else sorted(self.dist.iterdir()):
            files.append({"filename": path.name, "digests": {"sha256": release.digest_file(path)},
                          "yanked": False, "url": f"https://files.pythonhosted.org/packages/{path.name}"})
        return {"info": {"name": "itofin", "version": VERSION}, "urls": files}

    def opener(self, request, timeout):
        self.requests.append((request.full_url, timeout))
        outcome = self.outcomes.pop(0)
        if isinstance(outcome, Exception):
            raise outcome
        data = json.dumps(outcome).encode() if isinstance(outcome, dict) else outcome
        return io.BytesIO(data)

    def prepare(self, *, verify_downloads=False):
        with contextlib.redirect_stdout(io.StringIO()):
            return release.prepare(TAG, self.dist, opener=self.opener, sleeper=self.delays.append,
                                   verify_downloads=verify_downloads)

    def snapshot(self):
        return {path.name: path.read_bytes() for path in self.dist.iterdir() if path.is_file()}

    def reject(self, outcome, message=None, *, verify_downloads=False):
        before = self.snapshot()
        self.outcomes = [outcome] if not isinstance(outcome, list) else outcome
        with self.assertRaises((ValueError, RuntimeError)) as raised:
            self.prepare(verify_downloads=verify_downloads)
        if message:
            self.assertIn(message, str(raised.exception))
        self.assertEqual(before, self.snapshot())

    def test_absent_release_preserves_all_files(self):
        before = self.snapshot()
        self.outcomes = [http_error(404)]
        self.assertTrue(self.prepare())
        self.assertEqual(before, self.snapshot())
        self.assertEqual(self.requests, [(f"https://pypi.org/pypi/itofin/{VERSION}/json", 30)])
        self.assertEqual(self.delays, [])

    def test_existing_identical_release_needs_no_upload(self):
        self.outcomes = [self.remote()]
        self.assertFalse(self.prepare())
        self.assertEqual(list(self.dist.iterdir()), [])

    def test_partial_publication_retains_only_missing_files(self):
        paths = sorted(self.dist.iterdir())
        self.outcomes = [self.remote(paths[:3])]
        self.assertTrue(self.prepare())
        self.assertEqual(sorted(self.dist.iterdir()), paths[3:])

    def test_empty_published_release_requires_all_files(self):
        self.outcomes = [self.remote([])]
        self.assertTrue(self.prepare())
        self.assertEqual(len(list(self.dist.iterdir())), 5)

    def test_conflict_on_last_file_does_not_remove_matching_files(self):
        remote = self.remote()
        remote["urls"][-1]["digests"]["sha256"] = "0" * 64
        self.reject(remote, "differs")

    def test_yanked_file_blocks_entire_preparation(self):
        remote = self.remote()
        remote["urls"][-1]["yanked"] = True
        self.reject(remote, "yanked")

    def test_optional_download_verifies_all_remote_bytes(self):
        paths = sorted(self.dist.iterdir())
        self.outcomes = [self.remote()] + [path.read_bytes() for path in paths]
        self.assertFalse(self.prepare(verify_downloads=True))
        self.assertEqual(len(self.requests), 6)

    def test_optional_download_failure_preserves_all_files(self):
        paths = sorted(self.dist.iterdir())
        self.reject([self.remote(), paths[0].read_bytes(), b"different bytes"], "download digest mismatch",
                    verify_downloads=True)

    def test_download_404_is_not_absence(self):
        self.reject([self.remote(), http_error(404)], "HTTP 404", verify_downloads=True)

    def test_download_url_must_use_pypi_host_without_credentials(self):
        for url in ("http://files.pythonhosted.org/test", "https://evil.test/test",
                    "https://user@files.pythonhosted.org/test"):
            with self.subTest(url=url):
                remote = self.remote()
                remote["urls"][0]["url"] = url
                self.reject(remote, "trusted download host", verify_downloads=True)

    def test_retry_transient_errors_then_succeed(self):
        self.outcomes = [URLError("temporary"), http_error(429), http_error(503), self.remote([])]
        self.assertTrue(self.prepare())
        self.assertEqual(self.delays, [1, 2, 4])
        self.assertEqual(len(self.requests), 4)

    def test_transport_errors_are_bounded_and_preserve_all_files(self):
        for failure in (URLError("offline"), TimeoutError("timeout"), ConnectionResetError("reset")):
            with self.subTest(failure=failure):
                self.delays.clear()
                self.requests.clear()
                self.reject([failure] * release.RETRIES, "bounded retries")
                self.assertEqual(self.delays, [1, 2, 4])
                self.assertEqual(len(self.requests), release.RETRIES)

    def test_truncated_http_read_is_retried_before_any_removal(self):
        class TruncatedResponse(io.BytesIO):
            def read(self, size):
                raise IncompleteRead(b"partial response", 100)

        outcomes = [TruncatedResponse(), io.BytesIO(json.dumps(self.remote()).encode())]

        def truncated_opener(request, timeout):
            return outcomes.pop(0)

        with contextlib.redirect_stdout(io.StringIO()):
            self.assertFalse(release.prepare(TAG, self.dist, opener=truncated_opener,
                                             sleeper=self.delays.append))
        self.assertEqual(self.delays, [1])
        self.assertEqual(list(self.dist.iterdir()), [])

    def test_auth_and_other_fatal_http_errors_do_not_retry(self):
        for code in (400, 401, 403, 409):
            with self.subTest(code=code):
                self.delays.clear()
                self.reject(http_error(code), f"HTTP {code}")
                self.assertEqual(self.delays, [])

    def test_malformed_json_and_response_schema_fail_closed(self):
        cases = [b"not JSON", [], {"info": {"name": "itofin", "version": VERSION}},
                 {"info": {"name": "itofin", "version": VERSION}, "urls": {}},
                 {"info": {"name": "other", "version": VERSION}, "urls": []},
                 {"info": {"name": "itofin", "version": "0.40.0"}, "urls": []}]
        for remote in cases:
            with self.subTest(remote=remote):
                data = remote if isinstance(remote, bytes) else json.dumps(remote).encode()
                self.reject(data)

    def test_invalid_remote_file_records_fail_closed(self):
        for field, value in (("filename", "other.whl"), ("yanked", "false"),
                             ("digests", {"sha256": "invalid"}), ("digests", {})):
            with self.subTest(field=field):
                remote = self.remote()
                remote["urls"][-1][field] = value
                self.reject(remote)
        remote = self.remote()
        remote["urls"].append(remote["urls"][0])
        self.reject(remote, "duplicate")

    def test_rebuilt_distribution_filename_conflict_fails_closed(self):
        remote = self.remote()
        remote["urls"][0]["filename"] = f"itofin-{VERSION}-cp310-abi3-macosx_12_0_arm64.whl"
        self.reject(remote, "different distribution filenames")

    def test_wrong_local_name_or_version_is_rejected_before_network(self):
        for name, version in (("other", VERSION), ("itofin", "0.40.0")):
            with self.subTest(name=name, version=version):
                write_wheel(self.dist, PLATFORMS[0], name=name, version=version)
                self.reject(http_error(404), "metadata")
                self.assertEqual(self.requests, [])
        write_wheel(self.dist, PLATFORMS[0])
        write_sdist(self.dist, version="0.40.0")
        self.reject(http_error(404), "metadata")

    def test_duplicate_wheel_metadata_is_rejected(self):
        write_wheel(self.dist, PLATFORMS[0], extra_metadata=True)
        self.reject(http_error(404), "one METADATA")

    def test_missing_required_platform_is_rejected(self):
        next(self.dist.glob("*aarch64*.whl")).unlink()
        self.reject(http_error(404), "exactly five")

    def test_duplicate_platform_is_rejected(self):
        next(self.dist.glob("*aarch64*.whl")).unlink()
        write_wheel(self.dist, "manylinux_2_28_x86_64")
        self.reject(http_error(404), "one wheel per required platform")

    def test_unsupported_or_mixed_platform_tags_are_rejected(self):
        path = next(self.dist.glob("*aarch64*.whl"))
        path.unlink()
        for tag in ("linux_aarch64", "win_amd64", "macosx_11_0_universal2",
                    "manylinux_2_17_aarch64.macosx_11_0_arm64"):
            with self.subTest(tag=tag):
                replacement = write_wheel(self.dist, tag)
                self.reject(http_error(404))
                replacement.unlink()

    def test_extra_files_and_symlinks_are_rejected(self):
        extra = self.dist / "extra.txt"
        extra.write_text("unexpected")
        self.reject(http_error(404), "exactly five")
        extra.unlink()
        wheel = next(self.dist.glob("*.whl"))
        saved = self.root / wheel.name
        wheel.rename(saved)
        wheel.symlink_to(saved)
        self.reject(http_error(404), "regular distribution")

    def test_changed_local_artifact_blocks_all_removals(self):
        remote = self.remote()
        original = self.opener

        def modifying_opener(request, timeout):
            response = original(request, timeout)
            next(self.dist.glob("*.whl")).write_bytes(b"changed during network call")
            return response

        self.outcomes = [remote]
        with self.assertRaisesRegex(ValueError, "changed during"):
            release.prepare(TAG, self.dist, opener=modifying_opener, sleeper=self.delays.append)
        self.assertEqual(len(list(self.dist.iterdir())), 5)

    def test_cli_writes_upload_required_without_uploading(self):
        output = self.root / "output"
        self.outcomes = [http_error(404)]
        with patch("sys.argv", ["prepare_release_pypi.py", TAG, str(self.dist)]), \
                patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}), \
                patch.object(release, "urlopen", self.opener), contextlib.redirect_stdout(io.StringIO()):
            release.main()
        self.assertEqual(output.read_text(), "upload_required=true\n")
        self.assertEqual(len(self.requests), 1)

    def test_cli_writes_false_when_every_file_exists(self):
        output = self.root / "output"
        self.outcomes = [self.remote()]
        with patch("sys.argv", ["prepare_release_pypi.py", TAG, str(self.dist)]), \
                patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}), \
                patch.object(release, "urlopen", self.opener), contextlib.redirect_stdout(io.StringIO()):
            release.main()
        self.assertEqual(output.read_text(), "upload_required=false\n")
        self.assertEqual(list(self.dist.iterdir()), [])

    def test_cli_failure_does_not_write_a_success_output(self):
        output = self.root / "output"
        before = self.snapshot()
        self.outcomes = [http_error(403)]
        with patch("sys.argv", ["prepare_release_pypi.py", TAG, str(self.dist)]), \
                patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}), \
                patch.object(release, "urlopen", self.opener), \
                contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            release.main()
        self.assertFalse(output.exists())
        self.assertEqual(before, self.snapshot())

    def test_wrong_release_tag_is_rejected(self):
        for tag in (VERSION, f"sdk/go/v{VERSION}", "v0.41.0-rc1"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.prepare(tag, self.dist, opener=self.opener)
        self.assertEqual(self.requests, [])


if __name__ == "__main__":
    unittest.main()
