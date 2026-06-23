# Distribution: native Rust bundles

RustCodeGraph ships a native Rust `rustcodegraph` binary. Standalone installers
and the npm installer expose that exact public command; compiled TypeScript
runtime files are no longer part of the published runtime. The npm package is a
cargo-dist installer package that downloads the matching Rust artifact from
GitHub Releases during install.

## What's In A Bundle

Built by cargo-dist in CI:

```
rustcodegraph-<triple>/
  rustcodegraph | rustcodegraph.exe
  README.md
  LICENSE
```

The GitHub Release workflow publishes cargo-dist's native Rust artifacts, such
as `rustcodegraph-x86_64-unknown-linux-gnu.tar.xz`. cargo-dist also generates
the Homebrew formula and the `rustcodegraph` npm installer package
from those artifacts.

## Install Channels

1. **`curl | sh`** ([`install.sh`](install.sh)) detects OS/arch, downloads the
   matching Rust archive from GitHub Releases, and links `rustcodegraph` onto PATH.
2. **npm** (`rustcodegraph`) installs a small launcher that
   downloads the matching native CLI from GitHub Releases.
3. **Homebrew** (`hunzhiwange/tap/rustcodegraph`) installs the formula generated
   by cargo-dist.
4. **Windows** ([`install.ps1`](install.ps1)) downloads the matching `.zip`,
   places `rustcodegraph.exe` under `current\bin`, and adds that directory to PATH.

## Release Pipeline

[`.github/workflows/release.yml`](.github/workflows/release.yml) lets cargo-dist
build and host GitHub Release artifacts. The workflow extracts GitHub Release
notes from `CHANGELOG.md` with the Rust `rustcodegraph extract-release-notes`
command, then publishes cargo-dist's Homebrew formula and npm installer package.
For a local packaging dry run, run:

```bash
dist build --artifacts=global
```

The workflow owns publishing; do not run `npm publish`, `git push`, or tag
creation manually from a local task.

Still TODO:
- Code signing for macOS Gatekeeper and Windows Authenticode.
- Scoop packages pointing at the Release archives.
