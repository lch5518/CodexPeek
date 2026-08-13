# Hover Popup Information Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove widget-duplicated weekly usage from the hover popup and render reset time plus pace, short-term forecast, and weekly forecast in one dynamically sized forecasting section.

**Architecture:** Keep `UsagePopupPresentation` as the domain-to-native boundary and delete the redundant usage metric fields there. Reuse existing localized forecast strings and semantic window labels, then replace fixed forecast rows with measured wrapped text laid out inside one native section.

**Tech Stack:** Rust 2021, Windows GDI/Win32 `DrawTextW`, existing localization and widget view models.

## Global Constraints

- Preserve Rust 1.85 compatibility and add no dependency.
- Keep polling, settings, authentication, accessibility fallback, and popup hover lifetime unchanged.
- Reuse existing localized strings; do not add UI copy unless the existing keys cannot express the approved design.
- All modified public or complex I/O APIs require Korean rustdoc.
- Long pace and forecast details must wrap without ellipsis in LTR and RTL layouts.

---

### Task 1: Simplify the popup presentation model

**Files:**
- Modify: `src/windows/popup.rs:67-137`
- Test: `src/windows/popup.rs:250-356`

**Interfaces:**
- Consumes: `WidgetViewModel`, `Language`, `ForecastView::line()`, and `domain::window_kind_label(WindowKind, Language)`.
- Produces: `UsagePopupPresentation { profile_label, reset_label, reset_text, forecast_label, pace_summary, pace_detail, forecasts }` and `PopupForecastLine { label, detail }`.

- [ ] **Step 1: Write failing presentation tests**

Replace the weekly metric assertions with assertions that the presentation exposes no `usage_label` or `metric_percent`, keeps the secondary reset time, uses `MenuUsageForecast` as `forecast_label`, and returns exactly these semantic rows in order:

```rust
assert_eq!(presentation.profile_label, "Work");
assert_eq!(presentation.reset_text.as_deref(), Some("2026-08-18 10:23"));
assert_eq!(presentation.forecast_label, "Usage forecasting");
assert_eq!(
    presentation.forecasts,
    vec![
        PopupForecastLine {
            label: "Short".to_owned(),
            detail: "Collecting primary samples".to_owned(),
        },
        PopupForecastLine {
            label: "Weekly".to_owned(),
            detail: "About 52% will remain".to_owned(),
        },
    ]
);
```

Add a fallback test that removes `secondary` and asserts the primary reset time is used. Keep the empty-window test and assert `reset_text == None` and `forecasts.is_empty()`.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test windows::popup::tests --lib
```

Expected: FAIL because the current model still exposes the weekly metric and uses `Primary window`/`Secondary window` labels.

- [ ] **Step 3: Implement the minimum presentation change**

Delete `usage_label` and `metric_percent`. Add:

```rust
pub(crate) forecast_label: String,
```

Set it from `LocalizationKey::MenuUsageForecast`. Build forecast rows with:

```rust
crate::domain::window_kind_label(crate::WindowKind::Primary, language)
crate::domain::window_kind_label(crate::WindowKind::Secondary, language)
```

Keep `row = secondary.or(primary)` for reset time, so weekly remains preferred without duplicating the metric.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run:

```powershell
cargo test windows::popup::tests --lib
```

Expected: all popup presentation tests PASS.

- [ ] **Step 5: Commit the model change**

```powershell
git add src/windows/popup.rs
git commit -m "refactor: Simplify hover popup presentation"
```

### Task 2: Render one dynamically measured forecast section

**Files:**
- Modify: `src/windows/native/usage_popup.rs:36-225`
- Modify: `src/windows/native/usage_popup.rs:319-530`
- Test: `src/windows/native/usage_popup.rs:691-736`
- Modify: `docs/RELEASE_CHECKLIST.md:138-149`

**Interfaces:**
- Consumes: simplified `UsagePopupPresentation` from Task 1.
- Produces: `PopupLayout` containing measured pace and forecast row heights; the renderer draws account/reset and one forecasting section.

- [ ] **Step 1: Write failing layout and formatting tests**

Add pure layout tests for a compact header plus measured content:

```rust
assert_eq!(forecast_row_height(18, 96), 42);
assert_eq!(forecast_row_height(44, 96), 68);
assert_eq!(popup_height_for_content(220, 96), 240);
```

Replace the old ellipsis assertion with:

```rust
assert_eq!(
    wrapped_text_format(false),
    DT_LEFT | DT_WORDBREAK | DT_NOPREFIX
);
assert_eq!(
    wrapped_text_format(true),
    DT_RIGHT | DT_RTLREADING | DT_WORDBREAK | DT_NOPREFIX
);
```

Expected production changes that make these tests pass: `forecast_row_height`, `popup_height_for_content`, and `wrapped_text_format`.

- [ ] **Step 2: Run focused native tests and verify RED**

Run:

```powershell
cargo test windows::native::usage_popup::tests --lib
```

Expected: FAIL because the current renderer uses fixed 60-pixel sections and `DT_END_ELLIPSIS`.

- [ ] **Step 3: Implement shared wrapped-text measurement**

Generalize `measure_pace_detail_height` into a helper that measures any string with `DrawTextW`, the existing 11-point font, `DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX`, and the existing RTL alignment. Measure `pace_detail` and every `PopupForecastLine::detail`; fall back to four lines when GDI measurement fails.

Keep one `PopupLayout` with the exact measured top and height for each rendered block. Do not add a general layout framework.

- [ ] **Step 4: Remove the duplicated weekly usage block**

In `paint_content`:

- draw the profile icon and profile label;
- draw `Reset at: ...` directly below the profile label;
- draw one separator;
- draw one forecasting icon and `presentation.forecast_label`;
- draw `pace_summary`, optional wrapped `pace_detail`, then `Short` and `Weekly` rows inside the same section;
- do not draw a percent, progress track, or separators between forecast rows.

Delete `draw_percent`, fixed `FORECAST_SECTION_HEIGHT_LOGICAL`, and obsolete fixed-top helpers.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```powershell
cargo test windows::native::usage_popup::tests --lib
cargo test windows::popup::tests --lib
```

Expected: all popup tests PASS.

- [ ] **Step 6: Update the manual release check**

Change the hover checklist to require:

- account/reset followed by one usage-forecast section;
- no duplicated weekly usage metric;
- short and weekly rows in one section;
- long Korean, English, and RTL forecast text wrapping without clipping at 100/125/150/200% DPI.

- [ ] **Step 7: Run repository verification**

```powershell
cargo fmt --all
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
git diff --check
```

Expected: all commands exit 0; the live-desktop-only test may remain ignored.

- [ ] **Step 8: Commit the renderer change**

```powershell
git add src/windows/native/usage_popup.rs docs/RELEASE_CHECKLIST.md
git commit -m "feat: Consolidate hover popup forecasts"
```

### Task 3: Manual Windows verification

**Files:**
- No source changes expected.

**Interfaces:**
- Consumes: release executable from Task 2.
- Produces: user-confirmed visual result before merge.

- [ ] **Step 1: Replace only the currently verified CodexPeek process**

Confirm the active process path, stop only that PID, then start the feature worktree's `target\release\codex-peek.exe` hidden.

- [ ] **Step 2: Verify the approved behavior**

Hover for at least ten seconds and confirm:

- the popup stays open while hovered;
- reset time is directly below the account name;
- weekly percent and progress bar are absent;
- pace, Short, and Weekly are in one forecast section;
- the long insufficient-activity sentence is fully visible and wraps.

- [ ] **Step 3: Keep the branch unmerged for user confirmation**

Report the running PID, test results, and manual checks still requiring confirmation. Do not merge or push until explicitly requested.
