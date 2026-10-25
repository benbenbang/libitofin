"""Static contracts for ordered, tag-bound Rust trusted publishing."""

from pathlib import Path
import re
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/semantic-release.yml"
GATE = "${{ !cancelled() && needs.resolve-release.result == 'success' && needs.publish-optimizer.result == 'success' && needs.resolve-release.outputs.publish == 'true' && (github.event_name == 'schedule' || inputs.publish_targets == 'all' || inputs.publish_targets == 'rust') }}"
TAG = "${{ needs.resolve-release.outputs.tag }}"
REVISION = "${{ needs.resolve-release.outputs.revision }}"


def jobs(source):
    starts = list(re.finditer(r"^  ([a-z][a-z-]+):\n", source, re.MULTILINE))
    return {
        match[1]: source[match.end():starts[index + 1].start() if index + 1 < len(starts) else len(source)]
        for index, match in enumerate(starts)
    }


def field(block, name):
    match = re.search(rf"^    {re.escape(name)}: (.+)$", block, re.MULTILINE)
    return match[1] if match else None


class OptimizerReleaseTests(unittest.TestCase):
    def setUp(self):
        self.source = WORKFLOW.read_text()
        self.jobs = jobs(self.source)

    def test_optimizer_precedes_core_and_failure_blocks_core(self):
        self.assertEqual(field(self.jobs["publish-optimizer"], "needs"), "resolve-release")
        self.assertEqual(field(self.jobs["publish-crate"], "needs"),
                         "[resolve-release, publish-optimizer]")
        self.assertEqual(field(self.jobs["publish-crate"], "if"), GATE)
        self.assertNotIn("always()", self.jobs["publish-crate"])
        self.assertNotIn("continue-on-error", self.source)

    def test_publishing_requires_resolved_release_and_dry_runs_never_publish(self):
        for name in ("publish-optimizer", "publish-crate", "build-python-wheels", "publish-pypi", "publish-go"):
            block = self.jobs[name]
            self.assertIn("needs.resolve-release.outputs.publish == 'true'", field(block, "if"))
        self.assertEqual(field(self.jobs["semantic-release"], "if"),
                         "${{ (inputs.prompt == 'true' || (github.event_name == 'schedule' && vars.ENABLE_SCHEDULED_RELEASES == 'true')) && inputs.release_tag == '' }}")
        self.assertIn("DRY_RUN: ${{ inputs.dry_run }}", self.jobs["resolve-release"])
        self.assertIn("REQUESTED_TAG: ${{ inputs.release_tag }}", self.jobs["resolve-release"])
        self.assertIn("needs.semantic-release.result == 'success'", self.jobs["resolve-release"])
        self.assertIn("needs.semantic-release.result == 'skipped'", self.jobs["resolve-release"])
        self.assertIn("cancel-in-progress: false", self.source)

    def test_scheduled_releases_are_opt_in_and_publish_all_channels(self):
        self.assertIn("\n  schedule:\n    - cron: '0 0 * * 2,6'", self.source)
        enabled = "(inputs.prompt == 'true' || (github.event_name == 'schedule' && vars.ENABLE_SCHEDULED_RELEASES == 'true'))"
        for name in ("semantic-release", "resolve-release"):
            with self.subTest(job=name):
                self.assertIn(enabled, field(self.jobs[name], "if"))
        self.assertIn("dry_run: ${{ inputs.dry_run || false }}", self.jobs["semantic-release"])
        for name, target in (("publish-optimizer", "rust"), ("publish-crate", "rust"),
                             ("build-python-wheels", "python"), ("publish-pypi", "python"),
                             ("publish-go", "go")):
            with self.subTest(job=name):
                self.assertIn(
                    f"(github.event_name == 'schedule' || inputs.publish_targets == 'all' || inputs.publish_targets == '{target}')",
                    field(self.jobs[name], "if"),
                )

    def test_both_crates_use_new_tooling_but_immutable_released_source(self):
        for name in ("publish-optimizer", "publish-crate"):
            block = self.jobs[name]
            self.assertEqual(block.count("uses: actions/checkout@v4"), 2)
            self.assertIn(f"ref: {REVISION}", block)
            self.assertIn("path: release", block)
            self.assertIn(f"RELEASE_TAG: {TAG}", block)
            self.assertIn(f"RELEASE_REVISION: {REVISION}", block)
            self.assertIn("release/rust-toolchain.toml", block)
            self.assertIn("toolchain: ${{ steps.rust.outputs.toolchain }}", block)
            self.assertLess(block.index("Verify package"), block.index("rust-lang/crates-io-auth-action@v1"))

    def test_oidc_is_conditional_job_scoped_and_uses_temporary_token(self):
        for name in ("publish-optimizer", "publish-crate"):
            block = self.jobs[name]
            self.assertIn("      contents: read\n", block)
            self.assertIn("      id-token: write", block)
            self.assertIn("uses: rust-lang/crates-io-auth-action@v1\n        id: auth", block)
            self.assertIn("CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}", block)
            self.assertEqual(block.count("if: steps.package.outputs.upload_required == 'true'"), 2)
            self.assertIsNone(field(block, "environment"))
            self.assertNotIn("secrets.", block)
            self.assertNotIn("cargo login", block)
            self.assertIn("CARGO_HTTP_MULTIPLEXING: 'false'", block)
            self.assertIn("CARGO_NET_RETRY: '5'", block)
            self.assertLess(block.index("rust-lang/crates-io-auth-action@v1"), block.index("--root release --publish"))

    def test_only_intended_package_uses_verified_helper(self):
        helper = (ROOT / "scripts/publish_release_crate.py").read_text()
        self.assertIn('["cargo", "package", "-p", package, "--locked"]', helper)
        self.assertIn('["cargo", "publish", "-p", args.package, "--locked", "--registry", "crates-io"]', helper)
        for name, package in (("publish-optimizer", "itofin-optimize"), ("publish-crate", "libitofin")):
            block = self.jobs[name]
            self.assertEqual(block.count(f'python3 scripts/publish_release_crate.py {package} "$RELEASE_TAG" "$RELEASE_REVISION" --root release'), 2)
            for unsafe in ("--no-verify", "--allow-dirty", "--workspace", "|| true"):
                self.assertNotIn(unsafe, block + helper)

    def test_python_and_go_are_independent_and_resume_artifacts_are_verified(self):
        for name in ("build-python-wheels", "publish-go"):
            self.assertEqual(field(self.jobs[name], "needs"), "resolve-release")
            self.assertIn(f"release_tag: {TAG}", self.jobs[name])
            self.assertIn(f"release_revision: {REVISION}", self.jobs[name])
        block = self.jobs["publish-pypi"]
        self.assertEqual(field(block, "needs"), "[resolve-release, build-python-wheels]")
        self.assertIn("needs.resolve-release.result == 'success'", block)
        self.assertIn("needs.build-python-wheels.result == 'success'", block)
        self.assertIn("inputs.python_artifact_run_id != ''", block)
        self.assertIn("run-id: ${{ inputs.python_artifact_run_id || github.run_id }}", block)
        self.assertIn('prepare_release_pypi.py "$RELEASE_TAG" dist --verify-downloads', block)
        self.assertIn("if: steps.package.outputs.upload_required == 'true'", block)
        self.assertNotIn("skip-existing:", block)
        self.assertIn("PYTHON_ARTIFACT_RUN_ID: ${{ inputs.python_artifact_run_id }}", self.jobs["resolve-release"])

    def test_recovery_overrides_skipped_ancestor_without_ignoring_failures(self):
        for name in ("publish-optimizer", "publish-crate", "build-python-wheels", "publish-pypi", "publish-go"):
            gate = field(self.jobs[name], "if")
            self.assertIn("!cancelled()", gate)
            self.assertIn("needs.resolve-release.result == 'success'", gate)
        self.assertIn("needs.publish-optimizer.result == 'success'", field(self.jobs["publish-crate"], "if"))

    def test_optimizer_bundles_unmodified_workspace_bsd_license(self):
        root_license = (ROOT / "LICENSE").read_bytes()
        crate = ROOT / "crates/itofin-optimize"
        self.assertEqual((crate / "LICENSE").read_bytes(), root_license)
        package = tomllib.loads((crate / "Cargo.toml").read_text())["package"]
        self.assertEqual(package["license"], "BSD-3-Clause")
        self.assertTrue(package["version"]["workspace"])
        self.assertNotIn("include", package)
        self.assertNotIn("exclude", package)
        self.assertNotEqual(package.get("publish"), False)

    def test_release_docs_record_verified_bootstrap_and_first_coordinated_ci(self):
        readme = (ROOT / "crates/itofin-optimize/README.md").read_text()
        for required in ("initial **0.36.0** publication", "configuration are verified",
                         "Owner `benbenbang`, repository `libitofin`, workflow `semantic-release.yml`",
                         "no GitHub environment", "`itofin-optimize` before `libitofin`",
                         "committed-version checks and OIDC", "CI dry runs perform no publication",
                         "first coordinated **v0.37.0** release", "verified optimizer-before-core OIDC",
                         "https://github.com/benbenbang/libitofin/actions/runs/37492675222",
                         "was closed by the maintainer"):
            with self.subTest(text=required):
                self.assertIn(required, readme)


if __name__ == "__main__":
    unittest.main()
