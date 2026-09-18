"""Failure and retry contracts for coordinated native asset publication."""

import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import publish_go_release as release


VERSION = "0.23.0"
REVISION = "a" * 40
TAG = f"v{VERSION}"
REPOSITORY = "example/libitofin"
PLATFORMS = ("linux-amd64", "darwin-arm64")


def archive_name(platform):
    return f"itofin-native-{VERSION}-{platform}.tar.gz"


def write_archive(directory, platform, revision=REVISION, build="local"):
    path = directory / archive_name(platform)
    manifest = f"version={VERSION}\nrevision={revision}\nplatform={platform}\nbuild={build}\n"
    data = manifest.encode()
    member = tarfile.TarInfo(path.name.removesuffix(".tar.gz") + "/VERSION")
    member.size = len(data)
    with tarfile.open(path, "w:gz") as archive:
        archive.addfile(member, io.BytesIO(data))
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    path.with_name(path.name + ".sha256").write_text(f"{digest}  {path.name}\n")
    return path


class PublicationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.module = self.root / "sdk/go/go.mod"
        self.module.parent.mkdir(parents=True)
        self.module.write_text(f"module github.com/{REPOSITORY}/sdk/go\n")
        self.local = self.root / "local"
        self.local.mkdir()
        for platform in PLATFORMS:
            write_archive(self.local, platform)
        self.assets = {}
        self.mutations = []
        self.references = {TAG: REVISION}
        self.draft = False
        self.reported_tag = TAG
        self.drop_uploads = False
        self.body = "Core release notes."
        for name, replacement in (
            ("check_version", lambda *_: VERSION),
            ("remote_commit", lambda _, tag: self.references.get(tag)),
            ("command", self.command),
            ("api", self.api),
        ):
            mock = patch.object(release, name, side_effect=replacement)
            mock.start()
            self.addCleanup(mock.stop)

    def command(self, *args):
        if args[0] == "git":
            self.assertEqual(args[-2:], ("rev-parse", "HEAD"))
            return REVISION
        self.assertEqual(args[:2], ("gh", "release"))
        action = args[2]
        if action == "download":
            name = args[args.index("--pattern") + 1]
            destination = Path(args[args.index("--dir") + 1])
            (destination / name).write_bytes(self.assets[name])
        elif action == "upload":
            path = Path(args[4])
            self.assertNotIn(path.name, self.assets)
            self.mutations.append(("upload", path.name))
            if not self.drop_uploads:
                self.assets[path.name] = path.read_bytes()
        elif action == "edit":
            notes = Path(args[args.index("--notes-file") + 1]).read_text()
            self.assertTrue(notes.startswith(self.body))
            self.assertIn("## Go bindings", notes)
            self.assertIn(f"github.com/{REPOSITORY}/sdk/go@v{VERSION}", notes)
            self.assertNotIn("/bindings/go@", notes)
            self.body = notes
            self.mutations.append(("edit", args[3]))
        else:
            self.fail(f"unexpected command: {args}")
        return ""

    def api(self, endpoint, *args):
        if endpoint.endswith("/git/refs"):
            self.assertIn("POST", args)
            expected = {archive_name(p) + suffix for p in PLATFORMS for suffix in ("", ".sha256")}
            self.assertTrue(expected.issubset(self.assets))
            self.assertIn(f"sha={REVISION}", args)
            tag = f"sdk/go/v{VERSION}"
            self.assertIn(f"ref=refs/tags/{tag}", args)
            self.references[tag] = REVISION
            self.mutations.append(("tag", tag))
            return {}
        self.assertEqual(endpoint, f"repos/{REPOSITORY}/releases/tags/{TAG}")
        return {"tag_name": self.reported_tag, "draft": self.draft, "body": self.body,
                "assets": [{"name": name} for name in self.assets]}

    def publish(self, tag=TAG):
        return release.publish(REPOSITORY, tag, self.local, self.root)

    def seed_remote(self, with_checksums=True):
        remote = self.root / "remote"
        remote.mkdir()
        for platform in PLATFORMS:
            path = write_archive(remote, platform, build="earlier-build")
            self.assets[path.name] = path.read_bytes()
            if with_checksums:
                self.assets[path.name + ".sha256"] = path.with_name(path.name + ".sha256").read_bytes()

    def test_corrupt_archive_and_checksum_are_rejected(self):
        path = self.local / archive_name(PLATFORMS[0])
        original = path.read_bytes()
        path.write_bytes(b"not a tar archive")
        with self.assertRaises(tarfile.ReadError):
            self.publish()
        self.assertEqual(self.mutations, [])
        path.write_bytes(original)
        path.with_name(path.name + ".sha256").write_text("incorrect checksum\n")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            self.publish()
        self.assertEqual(self.mutations, [])

    def test_wrong_archive_revision_is_rejected(self):
        write_archive(self.local, PLATFORMS[0], revision="b" * 40)
        with self.assertRaisesRegex(ValueError, "identity"):
            self.publish()
        self.assertEqual(self.mutations, [])

    def test_release_and_tag_mismatches_precede_mutations(self):
        cases = (("draft", True), ("reported_tag", "v0.21.0"),
                 ("references", {TAG: "b" * 40}),
                 ("references", {TAG: REVISION, f"sdk/go/v{VERSION}": "b" * 40}))
        for attribute, invalid in cases:
            with self.subTest(attribute=attribute, invalid=invalid):
                original = getattr(self, attribute)
                setattr(self, attribute, invalid)
                with self.assertRaises(ValueError):
                    self.publish()
                self.assertEqual(self.mutations, [])
                setattr(self, attribute, original)
        with self.assertRaisesRegex(ValueError, "v-prefixed"):
            self.publish(VERSION)
        self.assertEqual(self.mutations, [])

    def test_tag_created_only_after_both_platform_assets(self):
        result = self.publish()
        self.assertEqual([action for action, _ in self.mutations], ["upload"] * 4 + ["tag", "edit"])
        self.assertEqual(result["revision"], REVISION)
        self.assertEqual(result["go_tag"], f"sdk/go/v{VERSION}")
        self.assertEqual(set(result["sha256"]), {archive_name(p) for p in PLATFORMS})

    def test_published_legacy_tag_is_preserved(self):
        legacy_tag = "bindings/go/v0.22.0"
        self.references[legacy_tag] = "c" * 40
        self.publish()
        self.assertEqual(self.references[legacy_tag], "c" * 40)
        self.assertNotIn(f"bindings/go/v{VERSION}", self.references)
        self.assertEqual(self.references[f"sdk/go/v{VERSION}"], REVISION)

    def test_missing_or_legacy_module_is_rejected_before_mutations(self):
        self.module.write_text(f"module github.com/{REPOSITORY}/bindings/go\n")
        with self.assertRaisesRegex(ValueError, "sdk/go module"):
            self.publish()
        self.assertEqual(self.mutations, [])
        self.module.unlink()
        with self.assertRaisesRegex(ValueError, "sdk/go module"):
            self.publish()
        self.assertEqual(self.mutations, [])

    def test_server_missing_uploaded_assets_prevents_tag(self):
        self.drop_uploads = True
        with self.assertRaisesRegex(ValueError, "both native platforms"):
            self.publish()
        self.assertNotIn(f"sdk/go/v{VERSION}", self.references)

    def test_rerun_preserves_remote_assets_when_rebuilt_bytes_differ(self):
        self.seed_remote()
        original = dict(self.assets)
        for name in original:
            self.assertNotEqual(original[name], (self.local / name).read_bytes())
        self.references[f"sdk/go/v{VERSION}"] = REVISION
        result = self.publish()
        self.assertEqual(self.assets, original)
        self.assertEqual(self.mutations, [("edit", TAG)])
        for name, digest in result["sha256"].items():
            self.assertEqual(digest, hashlib.sha256(original[name]).hexdigest())
        self.mutations.clear()
        self.publish()
        self.assertEqual(self.mutations, [])

    def test_partial_upload_repairs_checksums_for_remote_bytes(self):
        self.seed_remote(with_checksums=False)
        original = dict(self.assets)
        self.publish()
        for name, content in original.items():
            self.assertEqual(self.assets[name], content)
            expected = f"{hashlib.sha256(content).hexdigest()}  {name}\n".encode()
            self.assertEqual(self.assets[name + ".sha256"], expected)
        uploads = [name for action, name in self.mutations if action == "upload"]
        self.assertEqual(uploads, [archive_name(p) + ".sha256" for p in PLATFORMS])

    def test_invalid_existing_remote_assets_prevent_tag(self):
        self.seed_remote()
        self.assets[archive_name(PLATFORMS[0]) + ".sha256"] = b"corrupt checksum"
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            self.publish()
        self.assertEqual(self.mutations, [])


class RemoteCommitTests(unittest.TestCase):
    def test_only_404_is_treated_as_missing(self):
        for message in ("HTTP 403", "HTTP 500", "connection refused"):
            with self.subTest(message=message), patch.object(release.subprocess, "run") as run:
                run.return_value = subprocess.CompletedProcess([], 1, "", message)
                with self.assertRaises(RuntimeError):
                    release.remote_commit(REPOSITORY, TAG)
        with patch.object(release.subprocess, "run") as run:
            run.return_value = subprocess.CompletedProcess([], 1, "", "gh: Not Found (HTTP 404)")
            self.assertIsNone(release.remote_commit(REPOSITORY, TAG))

    def test_annotated_tags_resolve_to_commit(self):
        result = {"object": {"type": "tag", "sha": "tag-sha"}}
        with patch.object(release.subprocess, "run") as run, patch.object(release, "api") as api:
            run.return_value = subprocess.CompletedProcess([], 0, json.dumps(result), "")
            api.return_value = {"object": {"type": "commit", "sha": REVISION}}
            self.assertEqual(release.remote_commit(REPOSITORY, "sdk/go/v0.23.0"), REVISION)
            self.assertIn("sdk%2Fgo%2Fv0.23.0", run.call_args.args[0][-1])


if __name__ == "__main__":
    unittest.main()
