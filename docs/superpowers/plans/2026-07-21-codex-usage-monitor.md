# Codex Usage Monitor Implementation Plan

> **For agentic workers:** Use test-driven development for every behavior change. Each task must include a failing-test run, minimal implementation, passing tests, self-review, and a commit.

**Goal:** Build a portable native Windows Codex usage monitor that obtains quota data through the official local Codex app-server RPC, displays it in tray/floating/taskbar modes, and never reads or logs tokens.

**Architecture:** A small domain layer is fed by a short-lived `codex app-server --stdio` JSONL client. A single-flight polling worker owns retry/stale state and posts snapshots to a raw Win32 UI. Taskbar integration is optional and must fall back to the floating widget without terminating the app.

**Tech stack:** Rust 2021 (rust-version 1.85), `windows`, `ureq` + `native-tls`, `serde`, `serde_json`, `dirs`, `semver`, `winres`, MSVC Windows 10/11.

## Global Constraints

- Product/executable: `Codex Usage Monitor` / `codex-usage-monitor.exe`; no copied branding or logo.
- Minimum documented Codex CLI: 0.141.0; runtime RPC capability is authoritative.
- Never deserialize, store, or log access tokens, refresh tokens, account IDs, emails, auth file contents, authorization headers, RPC originals, or proxy values.
- Do not invoke `codex exec`; use only `account/read` and `account/rateLimits/read` through `codex app-server --stdio`.
- All Rust documentation comments are Korean.
- Default settings: taskbar mode, visible, 5-minute polling, always-on-top, auto auth refresh enabled, autostart disabled, language auto.
- Polling: one request at a time, 10-second manual cooldown, 1/2/4/8/15-minute backoff, 30-second RPC deadline, stale after `max(2 * interval, 10 minutes)`.
- Usage levels: stable 0-49, normal 50-74, caution 75-89, danger 90-99, limit 100+. Preserve values above 100; clamp only visual bars.
- UI supports Korean/English, Per-Monitor V2 DPI, text/icon plus color, and taskbar-to-floating fallback.
- Settings path `%APPDATA%\CodexUsageMonitor\settings.json`; log path `%TEMP%\codex-usage-monitor.log`, one 1 MiB rotation.
- Update feature checks GitHub releases only; no binary download or self-replacement.
- License is MIT with the reference project's MIT notice in `THIRD_PARTY_NOTICES.md`.

---

### Task 1: Project foundation and domain behavior

Create Cargo/build configuration and the library/module skeleton. Test-first implement `WindowKind`, `UsageWindow`, `CodexUsage`, usage severity, percentage validation, actual-period labels, reset countdown formatting, Korean/English localization, and typed `UsageError` user/diagnostic messages. Cover 0, 100, >100, negative/non-finite, missing/past timestamps, and language completeness.

### Task 2: Safe Codex CLI and app-server client

Test-first implement CLI discovery with native EXE preference, wrapper launch plans, hidden child processes, Windows Job Object kill-on-close, 30-second deadline, JSONL request/response correlation with interleaved notifications, account state read, rate-limit parsing, optional forced refresh exactly once, method-unavailable/overloaded/timeout/malformed response mapping, and graceful shutdown. Deserialize only required non-sensitive fields.

### Task 3: Runtime services

Test-first implement versioned settings with validation, atomic replacement and corrupt backup; redacted rotating diagnostics; a single-flight polling state machine with manual cooldown/backoff/stale preservation/reset-triggered refresh; and an injectable GitHub release checker that disables itself without valid repository metadata. Add registry-backend-independent autostart command construction tests where appropriate.

### Task 4: Native Windows application

Implement the Win32 application shell, single-instance behavior, tray icon/menu, 380x112 floating widget, 380x48 taskbar widget, GDI rendering, status text/icons, multi-monitor position recovery, Per-Monitor V2 DPI, taskbar enumeration/embedding verification, Explorer/taskbar recovery, vertical-taskbar rejection, automatic floating fallback, HKCU Run registration verification, and all context-menu actions. Keep UI independent from RPC/config internals and unit-test pure layout/action/state mapping.

### Task 5: Packaging, documentation, and CI

Add an original meter icon and Windows version resources, optimized release profile, MIT license, third-party notice, Korean/English README covering installation/use/security/network paths/settings/logs/diagnostics/taskbar limitations/building, and Windows GitHub Actions for fmt/clippy/tests/release artifact. Update checks must remain dormant until a valid GitHub repository metadata URL is configured.

### Task 6: Integration and release verification

Run formatting, clippy with warnings denied, all tests, release build, unauthenticated/available Codex `--diagnose` paths, and inspect the final diff against every global constraint. Record manual Windows 10/11, DPI, multi-monitor, Explorer restart, autohide, logout, and proxy checks in a release checklist without claiming unperformed checks.

