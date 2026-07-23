# Release Guide

This guide describes how to version and publish Codex Usage Monitor.
The GitHub Actions release workflow runs only when a `v*` tag is pushed.

## Versioning

Use Semantic Versioning and treat `Cargo.toml` as the single source of truth.
The Git tag must match the package version exactly, with a leading `v`.

| Change | Example | Use it for |
| --- | --- | --- |
| Patch | `0.1.0` -> `0.1.1` | Bug fixes with no intended behavior change. |
| Minor | `0.1.0` -> `0.2.0` | Backward-compatible features or meaningful improvements. |
| Major | `0.9.0` -> `1.0.0` | A stable public release or a compatibility-breaking change. |

For example, this package version requires the `v0.1.1` tag:

```toml
# Cargo.toml
version = "0.1.1"
```

## Release Procedure

Replace `0.1.1` with the version being released.

1. Update the `version` field in `Cargo.toml`.
2. Run the local checks from the repository root:

   ```powershell
   cargo fmt --all -- --check
   cargo test --all-targets
   cargo clippy --all-targets --all-features -- -D warnings
   cargo build --release
   git diff --check
   git status --short
   ```

3. Commit the version change. Include `Cargo.lock` only if it changed.

   ```powershell
   git add Cargo.toml Cargo.lock
   git commit -m "release: v0.1.1"
   git push origin main
   ```

4. Create and push the matching annotated tag.

   ```powershell
   git tag -a v0.1.1 -m "Release v0.1.1"
   git push origin v0.1.1
   ```

## What the Release Workflow Does

After the tag is pushed, GitHub Actions verifies that the tag and `Cargo.toml`
version match. It then formats, tests, lints, builds, and packages the Windows
executable before creating or updating the GitHub Release.

The release asset is named like this:

```text
codex-usage-monitor-v0.1.1-windows-x86_64.zip
```

## Release Verification

After GitHub Actions completes, download the ZIP from the GitHub Release and
confirm that it contains the executable, `README.md`, `LICENSE`,
`THIRD_PARTY_NOTICES.md`, `SECURITY.md`, and this guide.

Before announcing a release, exercise the applicable manual checks:

- Windows 10 and Windows 11
- 100%, 125%, 150%, and 200% display scaling
- Multiple monitors, taskbar auto-hide, and Explorer restart
- Missing or logged-out Codex CLI
- Windows autostart enable, verify, and disable

If an automated or manual check fails, fix the issue and publish a new patch
version rather than replacing a published release asset silently.
