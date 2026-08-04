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

### User-facing release note

This release fixes two Windows integration failures: update checks now use the configured native
TLS provider instead of terminating the app, and browser sign-in initializes COM before opening
the authorization page, including when the default account is logged out.

It also includes the optional local usage-exhaustion estimate. When enabled (the default),
CodexPeek keeps only successful usage percentages and reset/observation timestamps for each
internal profile and rate-limit window in `%APPDATA%\CodexPeek\usage-history.json`. The estimate
is rounded and is not a guarantee of OpenAI's limit policy; it is never uploaded or synchronized.
Users can disable forecasting or clear all history from the tray, and deleting a managed profile
removes its history. Installer and Portable uninstall preserve `%APPDATA%\CodexPeek`, so release
notes and support responses must explain that history can remain until the user clears it or
manually deletes the file.

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
- Uninstall preserves `%APPDATA%\CodexPeek` (including `usage-history.json`) and the bounded
  diagnostic log; verify that this retention is called out in the user-facing release notes.

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

### Usage forecast and local-history matrix

- [ ] On a new profile, verify the usage detail starts at **Collecting forecast data** and shows a
      forecast only after at least three successful observations from the same profile, window,
      and reset cycle; the taskbar surface remains compact and the forecast appears only in its
      tooltip/details.
- [ ] Restart the app after enough samples have been collected and verify the estimate is restored
      from `%APPDATA%\CodexPeek\usage-history.json` without requiring a new login.
- [ ] Cross a real reset boundary and separately change `resets_at`; verify samples from the old
      cycle are not mixed into the new forecast. Verify a missing reset time omits only the
      before-reset comparison, not the exhaustion estimate.
- [ ] Stop successful polling long enough to make history stale and verify the UI marks or hides
      the forecast instead of presenting an old estimate as current. A failed poll must not append
      a sample or erase the last normal usage display.
- [ ] Toggle **Usage forecasting** off and verify recording and display stop; toggle it on again
      and verify collection restarts. Use **Clear usage forecast history**, confirm the prompt,
      and verify every profile returns to collection. Delete one managed profile and verify only
      that profile's history is removed.
- [ ] Inspect the local history file after representative polls: it contains only the internal
      profile ID, `Primary`/`Secondary`, usage percent, optional reset timestamp, and successful
      observation timestamp. Verify no email, account ID, display name, profile root, token,
      auth-file content, prompt, proxy setting, or raw RPC payload is present.
- [ ] Verify the 30-day and 1,000-samples-per-profile/window bounds, duplicate/minimum-interval
      write suppression, and atomic replacement. Replace the file with empty, malformed, or an
      unsupported-schema JSON document; verify it is quarantined/reset and usage display still
      works.
- [ ] Repeat forecast rendering in all 12 supported locales, including Arabic RTL and long
      strings, at 100%, 125%, 150%, and 200% DPI. Check the details/tooltip has no clipping and
      remains readable after Explorer restart, on a single monitor, and on multiple monitors.
- [ ] After three samples spanning at least 30 minutes with flat or low activity, verify a green
      dot and the localized **Comfortable** pace copy appear even when no exhaustion forecast is
      shown.
- [ ] Verify the pace-to-safe-rate ratios at exactly 0.5 and 1.0 switch the dot to amber and red,
      respectively.
- [ ] With current usage at 95% and a comfortable pace, verify the progress bar stays red while
      the upper-left dot stays green.
- [ ] Verify dot precedence: loading is gray, stale history or a missing reset time is gray, and a
      refresh error uses the red `!` even when a pace was previously available.
- [ ] Disable **Usage forecasting** and verify the dot is gray and the tooltip says the pace
      display is off.
- [ ] Repeat pace rendering in all 12 locales, light and dark themes, 100%, 125%, 150%, and 200%
      DPI, the minimal layout, and after an Explorer restart.

### Usage-profile dialog and sign-in matrix

- [ ] 사용량 프로필 관리/추가 창이 Windows 10/11 밝은·어두운 모드에서 같은 Native Refined 토큰을 사용한다.
- [ ] 100/125/150/175/200% DPI에서 목록, 입력 필드, `+`, 작업 버튼이 겹치거나 잘리지 않는다.
- [ ] 12개 지원 언어에서 모든 버튼 문구가 버튼 내부 한 줄로 표시된다.
- [ ] 좁은 작업 영역에서 작업 버튼 전체가 두 행으로 이동하며 버튼 내부 문구는 줄바꿈되지 않는다.
- [ ] 아랍어에서 목록 선택선, 텍스트, 진행률, 작업 버튼 정렬이 RTL로 반전된다.
- [ ] 키보드 Tab/Shift+Tab, 방향키, Enter, Escape 및 포커스 표시가 네이티브 규칙대로 동작한다.
- [ ] 정상/주의/위험/로딩/로그인 필요/오류 상태가 색상만 의존하지 않고 구분된다.
- [ ] 다중 모니터, 작업표시줄 자동 숨김, Explorer 재시작 후에도 두 창이 올바른 화면 중앙에 열린다.
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
- [ ] With the default system profile logged out, start login from the tray and verify the
      authorization page opens in the browser, authentication completes, and polling resumes
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
- [ ] After the tray menu closes, a user-initiated update check shows exactly one owned dialog for
      current, failed, and available-release results; an available release opens only after an
      explicit confirmation and a browser failure shows the localized recovery message
- [ ] Installer and portable upgrade preserve settings and recover valid profile state

Record any check that could not be completed. Fix failures in a new patch version
instead of silently replacing a published release asset.
