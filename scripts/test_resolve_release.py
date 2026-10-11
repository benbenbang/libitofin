"""Offline release selection, source identity and original-artifact contracts."""

import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import resolve_release as release


class SelectionTests(unittest.TestCase):
    def test_only_explicit_existing_or_new_release_is_selected(self):
        self.assertEqual(release.choose_tag("v0.41.0", "", ""), "v0.41.0")
        self.assertEqual(release.choose_tag("", "true", "v0.42.0"), "v0.42.0")
        self.assertEqual(release.choose_tag("v0.41.0", "true", "v0.42.0"), "v0.41.0")
        self.assertEqual(release.choose_tag("", "false", "v0.42.0"), "")
        for tag in ["main", "0.41.0", "sdk/go/v0.41.0", "v0.41.0-rc1", "v01.41.0", "v0.41.0\n"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.choose_tag(tag, "", "")

    def test_all_committed_local_versions_must_match_tag(self):
        manifest = '[workspace.package]\nversion="0.41.0"\n'
        lock = '[[package]]\nname="a"\nversion="0.41.0"\n[[package]]\nname="external"\nversion="1.0.0"\nsource="registry"\n'
        release.validate_documents("v0.41.0", manifest, lock)
        for changed_manifest, changed_lock in [(manifest.replace("0.41.0", "0.40.0"), lock), (manifest, lock.replace('version="0.41.0"', 'version="0.40.0"')), (manifest, 'package=[]')]:
            with self.subTest(lock=changed_lock), self.assertRaises(ValueError):
                release.validate_documents("v0.41.0", changed_manifest, changed_lock)

    def test_resume_dry_run_validates_without_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            env = {"REQUESTED_TAG": "v0.41.0", "DRY_RUN": "true", "GITHUB_REPOSITORY": "owner/repo", "GITHUB_OUTPUT": str(output)}
            with patch.dict(os.environ, env, clear=True), patch.object(release, "resolve", return_value="a" * 40) as resolve:
                release.main()
                resolve.assert_called_once_with("v0.41.0", "owner/repo")
            self.assertIn("publish=false", output.read_text())
            self.assertIn("revision=" + "a" * 40, output.read_text())

    def test_no_new_release_does_not_guess_latest_or_publish(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            with patch.dict(os.environ, {"NEW_RELEASE": "false", "GITHUB_OUTPUT": str(output)}, clear=True), patch.object(release, "resolve") as resolve:
                release.main()
                resolve.assert_not_called()
            self.assertIn("publish=false", output.read_text())

    def test_new_release_preview_does_not_require_uncreated_tag(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            env = {"NEW_RELEASE": "true", "NEW_TAG": "v0.42.0", "DRY_RUN": "true", "GITHUB_OUTPUT": str(output)}
            with patch.dict(os.environ, env, clear=True), patch.object(release, "resolve") as resolve:
                release.main()
                resolve.assert_not_called()
            self.assertIn("publish=false", output.read_text())
            self.assertIn("tag=v0.42.0", output.read_text())


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.run = {"id": 7, "repository": {"full_name": "owner/repo"}, "path": ".github/workflows/semantic-release.yml", "event": "workflow_dispatch", "status": "completed", "head_sha": "parent"}
        self.jobs = [{"name": f"build itofin wheels / build itofin wheel ({leg})", "conclusion": "success"} for leg in release.LEGS]
        self.artifacts = [{"name": "wheels-" + leg, "expired": False} for leg in release.LEGS]

    def verify(self, run=None, jobs=None, artifacts=None, changed=None, run_id="7"):
        release.verify_artifacts(run_id, "owner/repo", "revision", "parent", changed if changed is not None else ["Cargo.toml", "Cargo.lock"], run or self.run, jobs if jobs is not None else self.jobs, artifacts if artifacts is not None else self.artifacts)

    def test_metadata_only_parent_and_exact_release_revision_are_valid(self):
        self.verify()
        self.verify(run={**self.run, "head_sha": "revision"})
        self.verify(jobs=[{**job, "name": job["name"].removeprefix("build itofin wheels / ")} for job in self.jobs])

    def test_foreign_repo_workflow_event_status_or_revision_fails(self):
        for key, value in [("repository", {"full_name": "other/repo"}), ("id", 8), ("path", ".github/workflows/other.yml"), ("event", "pull_request"), ("status", "in_progress"), ("head_sha", "unrelated")]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.verify(run={**self.run, key: value})
        for changed in [[], ["src/lib.rs"], ["Cargo.toml", "src/lib.rs"]]:
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                self.verify(changed=changed)
        for run_id in ["0", "-1", "1/../2", "7\n", "word"]:
            with self.subTest(run_id=run_id), self.assertRaises(ValueError):
                self.verify(run_id=run_id)

    def test_each_required_job_and_artifact_must_exist_once_and_succeed(self):
        for index in range(4):
            for conclusion in ["failure", "cancelled", "skipped", None]:
                jobs = copy.deepcopy(self.jobs)
                jobs[index]["conclusion"] = conclusion
                with self.subTest(index=index, conclusion=conclusion), self.assertRaises(ValueError):
                    self.verify(jobs=jobs)
            artifacts = copy.deepcopy(self.artifacts)
            artifacts[index]["expired"] = True
            with self.assertRaises(ValueError):
                self.verify(artifacts=artifacts)
        for jobs, artifacts in [(self.jobs[:-1], self.artifacts), (self.jobs + [self.jobs[0]], self.artifacts), (self.jobs, self.artifacts[:-1]), (self.jobs, self.artifacts + [self.artifacts[0]])]:
            with self.assertRaises(ValueError):
                self.verify(jobs=jobs, artifacts=artifacts)


class ResolutionTests(unittest.TestCase):
    def command(self, *args):
        if args[:2] == ("git", "ls-remote"):
            return "tag-object\trefs/tags/v0.41.0\nrevision\trefs/tags/v0.41.0^{}"
        if args[:2] == ("git", "rev-parse"):
            return "revision"
        if args[:2] == ("gh", "release"):
            return json.dumps(self.metadata)
        if args[-1].endswith(":Cargo.toml"):
            return '[workspace.package]\nversion="0.41.0"'
        if args[-1].endswith(":Cargo.lock"):
            return '[[package]]\nname="local"\nversion="0.41.0"'
        self.fail(args)

    def setUp(self):
        self.metadata = {"tagName": "v0.41.0", "isDraft": False, "isPrerelease": False}

    def test_remote_annotated_tag_peel_and_source_documents_checked(self):
        with patch.object(release, "command", side_effect=self.command), patch.object(release.subprocess, "run") as fetch:
            self.assertEqual(release.resolve("v0.41.0", "owner/repo"), "revision")
            fetch.assert_called_once_with(["git", "fetch", "--no-tags", "origin", "refs/tags/v0.41.0:refs/tags/v0.41.0"], check=True)

    def test_draft_prerelease_wrong_tag_or_moved_local_tag_rejected(self):
        for key, value in [("tagName", "v0.40.0"), ("isDraft", True), ("isPrerelease", True)]:
            self.metadata = {"tagName": "v0.41.0", "isDraft": False, "isPrerelease": False, key: value}
            with self.subTest(key=key), patch.object(release, "command", side_effect=self.command), patch.object(release.subprocess, "run"), self.assertRaises(ValueError):
                release.resolve("v0.41.0", "owner/repo")
        with patch.object(release, "command", side_effect=["tag\trefs/tags/v0.41.0", "different"]), patch.object(release.subprocess, "run"), self.assertRaises(ValueError):
            release.resolve("v0.41.0", "owner/repo")


if __name__ == "__main__":
    unittest.main()
