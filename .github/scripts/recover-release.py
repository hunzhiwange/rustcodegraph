"""Validate and reuse cargo-dist artifacts from a completed tag-push Release run."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_run():
    run_id = os.environ["SOURCE_RUN_ID"]
    require(re.fullmatch(r"[0-9]+", run_id), "Source run ID must contain digits only")
    repository = os.environ["GITHUB_REPOSITORY"]
    run = api(f"repos/{repository}/actions/runs/{run_id}")
    require(run["path"] == ".github/workflows/release.yml", "Source must be a Release run")
    require(run["event"] == "push", "Source must be an original tag-push run")
    require(run["status"] == "completed", "Source run must have completed")
    tag = run["head_branch"]
    require(re.fullmatch(r"v?[0-9]+\.[0-9]+\.[0-9]+[\w.+-]*", tag), "Source must use a version tag")
    target = api(f"repos/{repository}/git/ref/tags/{tag}")["object"]
    for _ in range(10):
        if target["type"] != "tag":
            break
        target = api(f"repos/{repository}/git/tags/{target['sha']}")["object"]
    require(target["type"] == "commit" and target["sha"] == run["head_sha"],
            "Version tag no longer points to the original build commit")
    Path("recovery-source.json").write_text(json.dumps({"tag": tag, "sha": run["head_sha"]}))
    print(f"Reusing {tag} artifacts from run {run_id}, commit {run['head_sha']}")


def restore_plan():
    source = json.loads(Path("recovery-source.json").read_text())
    root = Path("recovery-artifacts")
    plan = json.loads((root / "plan-dist-manifest.json").read_text())
    host = json.loads((root / "dist-manifest.json").read_text())
    for manifest in (plan, host):
        require(manifest["announcement_tag"] == source["tag"], "Manifest tag differs from source run")
    require(plan["announcement_changelog"].strip(), "Release notes are missing")
    require(plan["releases"], "Release plan is empty")
    files = set()
    for release in plan["releases"]:
        hosting = release["hosting"]["github"]
        require(f"{hosting['owner']}/{hosting['repo']}" == os.environ["GITHUB_REPOSITORY"],
                "Release artifacts belong to another repository")
        files.update(release["artifacts"])
    require(files, "Release plan has no artifacts")
    for name in sorted(files):
        require(Path(name).name == name and name not in (".", ".."), "Invalid artifact filename")
        require((root / name).is_file(), f"Original artifact is missing: {name}")
        if name.endswith(".sha256"):
            original = root / name.removesuffix(".sha256")
            require(original.is_file(), f"Checksum has no artifact: {name}")
            expected = (root / name).read_text().split()[0].lower()
            actual = hashlib.sha256(original.read_bytes()).hexdigest()
            require(actual == expected, f"Artifact checksum mismatch: {original.name}")
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"tag={source['tag']}\n")
        output.write(f"manifest={json.dumps(plan, separators=(',', ':'))}\n")
    print(f"Validated {len(files)} original release files; no build required")


if __name__ == "__main__":
    {"validate-run": validate_run, "restore-plan": restore_plan}[sys.argv[1]]()
