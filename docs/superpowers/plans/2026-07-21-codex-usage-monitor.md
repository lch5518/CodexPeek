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

**Files:** create `Cargo.toml`, `src/lib.rs`, `src/domain.rs`, `src/errors.rs`, `src/localization.rs`; add unit tests beside the modules. Do not create UI/RPC/runtime stubs yet.

Create package `codex-usage-monitor` with Rust edition 2021 and `rust-version = "1.85"`. Declare the plan's runtime dependencies and optimized release profile (`opt-level = "z"`, LTO, strip, one codegen unit, panic abort), but keep task code platform-neutral.

Test-first implement these public interfaces:

```rust
pub enum WindowKind { Primary, Secondary }
pub enum UsageLevel { Stable, Normal, Caution, Danger, Limited }
pub struct UsageWindow {
    pub kind: WindowKind,
    pub used_percent: f64,
    pub window_duration_mins: Option<u64>,
    pub resets_at: Option<SystemTime>,
}
pub struct CodexUsage {
    pub primary: Option<UsageWindow>,
    pub secondary: Option<UsageWindow>,
    pub fetched_at: SystemTime,
}
pub enum Language { Korean, English }
pub enum UsageError {
    CliNotFound, UnsupportedCli, AppServerStartFailed, RpcTimeout,
    RpcOverloaded, NotLoggedIn, AuthenticationExpired, InvalidResponse,
    RateLimitUnavailable, RequestFailed,
}
```

`UsageWindow::new` rejects negative or non-finite percentages with `InvalidResponse`, accepts values above 100, `bar_percent` clamps only rendering to 0..100, and `level` implements the exact global thresholds. Period labels use actual positive durations (`N일/Nd`, `N시간/Nh`, otherwise `N분/Nm`); missing/zero duration falls back to `단기/Short` for primary and `주간/Weekly` for secondary. Remaining labels round up to minutes and format days+hours, hours+minutes, or minutes; missing timestamp is `초기화 시각 없음/Reset unavailable`, and elapsed timestamps are `곧 초기화/Reset soon`.

Each `UsageError` exposes a stable snake-case diagnostic code and a Korean/English user message without embedding source errors or sensitive values. Public Rust documentation comments must be Korean. Cover 0, 49, 50, 74, 75, 89, 90, 99, 100, >100, negative/non-finite, missing/zero duration, missing/past timestamps, and language completeness. Run focused RED/GREEN tests, then `cargo test --all-targets` and `cargo fmt --check` before committing `feat: add usage domain foundation`.

### Task 2: Safe Codex CLI and app-server client

**Files:** create `src/codex/mod.rs`, `src/codex/locator.rs`, `src/codex/process.rs`, and `src/codex/app_server.rs`; update `src/lib.rs` and the Windows feature list in `Cargo.toml`.

Expose a `UsageProvider: Send + Sync` trait and an `AppServerUsageProvider` implementation. CLI discovery gathers and deduplicates `where.exe` and `PATH` candidates named `codex.exe`, `codex.cmd`, `codex.ps1`, or `codex`, then verifies each candidate by running `--version` with a short timeout. Prefer native executables, followed by direct executables, `.cmd`, and `.ps1`; versions below 0.141.0 return `UnsupportedCli`. Wrapper launch plans are fixed and never interpolate user-provided arguments: `.cmd` uses `cmd.exe /D /S /C`, and `.ps1` uses `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`.

Launch `codex app-server --stdio` without a visible console, with piped stdin/stdout and discarded stderr. Put the child in a Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; the process guard closes stdin, waits briefly for graceful exit, and terminates the job tree on timeout or drop. The complete provider call has a 30-second deadline.

Use newline-delimited JSON requests with monotonically increasing numeric IDs. Send `initialize` with client name `codex_usage_monitor`, title `Codex Usage Monitor`, and the package version, followed by the `initialized` notification. Then call `account/read { refreshToken: false }`, deserialize only the account type/presence needed to detect login, and call `account/rateLimits/read`. Ignore interleaved notifications and correlate responses by ID. Deserialize only `rateLimits.primary`, `rateLimits.secondary`, `usedPercent`, `windowDurationMins`, and `resetsAt`; ignore all additional fields without retaining or logging them.

Map RPC method-not-found to `UnsupportedCli`, overload to `RpcOverloaded`, deadline expiry to `RpcTimeout`, malformed/invalid DTO data to `InvalidResponse`, missing account to `NotLoggedIn`, and both windows absent to `RateLimitUnavailable`. Other RPC errors become `RequestFailed` without propagating server messages or data. Convert non-negative UNIX reset seconds to `SystemTime`; an invalid reset timestamp becomes `None` while preserving the window. Percent validation remains delegated to `UsageWindow::new`, including preserving values above 100.

When the rate-limit request fails with a refreshable authentication/request error and `allow_auth_refresh` is true, call `account/read { refreshToken: true }` and retry the rate-limit request exactly once. Do not retry method-unavailable, overload, timeout, or malformed responses. A refresh that still has no account returns `AuthenticationExpired`.

Unit-test pure candidate priority/version parsing/wrapper plans and a transport-independent JSONL session. Fixtures must cover the documented full response plus extra sensitive-looking fields being ignored, primary-only, secondary-only, both-null, notification interleaving, invalid percentages/timestamps, malformed JSON/EOF, login absence, method-unavailable, overload, timeout, and the exact forced-refresh request sequence. Tests must validate parsed domain values and emitted requests rather than merely mock call counts. Run focused RED/GREEN tests, then `cargo test --all-targets`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` before committing `feat: add safe codex app server client`.

### Task 3: Runtime services

**Files:** create `src/config.rs`, `src/diagnostics.rs`, `src/poller.rs`, and `src/update_check.rs`; update `src/lib.rs`, `src/localization.rs`, `src/codex/app_server.rs`, and dependencies only as required. Windows registry integration remains Task 4.

Implement schema version 1 settings with the exact fields from the product plan: `schema_version`, `refresh_interval_minutes`, `display_mode`, `widget_visible`, `taskbar_offset`, `monitor_device`, `floating_position`, `always_on_top`, `start_with_windows`, `startup_view`, `auto_auth_refresh`, `language`, and `last_update_check_unix`. Use typed enums for display mode (`Taskbar`/`Floating`), startup view (`Widget`/`TrayOnly`), and language preference (`Auto`/`Korean`/`English`), plus an integer logical-pixel position. Defaults are taskbar, visible, 5 minutes, offset zero, no saved monitor/position, always-on-top, autostart off, widget startup, automatic auth refresh, language auto, and no prior update check. Only 1/5/10/15/30-minute intervals are valid; reject or normalize unsafe/unreasonable string and coordinate data without panicking.

`SettingsStore` resolves `%APPDATA%/CodexUsageMonitor/settings.json`, creates the directory, writes a same-directory uniquely named temporary file, flushes and syncs it, and atomically replaces the target with the Windows replace/move API. On invalid JSON, unsupported schema, or invalid fields, preserve the original as `settings.corrupt-<unix>.json` and return defaults. Loading a missing file returns defaults without creating it. Tests use an injected root directory and cover round trips, every default, validation, replacement, unsupported schema, corrupt backup contents, and no leftover temporary files.

`DiagnosticLogger` writes sanitized single-line records to `%TEMP%/codex-usage-monitor.log`, rotates once to `.log.1` before exceeding 1 MiB, and never records raw RPC frames, auth contents, proxy values, tokens, account IDs, or emails. Its API accepts stable diagnostic codes plus controlled descriptions; a defense-in-depth sanitizer masks bearer values and credential/account/email/proxy-looking key-value text. Tests cover masking, newline removal, rotation, and replacement of an existing `.log.1`. Add safe diagnostic models for CLI/RPC/login/settings/proxy-presence/taskbar checks; auth diagnostics may report only the auth path and existence boolean.

Refactor `UsageProvider` to the plan's runtime interface `fetch(&self, allow_auth_refresh: bool)`, preserving the provider's shared single-flight safety. Implement a pure `PollState` plus a thread-based `PollingService`. The state accepts automatic, manual, reset, and forced-auth triggers; it permits only one fetch at a time, rejects manual refreshes within 10 seconds, schedules normal polling at the configured interval, and uses 1/2/4/8/15-minute failure backoff. A successful parse alone updates `last_success_at`; failures preserve the last good `CodexUsage`. A snapshot is stale after `max(2 * interval, 10 minutes)`. Future reset timestamps can advance the next poll; each distinct elapsed reset timestamp may trigger at most one immediate fetch, preventing a tight loop when the server keeps returning an old timestamp. The worker receives commands over channels, stops cleanly, exposes snapshots without holding locks during provider calls, and passes the current automatic/forced authentication-refresh policy to the provider. Use an injectable clock or explicit `Instant`/`SystemTime` inputs for deterministic state-machine tests.

Implement an injectable update checker. A repository URL is valid only for `https://github.com/<owner>/<repo>` (optional `.git`, no credentials/query/fragment); absent or invalid package metadata disables checks without network activity. When due (at most once per 24 hours), GET the GitHub `releases/latest` API with a fixed product user agent, bounded response size and timeout, parse `tag_name` as semver, and report only a newer version plus its HTTPS GitHub release page. Never download or execute an asset. A fake HTTP backend covers disabled metadata, not-due state, newer/equal/older versions, malformed/oversized responses, non-success responses, and unsafe release URLs.

Extend localization with exhaustive Korean/English keys required by polling status, stale state, menus, diagnostics, update availability, and window labels. Tests iterate every key and both languages to reject missing/empty text. Run focused RED/GREEN tests, then `cargo test --all-targets`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` before committing `feat: add runtime services`.

### Task 4: Native Windows application

Implement the Win32 application shell, single-instance behavior, tray icon/menu, 380x112 floating widget, 380x48 taskbar widget, GDI rendering, status text/icons, multi-monitor position recovery, Per-Monitor V2 DPI, taskbar enumeration/embedding verification, Explorer/taskbar recovery, vertical-taskbar rejection, automatic floating fallback, HKCU Run registration verification, and all context-menu actions. Keep UI independent from RPC/config internals and unit-test pure layout/action/state mapping.

### Task 5: Packaging, documentation, and CI

Add an original meter icon and Windows version resources, optimized release profile, MIT license, third-party notice, Korean/English README covering installation/use/security/network paths/settings/logs/diagnostics/taskbar limitations/building, and Windows GitHub Actions for fmt/clippy/tests/release artifact. Update checks must remain dormant until a valid GitHub repository metadata URL is configured.

### Task 6: Integration and release verification

Run formatting, clippy with warnings denied, all tests, release build, unauthenticated/available Codex `--diagnose` paths, and inspect the final diff against every global constraint. Record manual Windows 10/11, DPI, multi-monitor, Explorer restart, autohide, logout, and proxy checks in a release checklist without claiming unperformed checks.
