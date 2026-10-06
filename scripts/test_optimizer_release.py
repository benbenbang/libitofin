"""Static contracts for ordered, tag-bound Rust trusted publishing."""

from pathlib import Path
import re
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/semantic-release.yml"
GATE = "${{ needs.semantic-release.outputs.new_release == 'true' && !inputs.dry_run }}"
TAG = "${{ needs.semantic-release.outputs.new_release_tag }}"


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
        self.assertEqual(field(self.jobs["publish-optimizer"], "needs"), "semantic-release")
        self.assertEqual(field(self.jobs["publish-crate"], "needs"),
                         "[semantic-release, publish-optimizer]")
        self.assertEqual(field(self.jobs["publish-crate"], "if"), GATE)
        self.assertNotIn("always()", self.jobs["publish-crate"])
        self.assertNotIn("continue-on-error", self.source)

    def test_publishing_requires_new_release_and_excludes_dry_runs(self):
        for name in ("publish-optimizer", "publish-crate", "build-python-wheels",
                     "publish-pypi", "publish-go"):
            with self.subTest(job=name):
                self.assertEqual(field(self.jobs[name], "if"), GATE)
        self.assertEqual(field(self.jobs["semantic-release"], "if"),
                         "${{ inputs.prompt == 'true' }}")

    def test_both_crates_checkout_and_validate_same_released_tag(self):
        for name in ("publish-optimizer", "publish-crate"):
            block = self.jobs[name]
            with self.subTest(job=name):
                self.assertIn("uses: actions/checkout@v4", block)
                self.assertIn(f"ref: {TAG}", block)
                self.assertIn(f"RELEASE_TAG: {TAG}", block)
                self.assertIn('run: python3 scripts/check_release_version.py "$RELEASE_TAG"', block)
                self.assertLess(block.index("check_release_version.py"),
                                block.index("rust-lang/crates-io-auth-action@v1"))

    def test_oidc_is_job_scoped_and_uses_temporary_token(self):
        for name in ("publish-optimizer", "publish-crate"):
            block = self.jobs[name]
            with self.subTest(job=name):
                self.assertIn("      contents: read\n", block)
                self.assertIn("      id-token: write", block)
                self.assertIn("uses: rust-lang/crates-io-auth-action@v1\n        id: auth", block)
                self.assertIn("CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}", block)
                self.assertIsNone(field(block, "environment"))
                self.assertNotIn("secrets.", block)
                self.assertNotIn("cargo login", block)
                self.assertLess(block.index("rust-lang/crates-io-auth-action@v1"),
                                block.index("run: cargo publish"))

    def test_only_intended_package_is_published_with_verification_and_lockfile(self):
        for name, package in (("publish-optimizer", "itofin-optimize"),
                              ("publish-crate", "libitofin")):
            block = self.jobs[name]
            with self.subTest(job=name):
                self.assertEqual(re.findall(r"^        run: cargo publish (.+)$", block, re.MULTILINE),
                                 [f"-p {package} --locked"])
                for unsafe in ("--no-verify", "--allow-dirty", "--workspace", "|| true"):
                    self.assertNotIn(unsafe, block)

    def test_python_and_go_do_not_depend_on_rust_publication(self):
        for name in ("build-python-wheels", "publish-go"):
            with self.subTest(job=name):
                self.assertEqual(field(self.jobs[name], "needs"), "semantic-release")
                self.assertIn(f"release_tag: {TAG}", self.jobs[name])
        self.assertEqual(field(self.jobs["publish-pypi"], "needs"),
                         "[semantic-release, build-python-wheels]")

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
