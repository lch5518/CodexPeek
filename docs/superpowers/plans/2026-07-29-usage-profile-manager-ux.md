# Usage Profile Manager UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the duplicate tray add command, add profiles through a dedicated `+` prompt in the manager, allow a persisted custom system-profile name, and simplify manager close controls.

**Architecture:** Extend `UsageProfileCatalog` with an optional, backward-compatible system label and continue routing every rename through the single settings writer. Keep the existing modal “return one typed action, perform I/O on workers” boundary: the profile manager opens a small owned add prompt, while tray rendering and manager-only row formatting remain pure testable functions.

**Tech Stack:** Rust 2021, minimum Rust 1.85, `serde`, existing `windows` Win32 bindings, native GDI/controls, deterministic Rust unit and integration tests.

## Global Constraints

- Support Windows 10/11 x64 and preserve the existing taskbar, tray, floating-widget, Explorer-recovery and DPI behavior.
- Do not add dependencies; use the standard library and existing `windows`, `serde`, and project modules.
- Never open, read, parse, copy, retain, or log any system or managed profile `auth.json` content.
- Never log profile labels, managed paths, account IDs, email addresses, tokens, raw RPC payloads, or environment values.
- Keep terminal, IDE, Codex app, WSL, Remote SSH and Dev Container sign-in unchanged.
- Keep the maximum at eight profiles including the system profile; no automatic profile selection or rotation.
- Keep file, settings and Codex RPC I/O off the UI thread and preserve the single settings writer and serialized profile worker.
- Add or update every user-facing string in all 12 supported languages in `src/localization.rs`.
- Write Korean rustdoc for every new or modified public API and every complex state/I/O boundary.
- Use TDD: write each behavior test, observe the expected RED failure, implement the smallest production change, then rerun GREEN.

---

### Task 1: Persist and Validate a Custom System Profile Name

**Files:**
- Modify: `src/profiles.rs:23-243`
- Modify: `tests/profile_runtime.rs:760-880,1640-1690`
- Modify: `tests/config_runtime.rs:35-95,175-195`

**Interfaces:**
- Consumes: `UsageProfileId`, `normalize_profile_label`, current `UsageProfileCatalog::{add,rename,remove,validate}`.
- Produces: `UsageProfileCatalog::system_label(&self) -> Option<&str>` and system-aware `UsageProfileCatalog::rename`/uniqueness validation used by Tasks 2–5.

- [ ] **Step 1: Write failing catalog tests for default compatibility and system rename**

Add literal-behavior tests in `tests/profile_runtime.rs`:

```rust
#[test]
fn system_profile_name_is_optional_and_can_be_renamed_without_changing_identity() {
    let mut catalog = UsageProfileCatalog::default();
    assert_eq!(catalog.system_label(), None);

    catalog.rename(UsageProfileId::System, "  Main account  ").unwrap();

    assert_eq!(catalog.system_label(), Some("Main account"));
    assert_eq!(catalog.selected(), UsageProfileId::System);
    assert_eq!(
        catalog.remove(UsageProfileId::System),
        Err(ProfileValidationError::SystemProfileImmutable)
    );
}

#[test]
fn custom_system_name_and_managed_names_are_unique_case_insensitively() {
    let mut catalog = UsageProfileCatalog::default();
    catalog.rename(UsageProfileId::System, "Main").unwrap();
    assert_eq!(catalog.add("main"), Err(ProfileValidationError::DuplicateLabel));

    catalog.add("Work").unwrap();
    assert_eq!(
        catalog.rename(UsageProfileId::System, "WORK"),
        Err(ProfileValidationError::DuplicateLabel)
    );
}
```

Add `system_profile_label_defaults_for_v2_and_round_trips` in `tests/config_runtime.rs`. It deserializes a literal schema-v2 `usage_profiles` object without `system_label`, asserts `None`, renames system, saves, reloads and asserts `Some("Main")`.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test --test profile_runtime system_profile_name_is_optional_and_can_be_renamed_without_changing_identity -- --exact
cargo test --test profile_runtime custom_system_name_and_managed_names_are_unique_case_insensitively -- --exact
cargo test --test config_runtime system_profile_label_defaults_for_v2_and_round_trips -- --exact
```

Expected: compile failure because `system_label()` and the serialized field do not exist, or assertion failure because `rename(System, ...)` returns `SystemProfileImmutable`.

- [ ] **Step 3: Add the optional field and system-aware uniqueness validation**

Modify `UsageProfileCatalog`:

```rust
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UsageProfileCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system_label: Option<String>,
    managed: Vec<ManagedUsageProfile>,
    selected: UsageProfileId,
    next_sequence: u32,
}
```

Set `system_label: None` in `Default`. Add Korean-rustdoc getter:

```rust
pub fn system_label(&self) -> Option<&str> {
    self.system_label.as_deref()
}
```

Refactor label validation around the stable `UsageProfileId`, not just managed sequence:

```rust
fn validate_new_label(
    &self,
    label: &str,
    current: Option<UsageProfileId>,
) -> Result<String, ProfileValidationError> {
    let normalized = normalize_profile_label(label)?;
    let normalized_key = normalized.to_lowercase();
    let conflicts_with_system = current != Some(UsageProfileId::System)
        && self.system_label.as_deref().is_some_and(|system| {
            system.to_lowercase() == normalized_key
        });
    let conflicts_with_managed = self.managed.iter().any(|profile| {
        current != Some(profile.id()) && profile.label.to_lowercase() == normalized_key
    });
    if conflicts_with_system || conflicts_with_managed {
        return Err(ProfileValidationError::DuplicateLabel);
    }
    Ok(normalized)
}
```

Make `rename` set `system_label` for `System` and update the matching managed item for `Managed(sequence)`. Validate a stored `system_label` with `normalize_profile_label`, and reject case-insensitive collision with every managed label. Do not reserve the locale-dependent fallback when `system_label` is `None`.

- [ ] **Step 4: Run focused domain/settings tests and verify GREEN**

Run:

```powershell
cargo test --test profile_runtime system_profile_name_is_optional_and_can_be_renamed_without_changing_identity -- --exact
cargo test --test profile_runtime custom_system_name_and_managed_names_are_unique_case_insensitively -- --exact
cargo test --test config_runtime system_profile_label_defaults_for_v2_and_round_trips -- --exact
cargo test --test profile_runtime profile_labels
```

Expected: PASS. Existing system delete tests remain PASS.

- [ ] **Step 5: Commit the domain change**

```powershell
git add src/profiles.rs tests/profile_runtime.rs tests/config_runtime.rs
git commit -m "feat: Allow naming the system usage profile"
```

---

### Task 2: Use the Custom System Name and Remove the Tray Add Entry

**Files:**
- Modify: `src/app.rs:820-900,1090-1190`
- Modify: `src/windows/tray.rs:83-120`
- Modify: `src/windows/profile_dialog.rs:1-155,267-294`
- Modify: `tests/windows_app.rs:450-565,640-765`

**Interfaces:**
- Consumes: `UsageProfileCatalog::system_label()` from Task 1, `UsageProfileView`, `Language`, `TrayMenuModel`.
- Produces: `system_profile_display_label(settings, language) -> String` in `app.rs` and `profile_manager_row_label(profile, language) -> String` in `windows/profile_dialog.rs`.

- [ ] **Step 1: Write failing view/tray tests**

Update `tests/windows_app.rs` with explicit menu and manager-only-label expectations:

```rust
#[test]
fn usage_profile_submenu_offers_manage_but_not_duplicate_add() {
    let model = tray_menu_model(&tray_settings_with_profiles());
    let submenu = usage_profile_submenu(&model);
    let ids = submenu_command_ids(submenu);

    assert!(ids.contains(&MENU_MANAGE_USAGE_PROFILES));
    assert!(!ids.contains(&MENU_ADD_USAGE_PROFILE));
    assert_eq!(model.action(MENU_ADD_USAGE_PROFILE), None);
}

#[test]
fn manager_marks_only_the_custom_system_profile_as_default() {
    let system = UsageProfileView {
        id: UsageProfileId::System,
        label: "Main".to_owned(),
        summary: String::new(),
        selected: true,
        login_required: false,
        managed: false,
    };
    assert_eq!(
        profile_manager_row_label(&system, Language::English),
        "Main (Default Codex account)"
    );

    let managed = UsageProfileView { id: UsageProfileId::Managed(1), ..system.clone() };
    assert_eq!(profile_manager_row_label(&managed, Language::English), "Main");
}
```

Add `custom_system_profile_name_is_used_outside_manager` to the existing `src/app.rs` test module. It sets `catalog.rename(System, "Main")`, builds the UI snapshot, and asserts the tray/floating `usage_profile_label` is exactly `Main`, without the default-account suffix.

- [ ] **Step 2: Run focused UI tests and verify RED**

Run:

```powershell
cargo test --test windows_app usage_profile_submenu_offers_manage_but_not_duplicate_add -- --exact
cargo test --test windows_app manager_marks_only_the_custom_system_profile_as_default -- --exact
cargo test --lib app::tests::custom_system_profile_name_is_used_outside_manager -- --exact
```

Expected: tray assertion fails because add is present; manager helper is missing; app snapshot still uses the localized fixed system label.

- [ ] **Step 3: Centralize the system display label in `app.rs`**

Add:

```rust
fn system_profile_display_label(settings: &Settings, language: Language) -> String {
    settings
        .usage_profiles
        .system_label()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            localized_text(LocalizationKey::UsageProfileSystem, language).to_owned()
        })
}
```

Use this helper both when creating the system `UsageProfileView` and in `selected_usage_profile_label`. Do not append the default marker in taskbar, floating-widget or tray data.

- [ ] **Step 4: Remove the rendered add command and add the manager row helper**

Delete only the `push_command` block that adds `MENU_ADD_USAGE_PROFILE` to `profile_entries`. Keep dynamic IDs and `MENU_MANAGE_USAGE_PROFILES` unchanged.

Add in `windows/profile_dialog.rs`:

```rust
pub fn profile_manager_row_label(profile: &UsageProfileView, language: Language) -> String {
    if profile.id != UsageProfileId::System {
        return profile.label.clone();
    }
    let marker = localized_text(LocalizationKey::UsageProfileSystem, language);
    if profile.label == marker {
        profile.label.clone()
    } else {
        format!("{} ({marker})", profile.label)
    }
}
```

The native list renderer in Task 4 must consume this helper before appending `profile.summary`.

- [ ] **Step 5: Run focused UI tests and verify GREEN**

Run:

```powershell
cargo test --test windows_app usage_profile_submenu_offers_manage_but_not_duplicate_add -- --exact
cargo test --test windows_app manager_marks_only_the_custom_system_profile_as_default -- --exact
cargo test --lib app::tests::custom_system_profile_name_is_used_outside_manager -- --exact
```

Expected: PASS. Existing popup identity tests also pass.

- [ ] **Step 6: Commit the display/menu change**

```powershell
git add src/app.rs src/windows/tray.rs src/windows/profile_dialog.rs tests/windows_app.rs
git commit -m "feat: Simplify usage profile menu"
```

---

### Task 3: Enable System Rename and Define Add-Prompt State and Localization

**Files:**
- Modify: `src/windows/profile_dialog.rs:40-294`
- Modify: `src/localization.rs:130-340,430-620`
- Modify: `tests/windows_app.rs:50-325`
- Modify: `tests/localization_runtime.rs:45-90`

**Interfaces:**
- Consumes: `UsageProfileCatalog` behavior from Task 1 and manager row helper from Task 2.
- Produces: system-aware `available_profile_actions`, `AddProfilePromptCommand`, `add_profile_prompt_result`, and localization key `UsageProfileCancel` for Task 4.

- [ ] **Step 1: Write failing controller tests for system rename and prompt decisions**

Update `tests/windows_app.rs`:

```rust
#[test]
fn system_profile_offers_rename_but_not_logout_or_delete() {
    let actions = available_profile_actions(&system_profile_view());
    assert!(actions.contains(&ProfileDialogCommand::Rename));
    assert!(actions.contains(&ProfileDialogCommand::Login));
    assert!(!actions.contains(&ProfileDialogCommand::Logout));
    assert!(!actions.contains(&ProfileDialogCommand::Delete));
}

#[test]
fn add_prompt_cancel_emits_no_action_and_submit_validates_the_name() {
    assert_eq!(
        add_profile_prompt_result("Work", AddProfilePromptCommand::Cancel),
        Ok(None)
    );
    assert_eq!(
        add_profile_prompt_result("  Work  ", AddProfilePromptCommand::Submit),
        Ok(Some(ProfileDialogAction::Add("Work".to_owned())))
    );
    assert_eq!(
        add_profile_prompt_result("", AddProfilePromptCommand::Submit),
        Err(ProfileValidationError::InvalidLabel)
    );
}
```

Extend `tests/localization_runtime.rs` so `UsageProfileCancel` is required and non-empty in every language, with exact Korean `취소` and English `Cancel` expectations.

- [ ] **Step 2: Run the controller/localization tests and verify RED**

Run:

```powershell
cargo test --test windows_app system_profile_offers_rename_but_not_logout_or_delete -- --exact
cargo test --test windows_app add_prompt_cancel_emits_no_action_and_submit_validates_the_name -- --exact
cargo test --test localization_runtime
```

Expected: system rename assertion fails; add prompt types and cancel localization key do not exist.

- [ ] **Step 3: Implement pure prompt decisions and system action availability**

Add:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddProfilePromptCommand {
    Submit,
    Cancel,
}

pub fn add_profile_prompt_result(
    value: &str,
    command: AddProfilePromptCommand,
) -> Result<Option<ProfileDialogAction>, ProfileValidationError> {
    match command {
        AddProfilePromptCommand::Cancel => Ok(None),
        AddProfilePromptCommand::Submit => validated_label(value)
            .map(|label| Some(ProfileDialogAction::Add(label))),
    }
}
```

Update `available_profile_actions` so both system and managed profiles receive `Rename` and `Login`, while only managed profiles can receive `Logout` (when logged in) and `Delete`. Update Korean rustdoc that currently says system rename is rejected.

- [ ] **Step 4: Add `UsageProfileCancel` in all 12 language tables**

Add the enum key, `ALL` entry, stable index mapping and one translated value in every existing language table. Reuse `MenuAddUsageProfile`, `UsageProfileAddTitle` and `UsageProfileName`; do not introduce duplicate add/name strings.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```powershell
cargo test --test windows_app system_profile_offers_rename_but_not_logout_or_delete -- --exact
cargo test --test windows_app add_prompt_cancel_emits_no_action_and_submit_validates_the_name -- --exact
cargo test --test localization_runtime
```

Expected: PASS with all 12 languages complete.

- [ ] **Step 6: Commit controller/localization behavior**

```powershell
git add src/windows/profile_dialog.rs src/localization.rs tests/windows_app.rs tests/localization_runtime.rs
git commit -m "feat: Enable system profile rename"
```

---

### Task 4: Rebuild the Native Manager Controls and Add the Owned Name Prompt

**Files:**
- Modify: `src/windows/profile_dialog/platform.rs:35-540`
- Modify: `src/windows/profile_dialog.rs` only for platform-facing safe helpers or rustdoc
- Modify: `tests/windows_app.rs:180-430`

**Interfaces:**
- Consumes: `profile_manager_row_label`, `AddProfilePromptCommand`, `add_profile_prompt_result`, `UsageProfileCancel`, and existing `ModalWindowGuard`/`ModalDialogLifecycle`.
- Produces: `show_add_profile_prompt_owned(owner, can_add, language) -> io::Result<Option<ProfileDialogAction>>` and the final Win32 manager layout.

- [ ] **Step 1: Write failing tests for the platform-facing control contract**

Create a pure control specification in `profile_dialog.rs` that production native setup must consume:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileManagerControl {
    AddBelowList,
    Rename,
    Login,
    Logout,
    Delete,
}

pub const PROFILE_MANAGER_CONTROLS: [ProfileManagerControl; 5] = [
    ProfileManagerControl::AddBelowList,
    ProfileManagerControl::Rename,
    ProfileManagerControl::Login,
    ProfileManagerControl::Logout,
    ProfileManagerControl::Delete,
];
```

Then add tests that assert the production list contains exactly these five controls—no bottom `Add`, no bottom `Close`—and that `AddBelowList` is disabled when `ProfileDialogController::can_add()` is false. The enum must drive the `setup_controls` loop in production rather than exist only for tests.

Add a lifecycle test using the existing pure `ModalDialogLifecycle` that models nested add-prompt cancellation: the child closes and restores the manager owner without setting a profile action; the manager remains live.

- [ ] **Step 2: Run focused native-contract tests and verify RED**

Run:

```powershell
cargo test --test windows_app profile_manager_controls_exclude_bottom_add_and_close -- --exact
cargo test --test windows_app cancelled_add_prompt_restores_the_live_manager -- --exact
```

Expected: compile failure because the control contract/add prompt does not exist, or assertion failure because current setup includes `ADD_ID` and `CLOSE_ID` in the bottom row.

- [ ] **Step 3: Change the manager layout**

In `setup_controls`:

- Keep the list at the top.
- Create a `+` button immediately below the list, aligned left, using a new `OPEN_ADD_ID`.
- Move the name label right of `+` or sufficiently below it so the controls do not overlap at 100–200% DPI.
- Keep the name edit rename-only.
- Create bottom buttons only for Rename, Login, Logout and Delete from `PROFILE_MANAGER_CONTROLS`.
- Remove `CLOSE_ID`, its control and its command branch.
- Keep `WM_CLOSE`, `IDCANCEL`/Esc and RAII cleanup unchanged.
- Build list text with `profile_manager_row_label(profile, language)` and append the existing summary after it.

In `update_controls`, enable `OPEN_ADD_ID` with `state.controller.can_add()` and continue using command availability for the four action buttons.

- [ ] **Step 4: Implement the owned add prompt**

Add a second registered Win32 dialog class and a compact `AddDialogState`:

```rust
struct AddDialogState {
    edit: HWND,
    language: Language,
    result: Option<ProfileDialogAction>,
}
```

Use the existing safe modal pattern:

- owner is the live manager HWND;
- disable only an owner that was enabled;
- store state in `GWLP_USERDATA` after `WM_NCCREATE`;
- clear user data, destroy the live child and restore owner through `ModalWindowGuard` on every exit;
- `EM_SETLIMITTEXT` uses `PROFILE_LABEL_MAX_UTF16_UNITS`;
- `IDOK`/Add reads UTF-16 once and calls `add_profile_prompt_result(..., Submit)`;
- invalid input shows `UsageProfileInvalidLabel` and leaves the prompt open;
- `IDCANCEL`, Esc and `WM_CLOSE` call `add_profile_prompt_result(..., Cancel)` and close with `None`.

In manager `handle_command`, `OPEN_ADD_ID` calls the prompt synchronously. `Ok(None)` returns to the still-live manager. `Ok(Some(Add(label)))` stores the result and destroys the manager so the existing native dispatch and browser-account confirmation run exactly once.

- [ ] **Step 5: Run Windows UI tests and verify GREEN**

Run:

```powershell
cargo test --test windows_app
cargo test --test localization_runtime
cargo fmt --all -- --check
```

Expected: PASS. Existing owner restoration, keyboard cancel, UTF-16 boundary, tray popup identity and login confirmation tests remain PASS.

- [ ] **Step 6: Commit the native UX change**

```powershell
git add src/windows/profile_dialog.rs src/windows/profile_dialog/platform.rs tests/windows_app.rs
git commit -m "feat: Add usage profile creation prompt"
```

---

### Task 5: Update Documentation and Run the Full Release Gate

**Files:**
- Modify: `README.md`
- Modify: `docs/translations/README.ko.md`
- Modify: `docs/translations/README.es.md`
- Modify: `docs/translations/README.pt-BR.md`
- Modify: `docs/translations/README.id.md`
- Modify: `docs/translations/README.de.md`
- Modify: `docs/translations/README.hi.md`
- Modify: `docs/translations/README.ja.md`
- Modify: `docs/translations/README.fr.md`
- Modify: `docs/translations/README.vi.md`
- Modify: `docs/translations/README.tr.md`
- Modify: `docs/translations/README.ar.md`
- Modify: `docs/INSTALL.md`
- Modify: `docs/ACCOUNT_STORAGE.md`
- Modify: `SECURITY.md`
- Modify: `docs/RELEASE_CHECKLIST.md`
- Test: all existing unit/integration/build tests

**Interfaces:**
- Consumes: completed behavior from Tasks 1–4.
- Produces: accurate user/security/release documentation and a fully validated, clean branch.

- [ ] **Step 1: Update user and storage documentation**

In all 12 README variants, replace any statement that the system/default profile cannot be renamed with the new rule: it may be renamed, but cannot be deleted or logged out, and the custom name affects CodexPeek display only.

Update `docs/ACCOUNT_STORAGE.md` JSON example:

```json
{
  "usage_profiles": {
    "system_label": "Main",
    "managed": [
      { "sequence": 1, "label": "Work" }
    ],
    "selected": "system",
    "next_sequence": 2
  }
}
```

State that the optional system label is user-provided metadata, is stored in `settings.json`, is not an account identity, and does not alter `CODEX_HOME` or authentication files. Update INSTALL and SECURITY with the same boundary.

- [ ] **Step 2: Extend the release checklist**

Add explicit unchecked items:

- tray profile submenu has no add command and has one manage command;
- `+` is below the list, disabled at eight profiles/pending, and has localized tooltip/accessibility text;
- add prompt Enter/Add, Cancel/Esc/X and invalid-name behavior;
- manager has no bottom Close button; X/Esc close without action;
- system rename persists across restart; tray/widget show only the custom name; manager alone shows the default marker;
- system logout/delete remain unavailable;
- keyboard tab order, 12-language long text, RTL and 100/125/150/200% DPI.

- [ ] **Step 3: Run formatting and focused suites**

Run:

```powershell
cargo fmt --all -- --check
cargo test --test profile_runtime --test config_runtime --test windows_app --test localization_runtime
git diff --check
```

Expected: PASS.

- [ ] **Step 4: Run the complete automated release gate**

Run:

```powershell
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
git diff --check
```

Expected: PASS with zero test failures and zero Clippy warnings. Do not modify or commit `target/`.

- [ ] **Step 5: Check storage/security wording**

Run:

```powershell
rg -n -i "system profile.*cannot be renamed|기본.*이름.*변경.*없|auth\.json|system_label|사용량 프로필 추가" README.md SECURITY.md docs -g '!docs/superpowers/**'
```

Expected: no stale “system profile cannot be renamed” claim; `auth.json` references only explain the no-read boundary; `system_label` appears only in the account-storage explanation and source/test code; tray documentation says add is manager-only.

- [ ] **Step 6: Commit documentation**

```powershell
git add README.md SECURITY.md docs/INSTALL.md docs/ACCOUNT_STORAGE.md docs/RELEASE_CHECKLIST.md docs/translations
git commit -m "docs: Update usage profile management guidance"
```

- [ ] **Step 7: Verify final history and worktree**

Run:

```powershell
git status --short
git log -5 --oneline
```

Expected: clean worktree and one logical commit per task. Record unexecuted manual Windows checks in the handoff; do not claim release readiness until they pass.
