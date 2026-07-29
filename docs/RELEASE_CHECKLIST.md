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
- Uninstall preserves `%APPDATA%\CodexPeek` and the bounded diagnostic log.

## Portable Verification

- Extract the ZIP to a writable directory and start the executable without setup.
- Confirm settings remain under `%APPDATA%\CodexPeek` rather than beside
  the executable.
- With only a populated `%APPDATA%\CodexUsageMonitor` legacy root present, start CodexPeek
  and confirm the complete directory moves to `%APPDATA%\CodexPeek`, the old path is gone,
  and existing managed profiles remain usable without another login.
- With both legacy and new roots present, confirm CodexPeek uses the new root and leaves the
  legacy root untouched instead of merging or deleting it.
- Run `codex-peek.exe --diagnose`.
- Compare both release files against `SHA256SUMS.txt`.
- On an unsigned build, confirm the README SmartScreen warning matches the observed
  Windows experience.

## Application Manual Checks

Before announcing a release, exercise every applicable scenario and record the Windows
version, scale, monitor/taskbar layout, result, and any item that could not be run.

### Windows shell and layout matrix

- [ ] Windows 10 x64 and Windows 11 x64
- [ ] 100%, 125%, 150%, and 200% display scaling, including a scale change while running
- [ ] Single monitor and multiple monitors with primary-monitor changes
- [ ] Taskbar auto-hide enabled and disabled
- [ ] Explorer restart while the taskbar widget is attached
- [ ] Successful taskbar attach, partial attach failure, and no eligible taskbar target;
      verify tray and floating-widget recovery remain usable
- [ ] Explorer/taskbar recovery preserves the selected usage profile and cached usage
- [ ] Windows autostart enable, verify, disable, and uninstall cleanup
- [ ] Tray icon cleanup on every normal exit path

### Usage-profile dialog and sign-in matrix

- [ ] Verify the tray profile submenu contains profile selection and exactly one Manage command,
      with no add command
- [ ] Verify `+` is below the manager list, is disabled at eight profiles and while a mutation is
      pending, and has localized tooltip and accessibility text
- [ ] Verify the add prompt: Enter/Add submits, Cancel/Escape/window `X` cancel, and invalid
      names remain rejected with a localized error
- [ ] Verify the manager has no bottom Close button and its window `X` and Escape close it
- [ ] Rename the system profile, restart, and verify the tray and widget show only the custom
      name while only the manager shows the default-account marker
- [ ] Verify system-profile logout and deletion remain unavailable
- [ ] Verify keyboard tab order, all 12 languages with long text, RTL layout, and 100/125/150/200%
      DPI scaling
- [ ] Open every profile dialog from the tray and complete all actions using keyboard only;
      verify focus order, Enter/Escape behavior, access keys, and focus restoration
- [ ] Verify English and an RTL language (Arabic), including mirrored layout, label clipping,
      confirmation text, and profile menu alignment
- [ ] At 100%, 125%, 150%, and 200% DPI, open the manager, add prompt, validation warning,
      login/delete confirmations, and safe profile errors; verify each message stays centered in
      the selected monitor work area without clipping or changing its existing buttons
- [ ] With negative-coordinate monitors and monitors arranged above and below the primary, open
      the complete profile flow from each monitor; verify a live visible owner wins and a hidden,
      closed, or zero owner falls back to the cursor monitor
- [ ] Move from manager to add prompt, warning, and confirmation on every monitor; verify centering
      continuity and that the add tooltip and every message remain fully visible
- [ ] Cancel and press Escape from the add prompt and every cancellable confirmation; verify the
      owner is restored, focus returns safely, and the manager can be reopened without duplicate
      commands or a stuck disabled window
- [ ] Repeat warning, confirmation, and error messages in Arabic; verify the exact localized RTL
      copy remains readable and visible with the existing button order and semantics
- [ ] Add, rename, select, sign in again, sign out, and delete a managed profile using a
      40-character Unicode label
- [ ] Add two profiles with intentionally distinct ChatGPT accounts and verify their usage
      independently
- [ ] Accidentally sign two differently labelled profiles into the same ChatGPT account;
      verify CodexPeek makes no identity claim and the UI remains safe and understandable
- [ ] With a different ChatGPT account already active in the browser, start profile login and
      verify the account-confirmation notice appears before the browser opens
- [ ] Complete browser login and verify the new profile is polled and selected
- [ ] Cancel browser login, retry it, and retry after an offline failure; verify the profile
      remains in **Login required** state until login succeeds or the user deletes it
- [ ] Sign out while online, then attempt sign-out while offline; verify failures do not claim
      success and other profiles remain usable
- [ ] Fill the catalog to eight total profiles, including the system profile, and verify a
      ninth is rejected without creating a directory or changing selection
- [ ] Verify selection is manual: low remaining usage, reset times, and failures never select
      or rotate to another profile automatically

### Polling, isolation, deletion, and recovery

- [ ] Make one managed profile time out, return a request error, and require login; verify each
      other profile retains independent cached usage, backoff, and refresh behavior
- [ ] Switch profiles with and without cached data; verify cached values render immediately and
      an uncached profile shows loading or login-required state without clearing other profiles
- [ ] Run manual refresh repeatedly and verify the global 10-second cooldown and selected-profile
      priority while all app-server operations remain serialized
- [ ] Before and after profile add/select/login/logout/delete, compare `codex login status` in a
      new terminal and the active account in VS Code/Codex IDE integration; both must be unchanged
- [ ] Repeat the unchanged-sign-in check for the Codex app, WSL, Remote SSH, and Dev Containers
      where available
- [ ] Confirm the profile child receives isolated behavior without changing Windows user/system
      `CODEX_HOME`, `PATH`, the default Codex home, or CLI/IDE settings
- [ ] Reject deletion at the confirmation prompt and verify all local profile data remains
- [ ] Force settings-save failure after the managed directory is staged for deletion; verify
      rollback restores the original directory and profile entry
- [ ] Force final directory cleanup failure after settings save; restart the app and verify only
      the validated deletion tombstone is cleaned without exposing its path in diagnostics
- [ ] Exercise startup with an interrupted add, a staged delete, an invalid/reparse-point managed
      path, and no attach target; verify safe recovery and no out-of-root file operation
- [ ] Exit during browser login and during an RPC timeout; verify no `codex app-server` child
      process tree remains
- [ ] Run `codex-peek.exe --diagnose` with mixed profile results; verify aggregate counts only and
      no labels, internal IDs, managed paths, email addresses, account IDs, or auth contents

### Existing application and distribution behavior

- [ ] Missing, unsupported, and logged-out Codex CLI
- [ ] Automatic release-metadata check and user-initiated release-page opening
- [ ] Installer and portable upgrade preserve settings and recover valid profile state

Record any check that could not be completed. Fix failures in a new patch version
instead of silently replacing a published release asset.
