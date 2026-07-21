# Task 5 report: packaging, documentation, and Windows CI

## Implemented

- Added Cargo package description, MIT license expression, and README metadata. The
  `repository` field remains intentionally unset, so production update checks remain
  disabled without network I/O.
- Moved `winres` from runtime dependencies to Windows-only build dependencies. Cargo.lock
  did not change because the resolved `winres 0.1.12` package was already present and a
  dependency-kind move does not alter Cargo's lockfile graph.
- Added a Windows build script that embeds:
  - ProductName/FileDescription `Codex Usage Monitor`
  - InternalName `codex-usage-monitor`
  - OriginalFilename `codex-usage-monitor.exe`
  - FileVersion/ProductVersion `0.1.0`
  - Common Controls v6, `asInvoker`, PerMonitorV2, and long-path-aware manifest settings
  - an original deterministic 16/32/48-pixel ICO showing two usage meters
- Added pure resource-helper tests for deterministic ICO layout and Windows numeric version
  packing.
- Added bilingual Korean/English README coverage for operation, status thresholds,
  prerequisites, all tray actions, fallback behavior, polling/backoff/stale policy, paths,
  diagnostics, network/privacy boundaries, troubleshooting, limitations, tests, and manual
  QA disclaimer.
- Added the project MIT license, SECURITY.md, third-party MIT notice for Claude Code Usage
  Monitor and Craig Constable, and a release checklist that separates passed automation
  from pending manual QA.
- Added Windows push/PR CI with Rust 1.85.0 MSVC and a tag release workflow that validates
  the tag version, verifies the project, builds the release EXE, produces a versioned ZIP,
  uploads a workflow artifact, and attaches the ZIP to a GitHub release. The release job
  alone receives `contents: write`.

## Verification evidence

All commands were run in `C:\Users\user\Documents\codexbar widgets` with
`C:\Users\user\.cargo\bin` prepended to PATH.

- `cargo fmt --all -- --check`: exit 0.
- `cargo test --all-targets`: exit 0; 102 passed, 0 failed across all test binaries.
  The new `tests/build_resources.rs` target passed 2/2 tests.
- `cargo clippy --all-targets --all-features -- -D warnings`: exit 0, no warnings.
- `cargo build --release`: exit 0; optimized EXE produced at
  `target\release\codex-usage-monitor.exe`, size 472,064 bytes.
- PowerShell `VersionInfo` inspection of the release EXE:
  - FileDescription: `Codex Usage Monitor`
  - ProductName: `Codex Usage Monitor`
  - InternalName: `codex-usage-monitor`
  - OriginalFilename: `codex-usage-monitor.exe`
  - FileVersion: `0.1.0`
  - ProductVersion: `0.1.0`
- Win32 resource inspection with `LoadLibraryEx`/`FindResource`:
  - RT_MANIFEST ID 1: 1,159 bytes
  - RT_GROUP_ICON ID 1: 48 bytes; `ExtractAssociatedIcon` returned a valid 32x32 icon
  - RT_VERSION ID 1: 624 bytes
  - loaded manifest contained Common Controls 6.0.0.0, PerMonitorV2, `asInvoker`, and
    `longPathAware`.
- `target\release\codex-usage-monitor.exe --diagnose`: process exit 0.
- `git diff --check`: exit 0.

## Remaining manual QA

The following items remain deliberately unchecked in `docs/RELEASE_CHECKLIST.md`:

- Windows 10 and Windows 11 interactive runs
- 100/125/150/200% DPI
- multi-monitor and mixed-DPI movement
- Explorer restart and taskbar recovery
- auto-hide taskbar
- Codex CLI missing, logged-out, and proxy-present environments
- autostart registration/verification/removal
- exact update-page opening after an official GitHub repository is configured

No installer, self-updater, signing workflow, package-manager distribution, or automatic
executable replacement was added.
