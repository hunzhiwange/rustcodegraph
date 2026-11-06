# Resume publishing an existing release build

When a tag-push Release run finishes building its artifacts but fails during
publishing, use the manual recovery mode in the same Release workflow. The
workflow change must first be committed and pushed to the default branch.

1. Open **Actions → Release → Run workflow** and select `main`.
2. Set `source_run_id` to the original tag-push Release run ID.
3. Start the workflow and monitor the publish jobs.

For the failed `v2.0.0` build, use run ID `42068198792`. With GitHub CLI:

```bash
gh workflow run release.yml --repo hunzhiwange/rustcodegraph --ref main \
  -f source_run_id=42068198792
```

Recovery validates the original run and existing tag, downloads its unexpired
artifacts, checks that every planned release file exists, and verifies SHA-256
sidecars. It reuses the original release notes, platform archives, installers,
npm package, and Homebrew formula. Platform and global artifact build jobs are
skipped. GitHub Release, Homebrew, npm, and crates.io publishing then use the
existing publish jobs and credentials. The crates.io job checks out the release
tag and retains its normal `cargo publish` validation, which may compile code.

Recovery requires the original plan and host manifests and all their release
files to remain available. It stops if the tag has moved, files are missing, or
checksums differ. It does not overwrite an existing GitHub Release; if publishing
fails after the release is created, rerun only the failed publish jobs in that
recovery run.

Rerunning the original failed run uses its original commit and workflow, so it
does not pick up the release command fix committed afterward.

Local validation for the recovery helper:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s .github/scripts -p 'test_recover_release.py' -v
```
