# Usage Profile Dialog Native Refined Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the usage-profile manager and add-profile dialog as a consistent, DPI-aware Native Refined Win32 experience with status-rich rows and guaranteed single-line button labels.

**Architecture:** Add a pure Windows design module for semantic profile status, palette tokens, DPI scaling, and dialog geometry, plus a small Windows theme adapter for the shared light/dark query. Extend `UsageProfileView` with typed progress data, then keep HWND/GDI ownership and narrowly scoped owner drawing inside the existing profile-dialog platform module so profile actions, modal lifecycle, polling, and authentication boundaries remain unchanged.

**Tech Stack:** Rust 2021 (minimum 1.85), existing `windows` 0.61 bindings, Win32 controls/GDI/DWM, existing localization and polling models, deterministic Rust unit and integration tests.

## Global Constraints

- Support Windows 10/11 x64; do not add Electron, Tauri, WebView, or another browser UI layer.
- Preserve taskbar, floating widget, tray, polling, diagnostics, Explorer recovery, profile mutation, and centered modal behavior.
- Do not read or parse any `auth.json`; do not expose tokens, account IDs, emails, authentication paths, proxy values, or raw RPC payloads.
- Keep CLI and IDE Codex login state unchanged; these dialogs only select the usage profile displayed by CodexPeek.
- Keep settings, filesystem, login/logout, and Codex RPC I/O off the UI thread.
- Use the approved profile-manager visual thresholds: Healthy below 70% consumed, Warning from 70% through below 90%, Critical at 90% or more. Do not change the existing global `UsageLevel` thresholds in this work.
- Use 16 logical pixels outer padding, 56 logical pixels per profile row, 32 logical pixels minimum interactive height, and the colors and typography in `design.md`.
- Measure every localized action string with the active dialog font. Never apply `BS_MULTILINE`; move whole buttons to a second row when necessary so text never wraps or clips.
- Support Korean, English, Spanish, Portuguese, Indonesian, Japanese, Hindi, German, French, Vietnamese, Turkish, and Arabic, including mirrored Arabic geometry.
- Handle 96, 120, 144, 168, and 192 DPI and preserve the selected monitor work-area centering behavior.
- Write Korean rustdoc for new or modified public APIs and complex Win32 resource/state boundaries.
- Use TDD: add a focused failing test, observe RED, implement the smallest change, rerun GREEN, and commit each task separately.

## File map

| File | Responsibility |
| --- | --- |
| `design.md` | Repository-wide CodexPeek visual design system and acceptance criteria |
| `src/windows/design.rs` | Pure status, palette, scaling, text-width, and profile-dialog geometry contracts |
| `src/windows/theme.rs` | Shared read-only Windows light/dark preference query |
| `src/windows/mod.rs` | `UsageProfileView` typed progress fields and module wiring |
| `src/app.rs` | Convert retained polling snapshots into profile row summaries and typed progress data |
| `src/windows/profile_dialog.rs` | Pure row copy/marker contract and controller access by item index |
| `src/windows/profile_dialog/platform.rs` | Win32 creation, fonts/brushes, DPI/theme events, layout, owner drawing, and cleanup |
| `src/windows/native/platform.rs` | Consume the shared Windows theme query for the existing taskbar palette |
| `Cargo.toml` | Enable the existing `windows` crate DWM feature only; no new crate |
| `tests/windows_app.rs` | Public presentation-model, localization, action, and dialog contract regression tests |
| `docs/RELEASE_CHECKLIST.md` | Manual profile-dialog visual, DPI, keyboard, and RTL checks |

---

### Task 1: Add Typed Usage Presentation to Profile Rows

**Files:**
- Modify: `src/windows/mod.rs:1-10,365-390`
- Modify: `src/app.rs:1130-1210`
- Modify: `tests/windows_app.rs:58-66,150-170,330-920`
- Modify: `src/windows/profile_dialog/platform.rs:1560-1590`
- Modify: `src/windows/tray.rs:595-620`

**Interfaces:**
- Consumes: retained `PollSnapshot::usage`, `UsageWindow::{used_percent,bar_percent}`, and the same secondary-or-primary window selection currently used for `summary`.
- Produces: `ProfileUsageStatus::from_used_percent(f64) -> ProfileUsageStatus`, `UsageProfileView::used_percent: Option<u8>`, and `UsageProfileView::usage_status: Option<ProfileUsageStatus>` for Tasks 2 and 4.

- [ ] **Step 1: Add failing status-boundary tests**

Add unit tests beside the new enum in `src/windows/mod.rs`:

```rust
#[test]
fn profile_usage_status_uses_the_design_system_thresholds() {
    assert_eq!(ProfileUsageStatus::from_used_percent(0.0), ProfileUsageStatus::Healthy);
    assert_eq!(ProfileUsageStatus::from_used_percent(69.99), ProfileUsageStatus::Healthy);
    assert_eq!(ProfileUsageStatus::from_used_percent(70.0), ProfileUsageStatus::Warning);
    assert_eq!(ProfileUsageStatus::from_used_percent(89.99), ProfileUsageStatus::Warning);
    assert_eq!(ProfileUsageStatus::from_used_percent(90.0), ProfileUsageStatus::Critical);
    assert_eq!(ProfileUsageStatus::from_used_percent(125.0), ProfileUsageStatus::Critical);
}
```

- [ ] **Step 2: Run the boundary test and verify RED**

Run:

```powershell
cargo test profile_usage_status_uses_the_design_system_thresholds --lib
```

Expected: compile failure because `ProfileUsageStatus` is not defined.

- [ ] **Step 3: Implement the typed status and fields**

Add the following public UI contract with Korean rustdoc in `src/windows/mod.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileUsageStatus {
    Healthy,
    Warning,
    Critical,
}

impl ProfileUsageStatus {
    pub fn from_used_percent(used_percent: f64) -> Self {
        if used_percent >= 90.0 {
            Self::Critical
        } else if used_percent >= 70.0 {
            Self::Warning
        } else {
            Self::Healthy
        }
    }
}
```

Extend `UsageProfileView`:

```rust
pub used_percent: Option<u8>,
pub usage_status: Option<ProfileUsageStatus>,
```

Update every literal fixture to explicitly use `None` unless that test needs a
progress state. Do not add a `Default` implementation that could hide missing
fixture intent.

- [ ] **Step 4: Run library and Windows contract tests**

Run:

```powershell
cargo test profile_usage_status_uses_the_design_system_thresholds --lib
cargo test --test windows_app
```

Expected: the boundary test passes; `windows_app` passes after all literals are
updated.

- [ ] **Step 5: Add failing snapshot-to-row tests**

In the existing `src/app.rs` test module, cover the pure conversion helper that
will be extracted from `usage_profile_views`:

```rust
#[test]
fn profile_usage_presentation_keeps_summary_and_typed_consumed_usage() {
    let presentation = profile_usage_presentation_for_window(Some(&usage_window(81.4)));
    assert_eq!(presentation.used_percent, Some(81));
    assert_eq!(presentation.usage_status, Some(ProfileUsageStatus::Warning));
}

#[test]
fn profile_usage_presentation_omits_fake_progress_without_valid_usage() {
    let presentation = profile_usage_presentation_for_window(None);
    assert_eq!(presentation.used_percent, None);
    assert_eq!(presentation.usage_status, None);
}
```

Also add a snapshot test where `last_error` is transient but `usage` still holds
the previous valid window; assert that typed progress remains present.
Define the local test fixture explicitly so the test does not depend on a real
account:

```rust
fn usage_window(used_percent: f64) -> UsageWindow {
    UsageWindow::new(WindowKind::Secondary, used_percent, None, None).unwrap()
}
```

- [ ] **Step 6: Run the focused app tests and verify RED**

Run:

```powershell
cargo test profile_usage_presentation --lib
```

Expected: compile failure because the presentation helper does not exist.

- [ ] **Step 7: Refactor row construction without parsing localized text**

Add a private value type and helper in `src/app.rs`:

```rust
struct ProfileUsagePresentation {
    summary: String,
    login_required: bool,
    used_percent: Option<u8>,
    usage_status: Option<ProfileUsageStatus>,
}
```

Build it once per profile from one snapshot. Select
`secondary.as_ref().or(primary.as_ref())` exactly as today. For valid usage, use
`window.bar_percent().round() as u8` for drawing and
`ProfileUsageStatus::from_used_percent(window.used_percent)` for color. When
login is required, initial loading has no retained usage, or no usage exists,
set both typed fields to `None`. When a transient error retains usage, keep the
typed fields and the current safe summary behavior.

- [ ] **Step 8: Run focused and regression tests**

Run:

```powershell
cargo test profile_usage_presentation --lib
cargo test --test windows_app
cargo test --test profile_runtime
```

Expected: all pass and existing profile actions remain unchanged.

- [ ] **Step 9: Commit the typed presentation contract**

```powershell
git add src/windows/mod.rs src/app.rs src/windows/profile_dialog/platform.rs src/windows/tray.rs tests/windows_app.rs
git commit -m "feat: Add typed profile usage presentation"
```

---

### Task 2: Centralize Dialog Tokens, DPI Scaling, and Responsive Geometry

**Files:**
- Create: `src/windows/design.rs`
- Modify: `src/windows/mod.rs:1-10`

**Interfaces:**
- Consumes: `ProfileUsageStatus` from Task 1 and measured button text widths supplied by the Win32 platform code.
- Produces: `DialogTheme`, `DialogPalette`, `LogicalRect`, `DialogLayoutInput`, `ProfileManagerLayout`, `AddProfileLayout`, `profile_manager_layout`, `add_profile_layout`, `scale_logical`, and `button_width` for Tasks 3–5.

- [ ] **Step 1: Write failing pure token and layout tests**

Create `src/windows/design.rs` with the test module first. Use exact cases:

```rust
#[test]
fn profile_status_colors_match_design_md() {
    let light = DialogPalette::for_theme(DialogTheme::Light);
    assert_eq!(light.status(ProfileUsageStatus::Healthy), 0x0074_c748);
    assert_eq!(light.status(ProfileUsageStatus::Warning), 0x0023_a6f5);
    assert_eq!(light.status(ProfileUsageStatus::Critical), 0x005c_5cff);
}

#[test]
fn logical_dimensions_scale_at_supported_dpis() {
    for (dpi, expected_row_height) in [(96, 56), (120, 70), (144, 84), (168, 98), (192, 112)] {
        assert_eq!(scale_logical(56, dpi), expected_row_height);
    }
}

#[test]
fn manager_layout_keeps_buttons_single_line_and_uses_whole_button_fallback() {
    let one_row = profile_manager_layout(DialogLayoutInput::new(620, 96, false, [74, 70, 82, 62]));
    assert_eq!(one_row.action_rows, 1);
    assert!(one_row.action_buttons.windows(2).all(|pair| pair[0].right <= pair[1].left));

    let two_rows = profile_manager_layout(DialogLayoutInput::new(420, 96, false, [150, 146, 158, 138]));
    assert_eq!(two_rows.action_rows, 2);
    assert!(two_rows.action_buttons.iter().all(|rect| rect.width() >= 32));
}

#[test]
fn arabic_layout_mirrors_selection_edge_and_action_alignment() {
    let ltr = profile_manager_layout(DialogLayoutInput::new(620, 96, false, [74, 70, 82, 62]));
    let rtl = profile_manager_layout(DialogLayoutInput::new(620, 96, true, [74, 70, 82, 62]));
    assert_eq!(ltr.selection_edge.left, ltr.list.left);
    assert_eq!(rtl.selection_edge.right, rtl.list.right);
    assert_eq!(ltr.action_buttons[3].right, ltr.content.right);
    assert_eq!(rtl.action_buttons[0].left, rtl.content.left);
}
```

Add a loop over 96/120/144/168/192 DPI asserting list, add control, label, edit,
and action rectangles neither overlap nor leave the client bounds.

- [ ] **Step 2: Run design tests and verify RED**

Run:

```powershell
cargo test windows::design::tests --lib
```

Expected: compile failure because the module and types do not exist.

- [ ] **Step 3: Implement exact design tokens and geometry**

Implement `DialogPalette` using Win32 `COLORREF` byte order, constants for
`OUTER_PADDING = 16`, `ROW_HEIGHT = 56`, `CONTROL_HEIGHT = 32`,
`SELECTION_EDGE = 3`, `PROGRESS_HEIGHT = 3`, and gaps 6/8/12. Implement scaling
with rounded integer math:

```rust
pub const fn scale_logical(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(96) as i64 + 48) / 96) as i32
}

pub const fn button_width(measured_text_width: i32, dpi: u32) -> i32 {
    let padded = measured_text_width + scale_logical(24, dpi);
    if padded > scale_logical(72, dpi) { padded } else { scale_logical(72, dpi) }
}
```

Make `profile_manager_layout` and `add_profile_layout` pure and deterministic.
They receive already-measured text widths, allocate whole buttons greedily from
the trailing edge, and use a second row only when the first cannot fit. They do
not truncate text or create a third row. Add Korean rustdoc describing inputs,
returned physical rectangles, RTL behavior, and the single-line invariant.

- [ ] **Step 4: Run design tests and formatting**

Run:

```powershell
cargo test windows::design::tests --lib
cargo fmt --all -- --check
```

Expected: all new pure tests pass.

- [ ] **Step 5: Commit the pure design foundation**

```powershell
git add src/windows/design.rs src/windows/mod.rs
git commit -m "feat: Add native dialog design tokens"
```

---

### Task 3: Share Windows Theme Detection and Manage Dialog Resources

**Files:**
- Create: `src/windows/theme.rs`
- Modify: `src/windows/mod.rs:1-10`
- Modify: `src/windows/native/platform.rs:1-65,1270-1290,1450-1470`
- Modify: `src/windows/profile_dialog/platform.rs:1-110,460-590`
- Modify: `Cargo.toml:15-20`

**Interfaces:**
- Consumes: `DialogPalette`, `DialogTheme`, and scaling functions from Task 2.
- Produces: `system_uses_light_theme() -> bool`, `DialogVisualResources::{new,rebuild_for_dpi}`, and resource cleanup used by Tasks 4–5.

- [ ] **Step 1: Add a failing shared-theme contract test**

Keep registry I/O behind a tiny injectable mapper in `src/windows/theme.rs`:

```rust
#[test]
fn missing_theme_registry_value_falls_back_to_dark() {
    assert!(!light_theme_from_registry_value(None));
    assert!(!light_theme_from_registry_value(Some(0)));
    assert!(light_theme_from_registry_value(Some(1)));
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test windows::theme::tests --lib
```

Expected: compile failure because the shared adapter does not exist.

- [ ] **Step 3: Extract the existing registry query**

Move only the `SystemUsesLightTheme` lookup from
`src/windows/native/platform.rs` into `src/windows/theme.rs`. Keep the same safe
fallback (`false`) when lookup fails. Export it as `pub(crate)` and replace the
taskbar call site with `theme::system_uses_light_theme()` so taskbar behavior is
unchanged.

- [ ] **Step 4: Add Win32 visual resources with deterministic ownership**

Enable `Win32_Graphics_Dwm` in the existing `windows` feature list; add no crate.
Extend both dialog state structs with one `DialogVisualResources` containing:

```rust
struct DialogVisualResources {
    dpi: u32,
    palette: DialogPalette,
    body_font: HFONT,
    heading_font: HFONT,
    background_brush: HBRUSH,
    surface_brush: HBRUSH,
}
```

Create `Segoe UI Variable` fonts first and retry with `Segoe UI` if font creation
fails. If both fail, use `GetStockObject(DEFAULT_GUI_FONT)` without claiming
ownership. Track ownership flags and delete only owned fonts/brushes in `Drop`.
Apply `WM_SETFONT` to list, labels, edits, and buttons after creation and rebuild.

- [ ] **Step 5: Apply supported theme integration with safe fallback**

Call `SetWindowTheme` for native controls and try `DwmSetWindowAttribute` for the
immersive-dark title-bar attribute. Treat unsupported DWM attributes as a visual
fallback, not a dialog-opening error. Handle `WM_SETTINGCHANGE` and
`WM_THEMECHANGED` by rebuilding the palette/brushes, applying control themes,
and calling `InvalidateRect`.

- [ ] **Step 6: Add resource-lifetime seam tests**

In the existing platform test module, add a fake resource owner test that records
owned handles and verifies rebuild/teardown delete each owned handle once while
stock-font fallback is never deleted. Keep the seam free of real HWND creation.

- [ ] **Step 7: Run focused and regression tests**

Run:

```powershell
cargo test windows::theme::tests --lib
cargo test windows::profile_dialog::platform::tests --lib
cargo test --test windows_app
```

Expected: all pass; existing centered message-box tests are unchanged.

- [ ] **Step 8: Commit shared theme and resource management**

```powershell
git add Cargo.toml src/windows/mod.rs src/windows/theme.rs src/windows/native/platform.rs src/windows/profile_dialog/platform.rs
git commit -m "feat: Theme native profile dialogs"
```

---

### Task 4: Replace the Flat List with Native Refined Owner-Draw Rows

**Files:**
- Modify: `src/windows/profile_dialog.rs:620-805`
- Modify: `src/windows/profile_dialog/platform.rs:1-70,650-790,950-1100`
- Modify: `tests/windows_app.rs:860-930`

**Interfaces:**
- Consumes: typed profile usage from Task 1, palette/layout from Task 2, and resources from Task 3.
- Produces: `ProfileManagerRowText`, `profile_manager_row_text`, `ProfileDialogController::profile_at`, and the `WM_DRAWITEM` row renderer used by the finished manager.

- [ ] **Step 1: Add failing row-copy tests for default/current markers**

Add to `tests/windows_app.rs`:

```rust
#[test]
fn profile_row_copy_marks_system_and_current_without_duplicate_default_text() {
    let mut system = system_profile_view();
    system.selected = true;
    let copy = profile_manager_row_text(&system, Language::Korean);
    assert_eq!(copy.name, system.label);
    assert!(copy.markers.contains(&localized_text(LocalizationKey::UsageProfileDisplayed, Language::Korean)));
    assert!(!copy.markers.windows(2).any(|pair| pair[0] == pair[1]));
}

#[test]
fn profile_row_copy_preserves_existing_safe_summary() {
    let mut profile = system_profile_view();
    profile.summary = "남은 사용량 표시: 81%".into();
    assert_eq!(profile_manager_row_text(&profile, Language::Korean).summary, profile.summary);
}
```

- [ ] **Step 2: Run the row-copy tests and verify RED**

Run:

```powershell
cargo test --test windows_app profile_row_copy -- --nocapture
```

Expected: compile failure because `ProfileManagerRowText` and helper do not
exist.

- [ ] **Step 3: Implement the pure row-copy contract**

Add with Korean rustdoc:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileManagerRowText {
    pub name: String,
    pub summary: String,
    pub markers: Vec<&'static str>,
}

pub fn profile_manager_row_text(
    profile: &UsageProfileView,
    language: Language,
) -> ProfileManagerRowText;
```

Use `UsageProfileSystem` for the system role and `UsageProfileDisplayed` for the
current row. If the profile name already equals the localized system label, omit
the duplicate system marker. Preserve `profile_manager_row_label` for any
existing caller until all call sites are intentionally migrated.

Add `ProfileDialogController::profile_at(index) -> Option<&UsageProfileView>` so
painting uses item identity without copying authentication or path information.

- [ ] **Step 4: Convert the list box to owner-draw-fixed**

Create the list with `LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOTIFY |
LBS_NOINTEGRALHEIGHT`. Set `LB_SETITEMHEIGHT` to the DPI-scaled 56-pixel row.
Continue adding one harmless display string per profile so list count,
selection, scrolling, accessibility, and keyboard navigation remain native.

- [ ] **Step 5: Implement `WM_DRAWITEM` row painting**

In a narrowly scoped unsafe helper:

```rust
unsafe fn draw_profile_row(
    item: &DRAWITEMSTRUCT,
    profile: &UsageProfileView,
    copy: &ProfileManagerRowText,
    visuals: &DialogVisualResources,
    rtl: bool,
) -> io::Result<()>;
```

Fill neutral or selected surface; draw a 3-logical-pixel selection edge; draw
name and compact role markers on the first line; draw summary on the second line;
draw the thin track and status fill only when both `used_percent` and
`usage_status` exist. Use `DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX`, add
`DT_RTLREADING | DT_RIGHT` for Arabic, and draw a focus rectangle when
`ODS_FOCUS` is present. Clamp the fill width even though the model is already a
`u8`.

- [ ] **Step 6: Add a pure draw-decision test seam**

Extract `profile_row_visual_state(profile, selected, focused)` returning palette
role, optional `(percent,status)`, and marker flags. Test healthy/warning/critical,
selected/unselected, logged-out `None`, and clamping. This avoids pixel-snapshot
tests while covering every state passed into GDI.

- [ ] **Step 7: Run row and interaction regressions**

Run:

```powershell
cargo test profile_row --lib
cargo test --test windows_app profile_row
cargo test --test windows_app profile_dialog_controls_follow_the_selected_profile_state -- --exact
cargo test --test windows_app profile_dialog_actions_use_the_current_selection_without_stale_identity -- --exact
```

Expected: all pass; list selection still updates edit text and action enablement.

- [ ] **Step 8: Commit owner-draw rows**

```powershell
git add src/windows/profile_dialog.rs src/windows/profile_dialog/platform.rs tests/windows_app.rs
git commit -m "feat: Refine usage profile list rows"
```

---

### Task 5: Apply Responsive Layout and Single-Line Actions to Both Dialogs

**Files:**
- Modify: `src/windows/profile_dialog/platform.rs:460-950,1000-1320`
- Modify: `src/windows/profile_dialog.rs:290-340`
- Modify: `tests/windows_app.rs:70-115,200-230,320-355`

**Interfaces:**
- Consumes: layout functions from Task 2, visual resources from Task 3, and owner-draw list from Task 4.
- Produces: DPI/theme-responsive manager and add dialogs with measured single-line actions and preserved modal behavior.

- [ ] **Step 1: Add failing localized button-copy and layout invariants**

Add a `profile_dialog_button_labels(language)` contract test to
`tests/windows_app.rs`. It must obtain the four manager actions plus Add and
Cancel in their actual runtime order, reject embedded CR/LF, and feed a
deterministic `utf16_count * 8` logical-pixel extent into the Task 2 layout for
every supported locale and DPI:

```rust
for language in Language::ALL {
    let labels = profile_dialog_button_labels(language);
    assert!(labels.iter().all(|label| !label.contains('\r') && !label.contains('\n')));
    for dpi in [96, 120, 144, 168, 192] {
        let measured = labels.map(|label| label.encode_utf16().count() as i32 * scale_logical(8, dpi));
        let layout = profile_manager_layout(DialogLayoutInput::new(620, dpi, language == Language::Arabic, measured[..4].try_into().unwrap()));
        assert!(layout.action_buttons.iter().zip(measured[..4].iter()).all(|(rect, text)| {
            rect.width() >= button_width(*text, dpi)
        }));
        assert!(layout.action_buttons.iter().all(|rect| rect.height() >= scale_logical(32, dpi)));
    }
}
```

Assert action rows are either one or two and never overlap.

- [ ] **Step 2: Run the layout tests and verify RED**

Run:

```powershell
cargo test --test windows_app profile_dialog_button_labels
```

Expected: compile failure because `profile_dialog_button_labels` does not exist.

- [ ] **Step 3: Implement the single source of localized button copy**

Add with Korean rustdoc:

```rust
pub fn profile_dialog_button_labels(language: Language) -> [&'static str; 6] {
    [
        localized_text(LocalizationKey::UsageProfileRename, language),
        localized_text(LocalizationKey::UsageProfileLogin, language),
        localized_text(LocalizationKey::UsageProfileLogout, language),
        localized_text(LocalizationKey::UsageProfileDelete, language),
        localized_text(LocalizationKey::MenuAddUsageProfile, language),
        localized_text(LocalizationKey::UsageProfileCancel, language),
    ]
}
```

Use this function in the platform module rather than looking up button strings a
second time. Rerun the focused test and expect PASS.

- [ ] **Step 4: Measure text and size the outer windows before showing**

Add `measure_text_width(HDC, HFONT, &str) -> io::Result<i32>` using
`GetTextExtentPoint32W`. Measure Rename, Login, Logout, Delete, Add, and Cancel
with the active body font. Pass those widths to the pure layouts. Use
`AdjustWindowRectExForDpi` to convert required client size to outer size, cap it
to the selected monitor work area, then reuse the existing centering functions.

Do not add `BS_MULTILINE` to any style. If the work area is narrow, request the
two-row pure layout; never reduce a button below its measured width plus padding.

- [ ] **Step 5: Replace fixed manager coordinates**

Remove `manager_control_layout` and the fixed 650-by-410 assumptions. Create
controls from the computed layout, preserving IDs and tab order. Place the
profile-name label above a full-width edit. Keep the 32-by-32 add control below
the list. Use `SetWindowPos` for relayout so existing HWND identity, focus, and
tooltips survive DPI/theme changes.

- [ ] **Step 6: Replace fixed add-dialog coordinates**

Use `add_profile_layout` for label-above-edit and trailing Add/Cancel buttons.
Preserve initial edit focus, `EM_SETLIMITTEXT`, Enter submit, Escape/X cancel,
validation warning, manager owner disabling, and manager restoration.

- [ ] **Step 7: Handle `WM_DPICHANGED` and RTL**

For both window procedures, on `WM_DPICHANGED`:

1. Copy the suggested `RECT` from `lparam`.
2. Call `SetWindowPos` with that rectangle.
3. Rebuild fonts/brushes for `HIWORD(wparam)` DPI.
4. Re-measure localized button strings.
5. Recompute layout and move every child.
6. Set the owner-draw row height and invalidate the dialog.

Use `WS_EX_LAYOUTRTL | WS_EX_NOINHERITLAYOUT` for Arabic dialog roots and
direction-aware draw flags. Verify the pure layout mirrors the selection edge,
progress origin, and trailing actions rather than manually negating individual
coordinates in multiple places.

- [ ] **Step 8: Paint surfaces and narrowly scoped primary buttons**

Handle `WM_ERASEBKGND`/`WM_CTLCOLORSTATIC`/`WM_CTLCOLOREDIT` with palette brushes.
Use `NM_CUSTOMDRAW` or owner-draw only for enabled Login in the manager and
enabled Add in the add dialog. Draw healthy green, pressed/hover variants,
single-line centered text, and native focus cues. Disabled primary actions use
neutral gray. Rename, Logout, Delete, Cancel, and `+` remain native neutral
buttons.

- [ ] **Step 9: Run all dialog behavior tests**

Run:

```powershell
cargo test windows::design::tests --lib
cargo test windows::profile_dialog --lib
cargo test --test windows_app profile_dialog_button_labels
cargo test --test windows_app profile_dialog
cargo test --test windows_app add_profile
cargo test --test windows_app centered
```

Expected: layout invariants and all existing modal, action, validation,
centering, and reentrancy tests pass.

- [ ] **Step 10: Commit responsive dialogs**

```powershell
git add src/windows/design.rs src/windows/profile_dialog.rs src/windows/profile_dialog/platform.rs tests/windows_app.rs
git commit -m "feat: Refine native profile dialogs"
```

---

### Task 6: Document Manual UI Verification

**Files:**
- Modify: `docs/RELEASE_CHECKLIST.md`
- Verify: `design.md`
- Verify: `docs/superpowers/specs/2026-07-29-profile-manager-native-refined-design.md`

**Interfaces:**
- Consumes: completed manager and add dialog behavior from Tasks 1–5.
- Produces: a repeatable manual release gate; no runtime API.

- [ ] **Step 1: Add explicit profile-dialog checklist items**

Add unchecked entries under the Windows UI section:

```markdown
- [ ] 사용량 프로필 관리/추가 창이 Windows 10/11 밝은·어두운 모드에서 같은 Native Refined 토큰을 사용한다.
- [ ] 100/125/150/175/200% DPI에서 목록, 입력 필드, `+`, 작업 버튼이 겹치거나 잘리지 않는다.
- [ ] 12개 지원 언어에서 모든 버튼 문구가 버튼 내부 한 줄로 표시된다.
- [ ] 좁은 작업 영역에서 작업 버튼 전체가 두 행으로 이동하며 버튼 내부 문구는 줄바꿈되지 않는다.
- [ ] 아랍어에서 목록 선택선, 텍스트, 진행률, 작업 버튼 정렬이 RTL로 반전된다.
- [ ] 키보드 Tab/Shift+Tab, 방향키, Enter, Escape 및 포커스 표시가 네이티브 규칙대로 동작한다.
- [ ] 정상/주의/위험/로딩/로그인 필요/오류 상태가 색상만 의존하지 않고 구분된다.
- [ ] 다중 모니터, 작업표시줄 자동 숨김, Explorer 재시작 후에도 두 창이 올바른 화면 중앙에 열린다.
```

- [ ] **Step 2: Verify documentation consistency**

Run:

```powershell
rg -n "70%|90%|single.line|한 줄|96|120|144|168|192|Arabic|아랍어" design.md docs/superpowers/specs/2026-07-29-profile-manager-native-refined-design.md docs/RELEASE_CHECKLIST.md
```

Expected: thresholds, DPI cases, RTL, and single-line constraints appear in the
design system, approved spec, and manual checklist without contradictory values.

- [ ] **Step 3: Commit the release checklist**

```powershell
git add docs/RELEASE_CHECKLIST.md
git commit -m "docs: Add native profile dialog checks"
```

---

### Task 7: Run Full Automated Verification and Record Manual Gaps

**Files:**
- Review: all files changed in Tasks 1–6
- Do not modify: `target/`, local settings, authentication files, or diagnostic logs

**Interfaces:**
- Consumes: the completed feature and documentation.
- Produces: evidence that the branch is ready for Windows manual verification.

- [ ] **Step 1: Check formatting**

Run:

```powershell
cargo fmt --all -- --check
```

Expected: exit code 0.

- [ ] **Step 2: Run all tests**

Run:

```powershell
cargo test --all-targets
```

Expected: exit code 0 with no test depending on a real Codex account or wall-clock
delay.

- [ ] **Step 3: Run Clippy with warnings denied**

Run:

```powershell
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: exit code 0.

- [ ] **Step 4: Build the release executable**

Run:

```powershell
cargo build --release
```

Expected: exit code 0 and `target/release/codex-peek.exe` produced without
modifying or committing `target/`.

- [ ] **Step 5: Check whitespace and scope**

Run:

```powershell
git diff --check
git status --short
git diff --stat
```

Expected: no whitespace errors, no authentication/settings/log files, and no
unrelated refactor.

- [ ] **Step 6: Perform available Windows visual checks**

Run the release executable and follow the new `docs/RELEASE_CHECKLIST.md` items.
Capture exact failures by Windows version, DPI, locale, and theme. If the current
environment cannot cover Windows 10, multiple monitors, or every DPI, report
those items as unverified; do not mark them passed by inference.

- [ ] **Step 7: Commit only a necessary verification fix**

If verification required a source correction, rerun the focused failing test and
the complete gate, then commit only that correction:

For example, if only the native renderer and its contract test changed:

```powershell
git add src/windows/profile_dialog/platform.rs tests/windows_app.rs
git commit -m "fix: Correct native profile dialog rendering"
```

If no correction was required, do not create an empty commit.

- [ ] **Step 8: Confirm final history and clean worktree**

Run:

```powershell
git status --short
git log -8 --oneline
```

Expected: clean worktree, one logical commit per completed task, and all manual
checks either explicitly passed or explicitly listed as outstanding.
