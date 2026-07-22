# Compact Taskbar Glass Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 380-pixel taskbar renderer with a fixed 208×48 logical-pixel weekly-usage glass widget while preserving all Codex usage retrieval behavior.

**Architecture:** Keep `AppRuntime`, polling, RPC, and domain types unchanged. Add a pure taskbar presentation/layout module consumed only by the Win32 renderer, extend the UI view model with localized taskbar copy and an explicit loading/ready/error state, and keep the floating renderer on its existing path. Render frosted translucency with the existing per-pixel `UpdateLayeredWindow` pipeline because official DWM blur APIs do not support the embedded child-window architecture.

**Tech Stack:** Rust 2021, `windows` 0.61 Win32 bindings, GDI, `UpdateLayeredWindow`, native tooltip controls, Cargo tests.

## Global Constraints

- Do not change Codex RPC, authentication, polling, rate-limit DTOs, or domain models.
- The taskbar widget width is exactly 208 logical pixels and never exceeds 220 logical pixels.
- The default display is weekly usage only; select `secondary` first and use the only available row as a safe fallback.
- Keep the floating widget and tray behavior unchanged.
- All Rust documentation comments are Korean.
- Use only documented Windows APIs; do not add undocumented composition calls.

---

### Task 1: Pure Taskbar Presentation and Layout

**Files:**
- Create: `src/windows/taskbar_widget.rs`
- Modify: `src/windows/mod.rs`
- Test: `src/windows/taskbar_widget.rs`

**Interfaces:**
- Consumes: `WidgetViewModel`, `UsageRowView`, `WidgetDataState`, `Rect`.
- Produces: `TaskbarLayout::for_size(width: i32, height: i32, dpi: u32)`, `TaskbarPresentation::from_view(view: &WidgetViewModel)`, and `TaskbarRisk`.

- [ ] **Step 1: Write failing selection, threshold, and DPI layout tests**

```rust
#[test]
fn weekly_row_is_preferred_and_single_row_is_the_fallback() {
    let view = view_with_rows(Some(row(20.0)), Some(row(80.0)));
    assert_eq!(TaskbarPresentation::from_view(&view).used_percent, Some(80.0));

    let view = view_with_rows(Some(row(20.0)), None);
    assert_eq!(TaskbarPresentation::from_view(&view).used_percent, Some(20.0));
}

#[test]
fn risk_thresholds_match_taskbar_policy() {
    assert_eq!(TaskbarRisk::from_percent(69.0), TaskbarRisk::Healthy);
    assert_eq!(TaskbarRisk::from_percent(70.0), TaskbarRisk::Warning);
    assert_eq!(TaskbarRisk::from_percent(90.0), TaskbarRisk::Critical);
}

#[test]
fn layouts_fit_at_supported_dpis() {
    for dpi in [96, 120, 144, 192] {
        let width = logical_to_physical(208, dpi);
        let height = logical_to_physical(48, dpi);
        let layout = TaskbarLayout::for_size(width, height, dpi);
        assert!(layout.dot.is_inside(layout.window));
        assert!(layout.label.is_inside(layout.window));
        assert!(layout.percent.is_inside(layout.window));
        assert!(layout.progress.is_inside(layout.window));
        assert!(!layout.label.intersects(layout.percent));
    }
}
```

- [ ] **Step 2: Run the tests and verify the module is missing**

Run: `cargo test taskbar_widget --lib`

Expected: compilation fails because `TaskbarLayout`, `TaskbarPresentation`, and `TaskbarRisk` are not defined.

- [ ] **Step 3: Implement the pure module**

```rust
pub const TASKBAR_WIDTH_LOGICAL: i32 = 208;
pub const TASKBAR_HEIGHT_LOGICAL: i32 = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarRisk { Healthy, Warning, Critical, Loading, Error }

impl TaskbarRisk {
    pub fn from_percent(percent: f64) -> Self {
        if percent >= 90.0 { Self::Critical }
        else if percent >= 70.0 { Self::Warning }
        else { Self::Healthy }
    }
}

pub struct TaskbarPresentation<'a> {
    pub label: &'a str,
    pub percent_text: &'a str,
    pub used_percent: Option<f64>,
    pub tooltip: &'a str,
    pub risk: TaskbarRisk,
}
```

`TaskbarLayout::for_size` uses fixed logical metrics: 11px horizontal inset, 9px top inset, 6px status dot, 8px dot-to-label gap, 42px percent reserve, and a 3px progress bar positioned 9px above the bottom edge. Clamp every derived rectangle into the actual client height so a 40px compact taskbar remains valid.

- [ ] **Step 4: Run the focused tests**

Run: `cargo test taskbar_widget --lib`

Expected: all taskbar presentation and layout tests pass.

- [ ] **Step 5: Commit the pure UI policy**

```powershell
git add src/windows/taskbar_widget.rs src/windows/mod.rs
git commit -m "feat: add compact taskbar presentation model"
```

### Task 2: Localized Weekly Copy and Tooltip State

**Files:**
- Modify: `src/windows/mod.rs`
- Modify: `src/app.rs`
- Modify: `src/localization.rs`
- Test: `src/app.rs`
- Test: `tests/localization_runtime.rs`

**Interfaces:**
- Consumes: existing `PollSnapshot`, `UsageRowView`, and localization functions.
- Produces: `WidgetDataState`, plus `taskbar_label` and `taskbar_tooltip` fields on `WidgetViewModel`.

- [ ] **Step 1: Write failing UI snapshot tests**

Add tests asserting that Korean output contains `주간 사용량`, `현재 사용량`, `남은 사용량`, and the existing reset text, while English output contains `Weekly usage`, `Current usage`, `Remaining`, and the reset text. Assert that 125% used formats remaining as `0%` and does not modify the source usage value.

- [ ] **Step 2: Run the snapshot and localization tests**

Run: `cargo test app::tests --all-targets` and then `cargo test --test localization_runtime`

Expected: failure because the taskbar localization keys and view-model fields do not exist.

- [ ] **Step 3: Add explicit UI state and localized fields**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetDataState { Loading, Ready, Error }

pub struct WidgetViewModel {
    // existing fields remain unchanged
    pub taskbar_label: String,
    pub taskbar_tooltip: String,
    pub data_state: WidgetDataState,
}
```

Add localization keys for `TaskbarWeeklyUsage`, `TaskbarCurrentUsage`, `TaskbarRemainingUsage`, `TaskbarReset`, `TaskbarHealthy`, `TaskbarWarning`, `TaskbarCritical`, `TaskbarLoading`, and `TaskbarError`. Build tooltip text in `AppRuntime::snapshot` from the selected weekly row and the already formatted `reset_text`. Set `WidgetDataState::Error` when `last_error` exists, `Loading` when no usage is available and a fetch is active, and `Ready` otherwise.

- [ ] **Step 4: Run the focused tests**

Run: `cargo test app::tests --all-targets` and then `cargo test --test localization_runtime`

Expected: all new copy, remaining-percent, and localization completeness tests pass.

- [ ] **Step 5: Commit localized UI state**

```powershell
git add src/windows/mod.rs src/app.rs src/localization.rs tests/localization_runtime.rs
git commit -m "feat: add weekly taskbar copy and tooltip text"
```

### Task 3: Compact Taskbar Geometry and Frosted Layer Rendering

**Files:**
- Modify: `src/windows/taskbar.rs`
- Modify: `src/windows/native/platform.rs`
- Test: `tests/windows_app.rs`
- Test: `src/windows/native/platform.rs`

**Interfaces:**
- Consumes: `TASKBAR_WIDTH_LOGICAL`, `TaskbarLayout`, and `TaskbarPresentation` from Task 1.
- Produces: fixed taskbar geometry and a taskbar-only layered renderer; the floating `paint_widget_content` path remains unchanged.

- [ ] **Step 1: Change geometry expectations before production code**

```rust
assert_eq!(taskbar_widget_size(48, 96), Ok((208, 48)));
assert_eq!(taskbar_widget_size(48, 120), Ok((260, 48)));
assert_eq!(taskbar_widget_size(60, 144), Ok((312, 60)));
assert_eq!(taskbar_widget_size(96, 192), Ok((416, 96)));
```

Also add pure tests for rounded-mask corner alpha, center alpha, deterministic noise bounds, and hover tint interpolation endpoints.

- [ ] **Step 2: Run the geometry and alpha tests**

Run: `cargo test taskbar_attachment_adapts_to_compact_taskbar_height --all-targets` and then `cargo test layered --all-targets`

Expected: failures showing the old 380 logical-pixel width and missing glass helpers.

- [ ] **Step 3: Implement the taskbar-only renderer**

Change `taskbar_widget_size` to use `logical_to_physical(TASKBAR_WIDTH_LOGICAL, dpi)`. Replace the taskbar call to the shared two-row renderer with `paint_compact_taskbar_content` while leaving floating rendering untouched.

The layered DIB pipeline must:

1. Start with zeroed pixels.
2. Draw a rounded 10px-radius material surface.
3. Assign material alpha 174 at rest and interpolate to 188 on hover.
4. Add deterministic per-pixel RGB noise in the inclusive range `-2..=2` only to material pixels.
5. Add a one-pixel top/edge highlight equivalent to white at 5% opacity.
6. Draw the state dot, weekly label, right-aligned percent, track, and fill using the pure layout rectangles.
7. Keep text and progress pixels opaque while premultiplying every translucent pixel before `UpdateLayeredWindow`.

Use `Segoe UI Variable` with `FW_NORMAL` for the label and `FW_MEDIUM` for the percent. Let the Windows font mapper substitute `Segoe UI` on systems without the Variable family.

- [ ] **Step 4: Run focused layout and alpha tests**

Run: `cargo test taskbar_widget --all-targets`, `cargo test taskbar_attachment_adapts --all-targets`, and `cargo test layered --all-targets`.

Expected: all focused tests pass.

- [ ] **Step 5: Commit compact glass rendering**

```powershell
git add src/windows/taskbar.rs src/windows/native/platform.rs tests/windows_app.rs
git commit -m "feat: render compact frosted taskbar widget"
```

### Task 4: Interruptible Hover and Native Tooltip

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/windows/native/platform.rs`
- Test: `src/windows/native/platform.rs`

**Interfaces:**
- Consumes: `taskbar_tooltip`, `TaskbarPresentation`, and the layered renderer.
- Produces: a non-blocking tooltip and a 150ms brightness transition with no layout movement.

- [ ] **Step 1: Write failing hover transition tests**

Test that a `HoverTransition` starts at zero, advances monotonically toward 255 in ten 15ms ticks, reverses from its current value without jumping, and reaches zero after mouse leave.

- [ ] **Step 2: Run the hover test**

Run: `cargo test hover_transition --lib`

Expected: failure because `HoverTransition` is not defined.

- [ ] **Step 3: Implement hover tracking and tooltip lifecycle**

Add the `Win32_UI_Controls` feature. Create one `TOOLTIPS_CLASSW` window owned by the hidden owner, register the widget client rectangle with `TTM_ADDTOOLW`, and update its text from `taskbar_tooltip` without retaining pointers to temporary UTF-16 buffers. Store the UTF-16 buffer in `NativeState` for the lifetime required by the control.

Handle `WM_MOUSEMOVE`, `WM_MOUSELEAVE`, and a dedicated widget timer. Call `TrackMouseEvent(TME_LEAVE)` once per entry. Every 15ms move the current hover value toward its target by 26, repaint from the current value, and kill the timer at the target. This makes reversal interruptible and settles in at most 150ms.

- [ ] **Step 4: Run hover and taskbar tests**

Run: `cargo test hover_transition --all-targets` and then `cargo test taskbar_widget --all-targets`.

Expected: hover reversal, tooltip copy, and layout tests pass.

- [ ] **Step 5: Commit interaction behavior**

```powershell
git add Cargo.toml Cargo.lock src/windows/native/platform.rs
git commit -m "feat: add taskbar hover and usage tooltip"
```

### Task 5: Full Verification and Installed-Binary Smoke Test

**Files:**
- Modify only if verification reveals a scoped defect.

**Interfaces:**
- Consumes: the completed compact renderer.
- Produces: a verified release executable and installed application.

- [ ] **Step 1: Run formatting, full tests, Clippy, and release build**

```powershell
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Expected: every command exits with code 0.

- [ ] **Step 2: Replace and restart the installed executable**

Stop the running `codex-usage-monitor` process, wait for termination, copy `target\release\codex-usage-monitor.exe` to `%LOCALAPPDATA%\Programs\CodexUsageMonitor\codex-usage-monitor.exe`, and start the installed path.

- [ ] **Step 3: Inspect the live taskbar window**

Use read-only Win32 probes to assert:

- the widget is a visible child of `Shell_TrayWnd`;
- its client size is 208×48 at 96 DPI;
- `WS_EX_LAYERED` remains set;
- the label and percentage appear in a screen capture;
- corner pixels retain taskbar visibility and the center has translucent material pixels.

- [ ] **Step 4: Verify installed and release hashes**

Run `Get-FileHash -Algorithm SHA256` on both executables and require identical hashes.

- [ ] **Step 5: Commit any verification-only correction**

If a correction was required, commit only the files changed for that defect with a message describing the verified symptom. If no correction was required, leave the worktree clean.
