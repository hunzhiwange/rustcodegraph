import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("recover_release", Path(__file__).with_name("recover-release.py"))
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        cwd = Path.cwd()
        os.chdir(self.temp.name)
        self.addCleanup(os.chdir, cwd)
        self.env = patch.dict(os.environ, {
            "SOURCE_RUN_ID": "123", "GITHUB_REPOSITORY": "owner/repo",
            "GITHUB_OUTPUT": str(Path("output").absolute()),
        })
        self.env.start()
        self.addCleanup(self.env.stop)
        self.run = {"path": ".github/workflows/release.yml", "event": "push",
                    "status": "completed", "head_branch": "v2.0.0", "head_sha": "built-commit"}

    def test_annotated_tag_resolves_to_original_build(self):
        responses = [self.run, {"object": {"type": "tag", "sha": "annotation"}},
                     {"object": {"type": "commit", "sha": "built-commit"}}]
        with patch.object(recovery, "api", side_effect=responses):
            recovery.validate_run()
        self.assertEqual(json.loads(Path("recovery-source.json").read_text())["tag"], "v2.0.0")

    def test_moved_tag_is_rejected(self):
        with patch.object(recovery, "api", side_effect=[self.run, {"object": {"type": "commit", "sha": "other"}}]):
            with self.assertRaisesRegex(ValueError, "original build commit"):
                recovery.validate_run()

    def test_pull_request_artifacts_are_rejected(self):
        with patch.object(recovery, "api", return_value={**self.run, "event": "pull_request"}):
            with self.assertRaisesRegex(ValueError, "tag-push"):
                recovery.validate_run()

    def artifacts(self):
        Path("recovery-source.json").write_text(json.dumps({"tag": "v2.0.0", "sha": "built-commit"}))
        root = Path("recovery-artifacts")
        root.mkdir()
        payload = b"original prebuilt binary archive"
        (root / "binary.zip").write_bytes(payload)
        (root / "binary.zip.sha256").write_text(hashlib.sha256(payload).hexdigest())
        self.plan = {"announcement_tag": "v2.0.0", "announcement_changelog": "Original release notes",
                     "releases": [{"hosting": {"github": {"owner": "owner", "repo": "repo"}},
                                   "artifacts": ["binary.zip", "binary.zip.sha256"]}]}
        for name in ["plan-dist-manifest.json", "dist-manifest.json"]:
            (root / name).write_text(json.dumps(self.plan))
        return root

    def test_original_artifacts_and_plan_are_preserved(self):
        self.artifacts()
        recovery.restore_plan()
        outputs = dict(line.split("=", 1) for line in Path("output").read_text().splitlines())
        self.assertEqual(outputs["tag"], "v2.0.0")
        self.assertEqual(json.loads(outputs["manifest"]), self.plan)

    def test_missing_artifact_is_rejected(self):
        root = self.artifacts()
        (root / "binary.zip").unlink()
        with self.assertRaisesRegex(ValueError, "missing"):
            recovery.restore_plan()

    def test_corrupt_artifact_is_rejected(self):
        root = self.artifacts()
        (root / "binary.zip").write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            recovery.restore_plan()

    def test_manifest_for_other_version_is_rejected(self):
        root = self.artifacts()
        (root / "dist-manifest.json").write_text(json.dumps({**self.plan, "announcement_tag": "v1.0.0"}))
        with self.assertRaisesRegex(ValueError, "tag differs"):
            recovery.restore_plan()


if __name__ == "__main__":
    unittest.main()
