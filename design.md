# CodexPeek Design System

## Design context

- Version: 1.0
- Product: CodexPeek
- Product type: Native Windows utility
- Platforms: Windows 10 and Windows 11
- Primary font: `"Segoe UI Variable", "Segoe UI", system-ui, sans-serif`
- Fallback font: `"Malgun Gothic", "Noto Sans", sans-serif`

CodexPeek is a compact native Windows utility that shows Codex rate-limit usage,
remaining allowance, reset times, loading state, and connection errors through a
taskbar widget, floating widget, and system tray.

The design principles are glanceable, compact, native, calm, status-driven,
content-first, accessible, and DPI-aware. The product personality is quiet,
precise, technical, dependable, and unobtrusive.

## Brand and color tokens

The brand primary color is healthy green `#48C774`. It means that Codex usage is
healthy and available; it is not a general decoration color.

### Status colors

| State | Color | Meaning |
| --- | --- | --- |
| Healthy | `#48C774` | Displayed usage is below 70% |
| Warning | `#F5A623` | Displayed usage is at least 70% and below 90% |
| Critical | `#FF5C5C` | Displayed usage is at least 90% |
| Error | `#FF5C5C` | The latest refresh failed |
| Loading | `#979797` | No successful value has loaded yet |

### Dark theme

| Token | Value |
| --- | --- |
| Background | `#1F1F1F` |
| Surface | `#262626` |
| Elevated surface | `#2C2C2C` |
| Border | `#404040` |
| Subtle border | `rgba(255, 255, 255, 0.08)` |
| Text | `#EEEEEE` |
| Secondary text | `#C8C8C8` |
| Muted text | `#979797` |
| Progress track | `rgba(255, 255, 255, 0.14)` |
| Hover | `rgba(255, 255, 255, 0.06)` |
| Pressed | `rgba(255, 255, 255, 0.10)` |
| Focus | `#48C774` |

### Light theme

| Token | Value |
| --- | --- |
| Background | `#F3F3F3` |
| Surface | `#FFFFFF` |
| Elevated surface | `#FAFAFA` |
| Border | `#D5D5D5` |
| Subtle border | `rgba(0, 0, 0, 0.08)` |
| Text | `#202020` |
| Secondary text | `#505050` |
| Muted text | `#737373` |
| Progress track | `rgba(0, 0, 0, 0.14)` |
| Hover | `rgba(0, 0, 0, 0.05)` |
| Pressed | `rgba(0, 0, 0, 0.09)` |
| Focus | `#27864D` |

## Typography

| Style | Size | Weight | Line height | Purpose |
| --- | --- | --- | --- | --- |
| Percentage | 13 px | 600 | 1 | Primary taskbar usage value; tabular numbers |
| Label | 11 px | 500 | 1.15 | Usage-window labels and compact metadata |
| Body | 12 px | 400 | 1.35 | Floating widget and diagnostics content |
| Heading | 14 px | 600 | 1.25 | Section headings |
| Caption | 10 px | 400 | 1.2 | Reset and update times, secondary information |

Percentages use tabular numeric alignment. Text must stay readable before any
decorative element is preserved.

## Geometry and motion tokens

- Base spacing: 2 px
- Spacing scale: 2, 4, 6, 8, 10, 12, 16, 20, 24 px
- Corner radii: 3 px small, 6 px medium, 10 px large, 12 px floating widget,
  and 9999 px pill
- Borders: 1 px standard, 2 px focus, 1 px status ring
- Taskbar shadow: none
- Floating widget shadow: `0 8px 24px rgba(0, 0, 0, 0.22)`
- Popup shadow: `0 4px 16px rgba(0, 0, 0, 0.20)`
- Inset highlight: `inset 0 1px 0 rgba(255, 255, 255, 0.06)`
- Hover duration: 150 ms
- State duration: 180 ms
- Popup duration: 200 ms
- Progress duration: 240 ms
- Easing: `cubic-bezier(0.2, 0, 0, 1)`
- Reduced-motion duration: 0 ms

Motion exists only to clarify feedback. Never use blinking, bouncing, pulsing,
continuous shimmer, or animations that delay access to information.

## Product intent and information hierarchy

CodexPeek is not a dashboard or a general-purpose monitoring suite. It quietly
answers three questions within one second:

1. How much Codex usage has been consumed or remains?
2. Is the state healthy, warning, critical, loading, or failed?
3. When will the relevant usage window reset?

Prioritize content in this order:

1. Percentage or remaining allowance
2. Current health state
3. Progress bar
4. Usage-window identity
5. Reset time
6. Last updated time
7. Diagnostics and explanatory text

When space is limited, remove lower-priority information instead of shrinking an
essential value until it is unreadable.

## Taskbar widget

- Preferred width: 208 logical pixels
- Minimum width: 88 logical pixels
- Horizontal padding: 11 px
- Status dot: 6 px
- Progress height: 3 px
- Element gap: 8 px

The responsive modes are:

- Full, at 140 logical pixels or wider: status dot, usage-window label,
  percentage, and progress bar.
- Compact, at 100 logical pixels or wider: status dot, percentage, and progress
  bar.
- Minimal, at 88 logical pixels or wider: percentage and progress bar.

Follow the native Windows taskbar appearance and allow its material to remain
visible. Do not draw a large opaque card. Widget dimensions must not change on
hover, loading, or status changes. Preserve alignment when percentages change
from one to three digits. Prefer one-line content and truncate optional labels
before reducing percentage readability.

The taskbar widget must not contain explanatory paragraphs, multiple buttons,
large icons, card grids, illustrations, gradients, glow, promotional text, or
continuously moving animations.

## Floating widget

- Preferred width: 280 logical pixels
- Minimum width: 240 logical pixels
- Maximum width: 340 logical pixels
- Padding: 16 px
- Section gap: 12 px
- Row gap: 8 px

Its structure is account or connection status, primary usage window, secondary
usage window, reset times, last-updated time, refresh action, and an error or
diagnostic message only when needed.

Use one quiet surface, one border, and a soft shadow. Keep percentages strongest
and reset times secondary. Do not imitate a web dashboard or put every value in
its own card. Avoid gradients and fake glass effects.

## Native dialogs

Dialog layouts use the same typography, colors, spacing scale, and semantic
status model as the widgets while preserving Win32 behavior.

- Use 16 logical pixels of outer padding and gaps from the shared spacing scale.
- Interactive controls have at least a 32 by 32 logical-pixel target.
- Place form labels above their inputs.
- Keep action labels on exactly one line. Do not enable multiline button styles.
- Measure localized action text using the active dialog font and add horizontal
  padding before choosing each button width.
- If the work area cannot hold one action row, wrap the action controls as whole
  buttons; never wrap or clip text inside a button.
- Use owner drawing only where native controls cannot express the selected row,
  semantic progress, or clear primary action. Preserve native focus, keyboard,
  accessibility, and disabled behavior.
- Keep system message boxes native.

### Usage profile manager

The profile manager uses the Native Refined direction:

- Use 56 logical-pixel profile rows with 16 px horizontal padding.
- The selected row has a subtle theme-appropriate green tint and a 3 px green
  selection bar.
- The first line is the profile name. The second line is the existing localized
  usage, loading, unavailable, or login-required summary.
- When valid usage exists, show a thin semantic progress indicator at the edge
  appropriate for the current layout direction.
- Indicate the system/default account and currently displayed account with text,
  not color alone.
- Place the 32 by 32 logical-pixel `+` control directly below the list and retain
  its localized tooltip/accessibility description.
- Keep profile name editing below the list with a label above the edit field.
- Keep Rename, Login, Logout, and Delete in a restrained action area. Only a
  clearly primary Login action may use healthy green; destructive and disabled
  controls remain neutral.

The add-profile dialog uses the same surface, font, spacing, control height,
button sizing, theme, DPI, RTL, focus, and validation rules. Add is the primary
green action when enabled; Cancel is neutral.

## System tray

Use native Windows menu behavior and spacing. Labels are explicit, related
settings are separated into groups, and selected values use native checked menu
states. Do not custom-render dark menus unless Windows provides the behavior.

Recommended groups are refresh, refresh interval, widget visibility, remaining
or used display mode, monitor placement, startup settings, authentication,
language, diagnostics and updates, and exit.

## State presentation

### Healthy

Use `#48C774` for status dots and progress. Keep the surface neutral and calm.

### Warning

Use `#F5A623` when displayed usage reaches 70%. Change semantic indicators only;
do not recolor the entire interface or interrupt the user.

### Critical

Use `#FF5C5C` when displayed usage reaches 90%. Make reset timing slightly more
prominent where it is present. Never flash or pulse.

### Loading

Use neutral `#979797`, reserve the final layout dimensions, and never show a fake
zero percentage.

### Error

Use a concise error indicator. Preserve the last successful values when
available and mark them stale. Put safe detail outside the taskbar widget. Never
expose tokens, account IDs, authentication files, or proxy values.

## Progress bars and icons

Taskbar progress height is 3 px and floating progress height is 6 px. Clamp the
visual percentage to 0 through 100. Use the current status color for fill and the
theme progress-track color for the remainder. Avoid gradients, stripes, glow,
and shimmer. Animate only when a newly fetched value changes, never on repaint.

Icons are simple, geometric, single-weight, and Windows-compatible. Use 6 px for
taskbar status, 12 px inline, 16 px for actions, and Windows-defined tray sizes.
Do not use emoji as production icons or mix outline and filled styles.

## Interaction and accessibility

- Hover may subtly brighten a surface within about 150 ms but must never resize
  or shift content.
- The taskbar widget opens or focuses the floating detail widget.
- Refresh requests a manual refresh.
- Tray interactions follow platform conventions.
- Provide visible keyboard focus, support Escape where conventional, do not trap
  focus, and preserve native menu keyboard navigation.
- Respect Windows reduced-motion preferences when accessible.
- Target WCAG AA-level contrast where applicable.
- Pair every status color with text or an icon.
- Add a contrasting ring when a status dot could blend into the taskbar.
- Tooltips must not be the only place where essential information is available.
- Loading and errors must remain understandable without animation.

## Windows integration and localization

- Use native Win32 behavior; do not add Electron, Tauri, WebView, or a browser UI.
- Preserve the Rust and Win32 architecture solely for styling.
- Respect Windows light/dark appearance and per-monitor DPI awareness.
- Test at 100%, 125%, 150%, 175%, and 200% scaling.
- Support primary-monitor-only and all-monitor taskbar modes.
- Survive Explorer restarts and taskbar changes without visual corruption.
- Avoid unsupported Acrylic or Mica simulations.
- Maintain low memory usage and negligible idle CPU consumption.
- Support Korean, English, Spanish, Portuguese, Indonesian, Japanese, Hindi,
  German, French, Vietnamese, Turkish, and Arabic.
- Do not assume translations have English widths. Fall back from Full to Compact
  or Minimal before clipping essential taskbar values.
- Respect right-to-left layout for Arabic and never put essential meaning in an
  untranslated image.

## Implementation constraints

1. Inspect existing Rust and Win32 code before changing UI.
2. Preserve taskbar, floating widget, tray, polling, localization, diagnostics,
   and Explorer-recovery behavior.
3. Use Healthy below 70%, Warning from 70% to below 90%, and Critical from 90%.
4. Preserve existing responsive taskbar layout modes.
5. Centralize colors, spacing, fonts, dimensions, and motion durations where
   practical; do not scatter raw RGB and pixel constants.
6. Do not introduce a browser framework or replace the native architecture.
7. Keep idle resource use effectively unchanged.
8. Preserve the last successful values during transient refresh failure.
9. Keep sensitive information out of UI, logs, tooltips, and diagnostics.
10. Prefer the smallest coherent UI change and preserve behavior unless the
    request explicitly changes it.

The repository currently has a more granular legacy `UsageLevel` model. A
component-specific visual may use the three design-system states without
silently changing established behavior elsewhere; any future global threshold
migration requires its own product decision and domain tests.

## Acceptance criteria

A UI change is complete only when:

- State is understandable within one second and without color alone.
- Healthy, warning, critical, loading, and error are visually distinct.
- Percentage position remains stable as digit count changes.
- Hover does not resize or shift content, and taskbar icons are not overlapped.
- Full, Compact, and Minimal modes remain usable.
- Light and dark Windows appearances are supported.
- Common DPI scales render correctly and long translations do not clip essential
  values.
- Arabic and other right-to-left content does not corrupt layout.
- Loading never displays fake zero and refresh failure preserves valid values.
- No sensitive account or authentication data is exposed.
- Existing tests pass.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test --all-targets` passes.
- `cargo build --release` succeeds.
- Final behavior is manually checked on Windows for taskbar, floating widget,
  tray menu, profile dialogs, common DPI levels, and Explorer restart.

Use this design system for every CodexPeek UI change. Before implementation,
identify the affected component and state, identify the applicable tokens and
layout rules, preserve unrelated behavior, make the smallest coherent change,
and verify every relevant state, theme, DPI level, locale, and layout mode.
