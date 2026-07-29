# Usage Profile Dialog Centering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Center every usage-profile management dialog on the intended monitor work area without changing existing profile actions or modal behavior.

**Architecture:** Add a pure work-area centering contract shared by native custom windows and a thread-local, RAII-scoped `WH_CBT` hook for existing profile `MessageBoxW` calls. Manager placement uses the cursor monitor; child dialogs and messages use a live owner monitor, with cursor and Windows-default fallbacks.

**Tech Stack:** Rust 2021, `windows` 0.61 Win32 APIs, existing native modal state machines, deterministic Rust tests.

## Global Constraints

- Keep Rust 1.85 compatibility and add no dependencies.
- Never read, parse, copy, log, or persist `%USERPROFILE%\.codex\auth.json` contents or sensitive account data.
- Do not change Codex CLI, IDE, or Codex app login/account state.
- Do not add long-running I/O to the UI thread.
- Preserve existing profile actions, confirmation copy/buttons, owner modality, reentrancy guards, `GWLP_USERDATA` cleanup, and UTF-16 limits.
- New or modified public/nontrivial contracts and unsafe invariants require useful Korean rustdoc or safety comments.
- Positioning failures must fall back to Windows default placement and must not block the dialog or profile operation.

---

### Task 1: Center native profile manager and add windows

**Files:**
- Modify: `src/windows/profile_dialog.rs`
- Modify: `src/windows/profile_dialog/platform.rs`
- Test: `src/windows/profile_dialog.rs`
- Test: `tests/windows_app.rs`

**Interfaces:**
- Produces: internal `DialogWorkArea`, `DialogWindowSize`, and `DialogOrigin` value types.
- Produces: `centered_dialog_origin(work_area, window_size) -> DialogOrigin`.
- Produces: native `DialogMonitorAnchor::{Cursor, Owner(HWND)}` and best-effort `center_window(dialog, anchor)`.
- Task 2 consumes the same pure geometry and monitor-selection helpers.

- [ ] **Step 1: Write failing pure geometry tests**

  Add tests with literal expected coordinates for a `1920x1040` work area, a negative-coordinate secondary monitor, odd sizes, and a window larger than its work area. Each test must fail because the geometry contract is absent.

- [ ] **Step 2: Run RED geometry tests**

  Run: `cargo test --lib windows::profile_dialog::tests::centered_dialog -- --nocapture`

  Expected: compilation/test failure caused by the missing centering contract.

- [ ] **Step 3: Implement the pure geometry contract**

  Calculate with `i64`, center independently on each axis, and clamp an oversized dimension to the work-area start without resizing the window. Convert the final in-range coordinates to `i32`.

- [ ] **Step 4: Run GREEN geometry tests**

  Run: `cargo test --lib windows::profile_dialog::tests::centered_dialog -- --nocapture`

  Expected: every geometry test passes with no warnings.

- [ ] **Step 5: Write failing placement-policy tests**

  Add production-consumed policy assertions proving manager=`Cursor`, add prompt=`Owner`, and a zero-sized/invalid owner falls back to the cursor policy.

- [ ] **Step 6: Run RED placement tests**

  Run: `cargo test --test windows_app profile_dialog_centering -- --nocapture`

  Expected: failure because the production placement policy is not implemented.

- [ ] **Step 7: Center both custom windows**

  Import `GetCursorPos`, `MonitorFromPoint`, `MonitorFromWindow`, `GetMonitorInfoW`, `GetWindowRect`, and `SetWindowPos`. Select `MONITOR_DEFAULTTONEAREST`; use `MONITOR_DEFAULTTOPRIMARY` only as final lookup fallback. After control setup and before owner disable/`ShowWindow`, move the manager to the cursor monitor and the add prompt to the live manager monitor. Re-query the outer size and perform at most one corrective move if a DPI transition changes it. Ignore positioning errors after preserving the original window.

- [ ] **Step 8: Run Task 1 tests and quality checks**

  Run:
  - `cargo test --lib windows::profile_dialog -- --nocapture`
  - `cargo test --test windows_app -- --nocapture`
  - `cargo fmt --all -- --check`
  - `git diff --check`

- [ ] **Step 9: Commit Task 1**

  Commit: `fix: Center usage profile windows`

---

### Task 2: Center profile confirmations, warnings, and errors

**Files:**
- Modify: `src/windows/profile_dialog.rs`
- Modify: `src/windows/profile_dialog/platform.rs`
- Modify: `src/windows/native/platform.rs`
- Modify: `docs/RELEASE_CHECKLIST.md`
- Test: `src/windows/profile_dialog.rs`
- Test: `tests/windows_app.rs`

**Interfaces:**
- Consumes: Task 1 geometry and monitor-selection helpers.
- Produces: `show_centered_profile_message(owner, message, title, style)` as the only profile `MessageBoxW` boundary.
- Produces: a thread-local centering request and `CenteredMessageBoxHookGuard` whose `Drop` always unhooks and restores previous state.

- [ ] **Step 1: Write failing hook lifecycle and routing tests**

  Add deterministic tests proving a centering request is consumed once, nested state is restored, and manager/add confirmation, validation warning, delete confirmation, login confirmation, and native profile-operation error routes select the centered profile-message contract.

- [ ] **Step 2: Run RED MessageBox tests**

  Run: `cargo test --test windows_app profile_message_centering -- --nocapture`

  Expected: failure because the centered message contract and hook lifecycle do not exist.

- [ ] **Step 3: Implement the scoped CBT hook**

  Resolve the target work area before calling `MessageBoxW`. Install `SetWindowsHookExW(WH_CBT, ..., GetCurrentThreadId())`; on the first `HCBT_ACTIVATE`, consume the thread-local request and center the activated HWND using Task 1 geometry. Always call `CallNextHookEx`. Guard the callback against unwinding, use copied values only, and restore both `HHOOK` and any prior request in `Drop`. Hook-install or move failure must still call `MessageBoxW` normally.

- [ ] **Step 4: Route every profile message through the wrapper**

  Keep existing message text, styles, return-value mapping, and owner handles. Replace the direct native profile-operation error `MessageBoxW` call with the profile-dialog wrapper. Leave non-profile update and application messages unchanged.

- [ ] **Step 5: Run GREEN focused tests**

  Run:
  - `cargo test --lib windows::profile_dialog -- --nocapture`
  - `cargo test --test windows_app -- --nocapture`
  - `cargo test --test localization_runtime -- --nocapture`

  Expected: all focused tests pass without warnings.

- [ ] **Step 6: Update manual release checks**

  Add unchecked scenarios for 100/125/150/200% DPI, negative-coordinate and vertically arranged monitors, opening from each monitor, manager→add→warning/confirmation continuity, Escape/cancel owner restoration, RTL copy, and tooltip/message visibility. Do not claim these manual checks were performed.

- [ ] **Step 7: Run full gates**

  Run:
  - `cargo fmt --all -- --check`
  - `cargo test --all-targets`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo build --release`
  - `git diff --check`

- [ ] **Step 8: Commit Task 2**

  Commit: `fix: Center usage profile messages`

