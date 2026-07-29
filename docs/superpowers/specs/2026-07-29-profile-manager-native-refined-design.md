# Usage Profile Dialog Native Refined Design

## Status

- Approved: 2026-07-29
- Platform: Windows 10/11 x64 native Win32
- Applies to: usage profile manager and add-profile dialog
- Design-system source: [`design.md`](../../../design.md)

## Problem analysis

The profile manager currently uses fixed physical coordinates, a default
single-line list box, a gray label block beside the name field, and uniformly
sized action buttons. It works, but it does not communicate row hierarchy,
selection, usage status, or account role clearly. Fixed widths also risk clipping
long translations and high-DPI text.

The goal is to make both profile dialogs calm, compact, and native while keeping
all profile operations, authentication boundaries, modal behavior, and CLI/IDE
state unchanged.

## Assumptions and risks

- This is a visual and layout change. It does not change profile persistence,
  login/logout semantics, Codex CLI or IDE authentication, polling, or tray
  commands.
- Native message boxes remain unchanged.
- The existing global `UsageLevel` model has legacy 50/75/90 boundaries. The
  manager uses a local `ProfileUsageStatus` with the approved 70/90 visual
  thresholds so this work does not silently alter taskbar behavior.
- Owner drawing increases Win32 resource-lifetime and DPI complexity. Custom
  painting is therefore limited to list rows and the clearly primary action;
  edit controls and ordinary buttons retain native behavior.
- Long translations cannot always fit one action row on a small monitor. Whole
  buttons may move to a second row, but their labels never wrap or clip.

## Chosen direction

Use the approved “A — Native Refined” direction for both dialogs.

### Shared tokens

- 16 logical pixels outer padding
- 6–12 logical pixels between related controls
- 32 logical pixels minimum interactive height and click target
- `Segoe UI Variable`, with `Segoe UI` fallback
- light/dark palettes and semantic colors from `design.md`
- logical-to-physical scaling using the current window DPI

### Profile manager

- Start at approximately 620 logical pixels of client width.
- Show three 56-logical-pixel rows before scrolling.
- Draw each row with a primary name line and secondary existing summary line.
- Mark the selected row with a subtle green-tinted surface and a 3-logical-pixel
  green selection edge.
- Show system/default and currently displayed roles through localized text.
- Draw a thin progress line only when valid usage exists. The fill represents
  consumed usage, while the existing second-line summary continues to state
  remaining allowance.
- Put the 32-by-32 `+` button directly below the list and keep its localized
  tooltip/accessibility description.
- Put the profile-name label above the edit control.
- Right-align Rename, Login, Logout, and Delete in a quiet footer. Login may be
  the green primary action when enabled. Delete and disabled actions remain
  neutral.
- Keep `X` and Escape as close mechanisms; do not restore a Close button.

### Add-profile dialog

- Use the same surface, typography, spacing, 32-pixel control height, dynamic
  button widths, focus treatment, and light/dark behavior.
- Put the name label above the edit.
- Right-align Add and Cancel. Add is the green primary action when enabled;
  Cancel is neutral.
- Preserve current validation, Enter, Escape, owner disabling, and centered
  warning behavior.

## Architecture

### Pure presentation model

Extend `UsageProfileView` with `used_percent: Option<u8>` and
`usage_status: Option<ProfileUsageStatus>`. Populate both directly from the
latest usable snapshot in `usage_profile_views`; never parse the localized
summary string. Clamp only the rendered percentage to 0–100. Login-required,
initial loading, and unavailable rows have no progress. A transient error may
still show progress when the snapshot retains a last successful usage value.

`ProfileUsageStatus` is a UI semantic enum:

```rust
pub enum ProfileUsageStatus {
    Healthy,
    Warning,
    Critical,
}
```

It maps consumed usage below 70 to Healthy, 70 through below 90 to Warning, and
90 or more to Critical.

### Reusable native design module

Create `src/windows/design.rs` for dialog-oriented colors, logical dimensions,
font description, status mapping, and pure layout geometry. This module owns no
HWND or GDI resources and performs no I/O. Add `src/windows/theme.rs` as the one
read-only adapter for the Windows light/dark registry preference instead of
duplicating that lookup. `src/windows/profile_dialog/platform.rs` owns handles,
brushes, fonts, painting, and cleanup. Existing taskbar colors and thresholds
remain unchanged.

### DPI and layout

Calculate manager and add-dialog rectangles in logical pixels, then scale using
the current window DPI. Button widths come from measured localized text plus
theme padding and a minimum width. No button uses `BS_MULTILINE`.

On `WM_DPICHANGED`, accept the Windows suggested outer rectangle, rebuild the
dialog font and size-dependent GDI resources, and relayout every child. On
`WM_SETTINGCHANGE` and `WM_THEMECHANGED`, refresh the palette and repaint.

If all manager actions do not fit, place whole buttons in two right-aligned rows.
The dialog remains within the selected monitor work area. Arabic mirrors text,
row accents, progress placement, and action alignment.

### Rendering and resource safety

Use an owner-draw list box with fixed DPI-scaled row height. `WM_DRAWITEM`
renders the row background, selection edge, name, role markers, summary, and
progress line. Native selection, focus, scrolling, and keyboard navigation are
preserved.

Use native edit controls and native buttons by default. A narrowly scoped button
custom-draw path may paint the enabled Login/Add primary action. It must retain
focus cues, pressed, hover, disabled, and keyboard behavior. Decorative resource
creation failures fall back to stock fonts/colors and must not prevent the dialog
from opening. All owned GDI objects are deleted once during window teardown.

## Accessibility and localization

- Status is communicated by the existing summary text as well as progress color.
- Selected, system/default, and currently displayed states have text markers.
- The `+` button keeps its localized tooltip/accessibility description.
- Tab order, arrow-key list navigation, Enter, Escape, and visible focus remain
  native.
- All 12 supported languages are measured using their actual strings.
- Arabic receives mirrored geometry and RTL text flags.
- Essential button text remains on one line at 100, 125, 150, 175, and 200% DPI.

## Error and security boundaries

- Do not read, parse, show, or log `auth.json` content.
- Do not expose tokens, account IDs, email addresses, paths, proxy values, or raw
  RPC payloads.
- Do not perform settings, login, logout, filesystem, or RPC I/O on the UI thread.
- Preserve the last successful usage value on transient refresh failure.
- Preserve modal cleanup, owner restoration, reentrancy guards, and centered
  message-box behavior.

## Verification

Automated coverage includes:

- 70/90 status boundaries and 0–100 visual clamping
- no progress for login, initial loading, and unavailable rows
- retained progress when a last successful snapshot exists
- layout at 96, 120, 144, 168, and 192 DPI
- text-measured one-line buttons and two-row whole-button fallback
- no overlap among list, add control, form field, and action rows
- mirrored Arabic row and action geometry
- light/dark palette mappings and selected-row presentation contracts
- existing selection, actions, add validation, modal lifecycle, centering, and
  reentrancy tests

Run:

```powershell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
git diff --check
```

Manually verify Windows 10/11, light/dark appearance, 100/125/150/175/200% DPI,
keyboard and focus behavior, all long translations, Arabic RTL, multiple
monitors, auto-hidden taskbar, Explorer restart, missing CLI, logged-out rows,
loading, stale data, and profile-limit states using `docs/RELEASE_CHECKLIST.md`.

## Acceptance criteria

- Both dialogs visibly follow the same Native Refined system.
- The account list communicates name, summary, selection, role, and available
  usage at a glance without looking like a web dashboard.
- Button labels never wrap or clip.
- Long translations and common DPI levels do not overlap controls.
- The dialogs follow Windows light/dark appearance and Arabic layout direction.
- Login/loading/error/unavailable states remain understandable without color.
- Existing profile actions and authentication/security boundaries are unchanged.
- Native message boxes remain native and centered.
- Automated checks pass; outstanding manual Windows checks are reported rather
  than assumed.
