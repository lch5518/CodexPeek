# Widget Tooltip Reset Time Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Identify Codex in the taskbar hover tooltip and show the reset date, weekday, and time in the Windows local time zone.

**Architecture:** Keep tooltip assembly in `app.rs`, add a validated internal calendar value in `domain.rs`, and isolate `SystemTime` to local-time conversion in a small Win32 module. Use the existing `windows` dependency with `Win32_System_Time`; do not add a date crate.

**Tech Stack:** Rust 2021, Rust 1.85+, windows 0.61 Win32 APIs, deterministic unit tests

## Global Constraints

- Preserve polling, RPC, settings, taskbar fallback, and the public `UsageWindow::remaining_label` API.
- Keep `UsageRowView.reset_text` and change only its displayed semantics.
- Format local reset time as `YYYY-MM-DD (요일) HH:mm` or `YYYY-MM-DD (Day) HH:mm`.
- Fall back to `초기화 시각 없음` or `Reset unavailable` on missing or invalid timestamps.
- Add Korean and English behavior together.
- Use TDD and keep Win32 `unsafe` calls in a small platform module with safety comments.

---

### Task 1: Identify Codex in the tooltip

**Files:**
- Modify: `src/app.rs`

**Interfaces:**
- Consumes: `UsageRowView` and `Language`
- Produces: the existing `TaskbarCopy` with revised user-facing text

- [ ] **Step 1: Change the existing tooltip test first**

Use literal assertions for:

```text
Codex 7일 사용량
초기화 시각: 2026-07-27 (월) 03:00
Codex 7d usage
Reset at: 2026-07-27 (Mon) 03:00
```

Add a no-data assertion for `Codex 사용량` and `Codex usage`.

- [ ] **Step 2: Run the targeted test and observe RED**

Run:

```powershell
cargo test --lib taskbar_copy_is_explicit_and_keeps_reset_details_in_the_tooltip
```

Expected: FAIL because the current first line omits Codex and uses `초기화` / `Reset`.

- [ ] **Step 3: Implement the minimal tooltip copy**

For rows, use `Codex {period} 사용량` / `Codex {period} usage` and rename the reset
line to `초기화 시각` / `Reset at`. For no data, use `Codex 사용량` / `Codex usage`.
Do not change the compact taskbar label.

- [ ] **Step 4: Run the targeted test and observe GREEN**

Run the command from Step 2 and require zero failures.

### Task 2: Format validated local calendar values

**Files:**
- Modify: `src/domain.rs`

**Interfaces:**
- Produces: crate-private `ResetDateTime::new(...) -> Option<Self>` and
  `ResetDateTime::localized_label(Language) -> String`

- [ ] **Step 1: Add failing domain tests**

Construct Monday `2026-07-27 03:04` and assert:

```text
2026-07-27 (월) 03:04
2026-07-27 (Mon) 03:04
```

Cover Sunday and reject month 0, weekday 7, hour 24, and minute 60.

- [ ] **Step 2: Run the tests and observe RED**

Run:

```powershell
cargo test --lib reset_date_time
```

Expected: compilation fails because `ResetDateTime` does not exist.

- [ ] **Step 3: Implement the value and formatter**

Store `year`, `month`, `day`, `weekday`, `hour`, and `minute`. Validate the ranges and
use fixed Korean and English weekday arrays. Zero-pad month, day, hour, and minute.
Expose the existing reset-unavailable label as crate-private for app fallback.

- [ ] **Step 4: Run the domain tests and observe GREEN**

Run the command from Step 2 and require zero failures.

### Task 3: Convert SystemTime through Win32 and connect the row

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/windows/mod.rs`
- Create: `src/windows/time.rs`
- Create: `src/windows/time/platform.rs`
- Modify: `src/app.rs`

**Interfaces:**
- Consumes: `SystemTime`
- Produces: `windows::time::local_reset_time(SystemTime) -> io::Result<ResetDateTime>`

- [ ] **Step 1: Add failing FILETIME boundary tests**

Test that Unix epoch maps to `116_444_736_000_000_000` 100-nanosecond ticks, one second
adds `10_000_000`, and pre-epoch time is rejected.

- [ ] **Step 2: Run the test and observe RED**

Run:

```powershell
cargo test --lib windows::time
```

Expected: compilation fails because the converter does not exist.

- [ ] **Step 3: Implement the Win32 conversion**

Enable `Win32_System_Time`. Convert Unix duration to FILETIME ticks with checked
arithmetic, call `FileTimeToSystemTime`, then `SystemTimeToTzSpecificLocalTime(None, ...)`.
Convert the returned `SYSTEMTIME` through `ResetDateTime::new`.

- [ ] **Step 4: Connect absolute reset text**

Have `row_view` call the converter for `window.resets_at`, format the validated value,
and use the unavailable label for `None` or conversion errors. Remove the now-unused
`now` argument from `row_view`; do not change `remaining_label`.

- [ ] **Step 5: Run focused tests and observe GREEN**

Run:

```powershell
cargo test --lib domain::tests
cargo test --lib app::tests
cargo test --lib windows::time
```

Expected: all focused tests pass.

### Task 4: Verify the complete change

**Files:**
- Review all files changed above

**Interfaces:**
- Produces: a release-safe implementation commit

- [ ] **Step 1: Run repository checks**

```powershell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

- [ ] **Step 2: Review the diff**

Confirm no settings or RPC behavior changed, no new crate was added, and every `unsafe`
Win32 call has a safety comment.

- [ ] **Step 3: Commit**

```powershell
git add Cargo.toml src/app.rs src/domain.rs src/windows/mod.rs src/windows/time.rs src/windows/time/platform.rs
git commit -m "feat: Show Codex reset time in widget tooltip"
```
