# Task 6 report: final integration review fixes

## Implemented

- Added `SettingsStore::inspect_validity`, a non-mutating validity inspection that treats a
  missing file as valid defaults, accepts valid settings, reports malformed JSON, unsupported
  schemas, and invalid fields as invalid, and propagates file-read errors.
- Added `inspect_settings_for_diagnostics` and switched `run_safe_diagnostics` to it. Invalid
  settings now produce `settings_valid=false` and a safe `valid=false` diagnostic event without
  renaming, replacing, or rewriting the settings file.
- Added a thread-safe `UpdatePresentation` coordinator that owns check execution, automatic/user
  intent, presentation status, validated update data, and browser-open requests under one mutex.
  Automatic results expose update availability without creating an open request; user-initiated
  results create one consumable request.
- Removed `open_validated_tag_page` from the update worker. The worker only records the
  checker-validated `AvailableUpdate`. The Win32 UI thread consumes a one-shot request during its
  normal settings/snapshot timer flow, or immediately opens an already stored validated result
  after an explicit update-menu action. The native opener still revalidates the exact GitHub tag
  URL before `ShellExecuteW`.
- Extended `UiSettings` with the minimal `UpdatePresentationStatus` view state. The widget and
  tray append localized Checking/Available/Current/Failed update text to the existing
  fetching/error/stale usage status, and the tray menu labels every state accordingly.
- Replaced the separate atomic in-flight flag with `begin_check`, which atomically returns
  Started/AlreadyRunning and promotes a user click that overlaps an automatic check. The explicit
  menu path uses an atomic `begin_user_action`, so result availability cannot race with starting a
  redundant forced worker.
- Corrected README and SECURITY data-handling statements: bounded raw RPC is transiently parsed
  but never retained, copied to durable storage, persisted, or logged; only required typed fields
  are deserialized. The documentation also states that automatic update workers never open a
  browser.

## TDD evidence

- RED: the first settings inspection test failed because `SettingsStore::inspect_validity` did
  not exist. GREEN: the missing-file/default behavior passed after adding the read-only API.
- Added coverage for valid, malformed, unsupported-schema, and invalid-field settings, exact byte
  and path preservation, absence of corrupt backups, and read-error propagation.
- RED: the diagnostics integration test failed because the non-mutating diagnostic helper did not
  exist. GREEN: it now reports/logs `valid=false` while preserving corrupt settings unchanged.
- RED: update presentation imports and methods did not exist. GREEN: automatic results became
  visible without an open request.
- RED: the user-initiated one-shot test returned no request. GREEN: the exact stored result is
  returned once and only once.
- RED: the explicit-action decision API did not exist. GREEN: an explicit action atomically
  returns StartCheck/WaitForRunning or only the exact stored checker result, with no
  caller-provided URL input.
- RED: user Current/Failed presentation states and the mutex-owned begin coordinator did not
  exist. GREEN: `Ok(None)` and network failure remain visible instead of silently disappearing.
- Added deterministic automatic/user overlap coverage for Available, Current, and Failed outcomes,
  repeated begin calls, duplicate completion rejection, and exactly one UI-thread open request.
- RED: Checking/Failed localization variants and the five-state tray-menu label mapper did not
  exist. GREEN: Korean/English completeness and every menu state are now verified.
- RED: the combined status helper did not exist. GREEN: update failure text is appended without
  hiding the existing usage request error.

## Verification evidence

All commands ran in `C:\Users\user\Documents\codexbar widgets` with
`C:\Users\user\.cargo\bin` prepended to PATH.

- `cargo fmt --all -- --check`: exit 0.
- `cargo test --all-targets`: exit 0; 117 passed, 0 failed across all test binaries.
- `cargo clippy --all-targets --all-features -- -D warnings`: exit 0, no warnings.
- `cargo build --release`: exit 0.
- `git diff --check`: exit 0 (only Git's configured LF-to-CRLF notices were emitted).
- Release EXE VersionInfo:
  - FileDescription: `Codex Usage Monitor`
  - ProductName: `Codex Usage Monitor`
  - ProductVersion: `0.1.0`
  - FileVersion: `0.1.0`
  - OriginalFilename: `codex-usage-monitor.exe`
- `target\release\codex-usage-monitor.exe --diagnose`: process exit 0.

## Scope and remaining manual QA

No unrelated feature or packaging changes were made. The interactive Windows/DPI/Explorer matrix
and exact update-page opening after official GitHub repository metadata is configured remain the
manual release checks documented in `docs/RELEASE_CHECKLIST.md`.
