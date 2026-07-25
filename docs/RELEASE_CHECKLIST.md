# Release Guide

This guide describes how to publish Codex Usage Monitor for Windows x64.
The GitHub Actions release workflow runs only when a `v*` tag is pushed.

## Versioning

Use Semantic Versioning and treat `Cargo.toml` as the single source of truth.
The annotated Git tag must match the package version exactly, with a leading `v`.
For example, package version `0.1.3` requires tag `v0.1.3`.

Never move a tag that has been pushed or replace files in a published release.
If a published build needs correction, increment the patch version.

## Release Procedure

Replace `0.1.3` below with the version being released.

1. Update `Cargo.toml` and the root package version in `Cargo.lock`.
2. Run the automated checks from the repository root:

   ```powershell
   pwsh -NoProfile -File tests/release_packaging.ps1
   cargo fmt --all -- --check
   cargo test --all-targets
   cargo clippy --all-targets --all-features -- -D warnings
   cargo build --release
   git diff --check
   git status --short
   ```

3. Complete the applicable manual Windows checks below.
4. Commit the release preparation using the repository's commit convention:

   ```powershell
   git add --all
   git commit -m "build: Prepare v0.1.3 release"
   git push origin main
   ```

5. Create and push the matching annotated tag:

   ```powershell
   git tag -a v0.1.3 -m "Release v0.1.3"
   git push origin v0.1.3
   ```

## Release Workflow Contract

The build job runs on `windows-2022`, whose image supplies Inno Setup 6. It verifies
the tag and official repository metadata, runs formatting/tests/Clippy, builds the
release executable, and creates exactly these files:

```text
CodexPeek-Setup-v<version>-x64.exe
SHA256SUMS.txt
codex-peek-v<version>-windows-x86_64-portable.zip
```

The portable ZIP contains:

```text
codex-peek.exe
LICENSE
README.ko.md
README.md
SECURITY.md
THIRD_PARTY_NOTICES.md
```

The workflow verifies both SHA-256 entries, silently installs and removes the
installer on its isolated runner, and then creates a new GitHub Release. It fails
instead of overwriting an existing release or asset.

## Installer Verification

Confirm all of the following on a clean current-user profile:

- Installation succeeds without an administrator prompt.
- The default directory is
  `%LOCALAPPDATA%\Programs\CodexUsageMonitor`.
- A Start Menu shortcut is created and no desktop shortcut is created.
- Interactive setup offers to launch the monitor after installation.
- Windows startup remains disabled until enabled from the tray menu.
- Apps & Features reports the expected version.
- Uninstall removes the executable, Start Menu shortcut, uninstall entry, and
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexUsageMonitor`.
- Uninstall preserves `%APPDATA%\CodexUsageMonitor` and the bounded diagnostic log.

## Portable Verification

- Extract the ZIP to a writable directory and start the executable without setup.
- Confirm settings remain under `%APPDATA%\CodexUsageMonitor` rather than beside
  the executable.
- Run `codex-peek.exe --diagnose`.
- Compare both release files against `SHA256SUMS.txt`.
- On an unsigned build, confirm the README SmartScreen warning matches the observed
  Windows experience.

## Application Manual Checks

Before announcing a release, exercise the applicable scenarios:

- Windows 10 and Windows 11 x64
- 100%, 125%, 150%, and 200% display scaling
- Multiple monitors, taskbar auto-hide, and Explorer restart
- Missing, unsupported, or logged-out Codex CLI
- Windows autostart enable, verify, disable, and uninstall cleanup
- Tray icon cleanup on every normal exit path
- Automatic release-metadata check and user-initiated release-page opening

Record any check that could not be completed. Fix failures in a new patch version
instead of silently replacing a published release asset.
