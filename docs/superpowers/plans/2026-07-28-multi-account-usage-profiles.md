# Multi-Account Usage Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 여러 Codex 계정을 독립된 Codex 홈에 등록하고 CodexPeek이 표시·조회할 사용량 프로필만 안전하게 전환한다.

**Architecture:** 기존 Codex 홈은 삭제 불가능한 `system` 프로필로 유지하고, 추가 프로필은 `%APPDATA%\CodexUsageMonitor\profiles\profile-NNNN\codex-home`에 격리한다. 새 폴링 조정자가 프로필별 `PollState`를 보유하면서 app-server fetch/login/logout을 한 워커에서 직렬화하고, 설정 조정자는 설정 저장과 디렉터리 변경을 한 큐에서 트랜잭션으로 처리한다. UI는 프로필 이름과 사용량만 표시하며 Windows 환경, 기본 인증, 터미널 및 IDE를 수정하지 않는다.

**Tech Stack:** Rust 2021, Rust 1.85+, `std`, `serde`/`serde_json`, 기존 `windows` 0.61 Win32 바인딩, 기존 trait 기반 테스트 대역

## Global Constraints

- Windows 10/11 x64와 Rust 1.85 이상을 유지하고 새 crate 의존성을 추가하지 않는다.
- 새 public API, 복잡한 상태 전이, I/O 부작용에는 입력·반환·부작용을 설명하는 한국어 rustdoc을 작성한다.
- 시스템·관리 프로필의 `auth.json` 내용을 열거나 파싱하거나 복사하지 않는다.
- 토큰, 계정 ID, 사용자 ID, 이메일, 인증 파일 내용, 프로필 이름·경로, 원본 RPC payload를 로그·진단·오류에 기록하지 않는다.
- 시스템 프로필을 포함한 전체 프로필 수는 최대 8개다.
- app-server 작업은 전역 단일 실행, 30초 fetch 제한, 5분 login 제한, 256 KiB JSONL 제한, `ProcessGuard`와 Job Object 정리를 유지한다.
- 프로필별 실패 백오프는 1/2/4/8/15분이고 수동 갱신은 전체 프로필에 10초 쿨다운을 적용한다.
- UI 스레드에서 설정·디렉터리·RPC I/O를 실행하지 않는다.
- 관리 app-server 자식에만 `CODEX_HOME`과 파일 인증 저장소 설정을 주입한다. Windows 사용자 환경, `PATH`, CLI·IDE 설정은 변경하지 않는다.
- 한도 기반 자동 선택, 자동 로테이션, CLI 작업 라우팅은 구현하지 않는다.
- 새 사용자 문구는 지원 언어 12개 모두에 추가한다.
- 각 작업은 해당 테스트와 `cargo fmt --all -- --check`를 통과한 뒤 하나의 논리적 커밋으로 끝낸다.

## File Structure

- Create `src/profiles.rs`: 프로필 ID, 표시명 검증, 카탈로그 불변 조건, 관리 경로 계산.
- Create `src/profile_poller.rs`: 프로필별 상태, 직렬 스케줄링, fetch/login/logout 명령과 결과 이벤트.
- Create `src/profile_settings.rs`: 프로필 설정과 관리 디렉터리의 순서 보장·롤백·삭제 대기 정리.
- Create `src/windows/profile_dialog.rs` and `src/windows/profile_dialog/platform.rs`: 프로필 관리 대화상자 계약과 Win32 구현.
- Modify `src/config.rs`: 스키마 v2, v1 마이그레이션, 순서 보장 설정 저장.
- Modify `src/codex/{process.rs,app_server.rs,mod.rs}`: app-server 프로필 실행 문맥과 logout RPC.
- Modify `src/app.rs`: 프로필 설정·폴링 이벤트와 UI 조립.
- Modify `src/windows/{mod.rs,tray.rs,tray/platform.rs,native.rs,native/platform.rs}`: 동적 메뉴, UI 동작, 프로필 표시.
- Modify `src/localization.rs`, `src/diagnostics.rs`, `src/lib.rs`: 지역화, 집계 진단, 최소 public 계약.
- Create `tests/profile_runtime.rs` and `tests/profile_poller_runtime.rs`; modify existing config/UI/localization/diagnostics tests.
- Modify README 번역본 전체, `docs/INSTALL.md`, `SECURITY.md`, `docs/RELEASE_CHECKLIST.md`.

---

### Task 1: Usage Profile Domain and Safe Paths

**Files:**
- Create: `src/profiles.rs`
- Modify: `src/lib.rs:1-23`
- Create: `tests/profile_runtime.rs`

**Interfaces:**
- Consumes: `std::path::{Path, PathBuf}`.
- Produces: `UsageProfileId`, `ManagedUsageProfile`, `UsageProfileCatalog`, `UsageProfileRoot`, `ProfileValidationError`, `normalize_profile_label`.

- [ ] **Step 1: Write failing profile validation and path tests**

```rust
use codex_usage_monitor::{
    normalize_profile_label, ProfileValidationError, UsageProfileCatalog,
    UsageProfileId, UsageProfileRoot, MAX_USAGE_PROFILES,
};

#[test]
fn profile_labels_are_trimmed_bounded_and_case_insensitively_unique() {
    assert_eq!(normalize_profile_label("  개인  ").unwrap(), "개인");
    assert_eq!(normalize_profile_label(".."), Err(ProfileValidationError::InvalidLabel));
    assert_eq!(normalize_profile_label("bad/name"), Err(ProfileValidationError::InvalidLabel));
    let mut catalog = UsageProfileCatalog::default();
    assert_eq!(catalog.add("Work").unwrap().id(), UsageProfileId::Managed(1));
    assert_eq!(catalog.add(" work "), Err(ProfileValidationError::DuplicateLabel));
}

#[test]
fn managed_paths_are_derived_only_from_numeric_ids() {
    let root = UsageProfileRoot::new(std::path::PathBuf::from(r"C:\safe\appdata"));
    assert_eq!(
        root.codex_home(UsageProfileId::Managed(7)).unwrap(),
        std::path::PathBuf::from(r"C:\safe\appdata\profiles\profile-0007\codex-home")
    );
    assert!(root.codex_home(UsageProfileId::System).is_err());
}
```

Add one test that inserts seven managed profiles and verifies the eighth managed add returns `TooManyProfiles` because the system profile counts toward `MAX_USAGE_PROFILES == 8`.

- [ ] **Step 2: Run the new test to verify missing APIs fail**

Run: `cargo test --test profile_runtime`

Expected: FAIL with unresolved imports from `codex_usage_monitor`.

- [ ] **Step 3: Add the profile domain types and invariants**

```rust
pub const MAX_USAGE_PROFILES: usize = 8;

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageProfileId {
    System,
    Managed(u32),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ManagedUsageProfile {
    sequence: u32,
    label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UsageProfileCatalog {
    managed: Vec<ManagedUsageProfile>,
    selected: UsageProfileId,
    next_sequence: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileValidationError {
    InvalidLabel,
    DuplicateLabel,
    TooManyProfiles,
    InvalidId,
    SystemProfileImmutable,
}
```

Implement `validate`, `add`, `rename`, `select`, and `remove`. `normalize_profile_label` trims, enforces 1–40 Unicode scalar values, and rejects control characters, `/`, `\`, `.`, and `..`. Duplicate comparison uses `to_lowercase()` only; no normalization dependency is added. `UsageProfileRoot::codex_home` accepts only `Managed(sequence > 0)` and joins fixed path segments.

- [ ] **Step 4: Export the minimal domain contract and run tests**

```rust
mod profiles;
pub use profiles::{
    normalize_profile_label, ManagedUsageProfile, ProfileValidationError,
    UsageProfileCatalog, UsageProfileId, UsageProfileRoot, MAX_USAGE_PROFILES,
};
```

Run: `cargo test --test profile_runtime && cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src/profiles.rs src/lib.rs tests/profile_runtime.rs
git commit -m "feat: Add usage profile domain model"
```

### Task 2: Settings Schema v2 and Deterministic Migration

**Files:**
- Modify: `src/config.rs:15-253`
- Modify: `tests/config_runtime.rs`

**Interfaces:**
- Consumes: `UsageProfileCatalog` from Task 1.
- Produces: `Settings.usage_profiles`, schema v1-to-v2 migration, `SettingsStore::root()`.

- [ ] **Step 1: Add failing migration and invalid-catalog tests**

```rust
#[test]
fn schema_v1_migrates_to_system_profile_and_is_persisted_as_v2() {
    let root = test_root("profile-migration");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("settings.json"), valid_schema_v1_json()).unwrap();
    let store = SettingsStore::for_root(root.clone());
    let loaded = store.load().unwrap();
    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.usage_profiles.selected(), UsageProfileId::System);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(persisted["schema_version"], 2);
    assert!(persisted.get("usage_profiles").is_some());
}
```

`valid_schema_v1_json()` must return the exact current v1 fields. Add tests for duplicate sequence, missing selected profile, non-increasing `next_sequence`, profile overflow, and schema 3 backup/reset.

Define `valid_schema_v1_json() -> &'static [u8]` in `tests/config_runtime.rs`; its literal must contain all eleven persisted v1 fields shown in `LegacySettingsV1`, with `schema_version: 1`, so the test is independent of current `Settings` serialization.

- [ ] **Step 2: Run migration test and verify failure**

Run: `cargo test --test config_runtime schema_v1_migrates_to_system_profile_and_is_persisted_as_v2`

Expected: FAIL because schema 1 currently resets and schema 2 fields are absent.

- [ ] **Step 3: Add an explicit v1 decoder and persist migration under one lock**

```rust
#[derive(Deserialize)]
struct SettingsEnvelope {
    schema_version: u32,
}

#[derive(Deserialize)]
struct LegacySettingsV1 {
    schema_version: u32,
    refresh_interval_minutes: u32,
    widget_visible: bool,
    taskbar_offset: i32,
    #[serde(default)]
    taskbar_display_mode: TaskbarDisplayMode,
    start_with_windows: bool,
    startup_view: StartupView,
    auto_auth_refresh: bool,
    #[serde(default = "default_language_preference")]
    language: LanguagePreference,
    last_update_check_unix: Option<u64>,
    #[serde(default)]
    show_remaining_percent: bool,
}
```

Set `SCHEMA_VERSION = 2`. `LegacySettingsV1::into_current` copies every preference and assigns `UsageProfileCatalog::default()`. Factor atomic write into `save_locked` so `load` can persist a migrated value without reacquiring the gate. `inspect_validity` accepts valid v1 without mutation. `Settings::validate` calls `usage_profiles.validate()`.

- [ ] **Step 4: Expose the read-only root and run all config tests**

```rust
pub fn root(&self) -> &Path {
    &self.root
}
```

Run: `cargo test --test config_runtime --test profile_runtime && cargo fmt --all -- --check`

Expected: PASS, including corruption backups and atomic replace tests.

- [ ] **Step 5: Commit**

```powershell
git add src/config.rs tests/config_runtime.rs
git commit -m "feat: Migrate settings to usage profiles"
```

### Task 3: Profile-Scoped app-server Processes and Account RPCs

**Files:**
- Modify: `src/profiles.rs`
- Modify: `src/codex/process.rs:17-153`
- Modify: `src/codex/app_server.rs:29-133,362-490`
- Modify: `src/codex/mod.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `UsageProfileId`, `UsageProfileRoot`.
- Produces: `ProfileExecutionContext`, `ProfileAccountProvider`, profile-aware launch plan, `account/logout`.

- [ ] **Step 1: Add failing launch isolation and logout protocol tests**

```rust
#[test]
fn managed_profile_applies_child_only_codex_home_and_file_credentials() {
    let root = UsageProfileRoot::new(PathBuf::from(r"C:\app"));
    let context = ProfileExecutionContext::managed(&root, UsageProfileId::Managed(2)).unwrap();
    let plan = launch_plan(CandidateKind::NativeExe, PathBuf::from("codex.exe"), &context);
    assert_eq!(plan.arguments, ["app-server", "--stdio", "-c", "cli_auth_credentials_store=file"]);
    assert_eq!(plan.environment[0].0, OsString::from("CODEX_HOME"));
}

#[test]
fn logout_initializes_then_sends_account_logout() {
    let mut transport = ScriptedTransport::new([
        r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"result":{}}"#,
    ]);
    logout_until(&mut transport, Instant::now() + Duration::from_secs(1)).unwrap();
    assert_eq!(transport.requests()[2], r#"{"id":2,"method":"account/logout","params":{}}"#);
}
```

Add a system-context test proving no environment override and exact legacy arguments `app-server --stdio`.

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --lib managed_profile_applies_child_only_codex_home_and_file_credentials logout_initializes_then_sends_account_logout`

Expected: FAIL because context, environment, and logout are absent.

- [ ] **Step 3: Add a redacted execution context and child environment**

```rust
#[derive(Clone, PartialEq, Eq)]
pub struct ProfileExecutionContext {
    id: UsageProfileId,
    codex_home: Option<PathBuf>,
    force_file_credentials: bool,
}
```

Provide `system`, `managed(&UsageProfileRoot, UsageProfileId)`, `id`, and crate-private `codex_home` accessors. The managed constructor derives its path internally and never accepts an arbitrary path. Implement custom `Debug` containing only ID and managed boolean. Extend `LaunchPlan` with `environment: Vec<(OsString, OsString)>`; apply with `command.env` only to app-server child. Keep version probes unchanged.

- [ ] **Step 4: Add the object-safe provider and bounded logout**

```rust
pub type LoginPageOpener = Arc<dyn Fn(&str) -> std::io::Result<()> + Send + Sync>;

#[derive(Clone, Default)]
pub struct OperationCancellation(Arc<AtomicBool>);

pub trait ProfileAccountProvider: Send + Sync {
    fn fetch_profile(&self, profile: &ProfileExecutionContext, allow_auth_refresh: bool,
        cancellation: OperationCancellation)
        -> Result<CodexUsage, UsageError>;
    fn login_profile(&self, profile: &ProfileExecutionContext, open: LoginPageOpener,
        cancellation: OperationCancellation)
        -> Result<bool, UsageError>;
    fn logout_profile(&self, profile: &ProfileExecutionContext,
        cancellation: OperationCancellation)
        -> Result<(), UsageError>;
}
```

Implement `cancel()` with `store(true, Ordering::Release)` and `is_cancelled()` with `load(Ordering::Acquire)`; never place profile data in this token.

Implement all methods on `AppServerUsageProvider` with its existing clone-shared single-flight gate. Run JSONL work on a transport worker while the caller owns `ProcessGuard`; poll the result channel in at most 50 ms slices, and call `guard.terminate_tree()` immediately when cancellation is set or the deadline expires. This pattern applies to fetch, login, and logout so a five-minute browser login can be cancelled during app shutdown. `logout_until` performs initialize → ID 2 `account/logout` with empty params → bounded ignored result. Do not deserialize `account/updated` fields. Existing `UsageProvider::fetch` delegates to system context with a fresh uncancelled token.

- [ ] **Step 5: Run codex tests and commit**

Run: `cargo test --lib codex:: && cargo fmt --all -- --check`

Expected: PASS, including wrapper quoting, timeouts, JSONL limits, and sensitive-extra-field tests.

```powershell
git add src/profiles.rs src/lib.rs src/codex/process.rs src/codex/app_server.rs src/codex/mod.rs
git commit -m "feat: Isolate Codex usage profile processes"
```

### Task 4: Serialized Multi-Profile Polling Coordinator

**Files:**
- Create: `src/profile_poller.rs`
- Modify: `src/lib.rs`
- Create: `tests/profile_poller_runtime.rs`

**Interfaces:**
- Consumes: `PollState`, `PollTrigger`, `PollSnapshot`, `ProfileExecutionContext`, `ProfileAccountProvider`, `LoginPageOpener`.
- Produces: `ProfilePollingService`, `ProfilePollEvent`; existing `PollingService` remains compatible.

- [ ] **Step 1: Write a fake provider and failing serialization tests**

```rust
#[derive(Clone)]
struct FakeProfileProvider {
    calls: Arc<Mutex<Vec<(UsageProfileId, &'static str)>>>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

impl ProfileAccountProvider for FakeProfileProvider {
    fn fetch_profile(
        &self,
        profile: &ProfileExecutionContext,
        _allow_auth_refresh: bool,
        _cancellation: OperationCancellation,
    ) -> Result<CodexUsage, UsageError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        self.calls.lock().unwrap().push((profile.id(), "fetch"));
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(usage_for(profile.id()))
    }

    fn login_profile(&self, profile: &ProfileExecutionContext, _open: LoginPageOpener,
        _cancellation: OperationCancellation)
        -> Result<bool, UsageError> {
        self.calls.lock().unwrap().push((profile.id(), "login"));
        Ok(true)
    }

    fn logout_profile(&self, profile: &ProfileExecutionContext,
        _cancellation: OperationCancellation) -> Result<(), UsageError> {
        self.calls.lock().unwrap().push((profile.id(), "logout"));
        Ok(())
    }
}
```

Test that selected profile is fetched first, two initial profiles are fetched serially, login is ordered between fetches, `max_active == 1`, one timeout preserves only that profile's last-good value, and global manual refresh rejects the second request within 10 seconds.

- [ ] **Step 2: Run the new target and verify missing service failure**

Run: `cargo test --test profile_poller_runtime`

Expected: FAIL with unresolved `ProfilePollingService` and `ProfilePollEvent`.

- [ ] **Step 3: Define commands, events, and snapshots**

```rust
pub enum ProfilePollEvent {
    LoginFinished { id: UsageProfileId, result: Result<bool, UsageError> },
    LogoutFinished { id: UsageProfileId, result: Result<(), UsageError> },
    ProfileQuiesced(UsageProfileId),
}

enum ProfilePollCommand {
    Select(UsageProfileId),
    RefreshSelected(PollTrigger),
    Add(ProfileExecutionContext),
    Quiesce(UsageProfileId),
    Remove(UsageProfileId),
    Login(UsageProfileId, LoginPageOpener),
    Logout(UsageProfileId),
    SetRefreshInterval(u32),
    SetAutoAuthRefresh(bool),
    Stop,
}
```

The worker owns stable ordered contexts, `HashMap<UsageProfileId, PollState>`, selected ID, global `last_manual_at`, provider, and the current `OperationCancellation`. Expose nonblocking command methods, `snapshot(id)`, `selected_snapshot()`, `selected_id()`, `take_events()`, and `stop()`.

- [ ] **Step 4: Implement exact scheduling rules**

Drain explicit commands before automatic work. Execute login/logout/quiesce on the same worker. Choose minimum `next_poll_at`, breaking ties by selected ID then stable vector order. Enforce global manual cooldown before calling the selected state's `begin`. Call the provider outside the state mutex and finish only the targeted state. `Quiesce` removes the state from scheduling and emits only after current work returns. `stop()` and `Drop` set the current cancellation before sending `Stop`; explicit `stop()` joins after the bounded provider returns, while `Drop` detaches after cancellation like the existing service. Never modify existing `PollState` backoff/reset semantics.

- [ ] **Step 5: Run old and new poller tests and commit**

Run: `cargo test --test poller_runtime --test profile_poller_runtime && cargo fmt --all -- --check`

Expected: PASS with every multi-profile test observing `max_active == 1`.

```powershell
git add src/profile_poller.rs src/lib.rs tests/profile_poller_runtime.rs
git commit -m "feat: Add serialized usage profile polling"
```

### Task 5: Transactional Profile Settings and Directory Changes

**Files:**
- Create: `src/profile_settings.rs`
- Modify: `src/config.rs:278-370`
- Modify: `src/profiles.rs`
- Modify: `src/lib.rs`
- Modify: `tests/config_runtime.rs`
- Modify: `tests/profile_runtime.rs`

**Interfaces:**
- Consumes: `SettingsStore`, `Settings`, `UsageProfileCatalog`, `UsageProfileRoot`.
- Produces: `ProfileSettingsService`, `ProfileSettingsMutation`, `ProfileSettingsEvent`, safe tombstone cleanup.

- [ ] **Step 1: Add failing ordered-write and rollback tests**

Define a test backend contract:

```rust
pub trait ProfileFileSystem: Send + Sync {
    fn create_managed_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()>;
    fn remove_empty_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()>;
    fn stage_delete(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<PathBuf>;
    fn restore_staged(&self, staged: &Path, destination: &Path) -> io::Result<()>;
    fn remove_staged(&self, staged: &Path) -> io::Result<()>;
    fn cleanup_staged(&self, root: &UsageProfileRoot) -> io::Result<()>;
}
```

Use a recording fake to assert:

```rust
#[test]
fn delete_stages_directory_saves_settings_then_removes_staged_directory() {
    let backend = RecordingProfileFileSystem::default();
    let service = service_with_managed_profile(backend.clone());
    service.submit(ProfileSettingsMutation::Delete {
        id: UsageProfileId::Managed(1),
    }).unwrap();
    assert!(matches!(service.wait_for_event(), ProfileSettingsEvent::Deleted { .. }));
    assert_eq!(backend.operations(), ["stage_delete", "save_settings", "remove_staged"]);
}
```

Add failure tests: add save failure removes only newly created empty home; delete save failure restores staged directory; final removal failure leaves a validated hidden tombstone; startup cleanup removes only tombstones; preference writes never replace a newer profile catalog.

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --test profile_runtime --test config_runtime profile_settings`

Expected: FAIL because service and backend are absent.

- [ ] **Step 3: Define typed mutations and events**

```rust
pub enum ProfileSettingsMutation {
    Add { label: String },
    Rename { id: UsageProfileId, label: String },
    Select { id: UsageProfileId },
    Delete { id: UsageProfileId },
}

pub enum ProfileSettingsEvent {
    Added { settings: Settings, id: UsageProfileId },
    Renamed { settings: Settings, id: UsageProfileId },
    Selected { settings: Settings, id: UsageProfileId },
    Deleted { settings: Settings, id: UsageProfileId },
    Failed { operation: &'static str, kind: io::ErrorKind },
}
```

`ProfileSettingsService::start` receives store, loaded settings, and backend; its worker owns authoritative settings. `save_preferences(Settings)` copies only non-profile fields before saving, so stale UI snapshots cannot overwrite the catalog. `submit` and `take_events` are nonblocking.

- [ ] **Step 4: Implement safe native filesystem transactions**

`NativeProfileFileSystem` derives every path from `UsageProfileRoot`, rejects system/sequence zero, verifies the absolute parent equals `<settings-root>\profiles`, and rejects reparse points. On Windows, inspect `std::os::windows::fs::MetadataExt::file_attributes()` and reject any target or existing parent with `FILE_ATTRIBUTE_REPARSE_POINT (0x0000_0400)`. Tombstones use `.deleting-profile-NNNN-<process-id>-<nonce>` generated internally.

Add sequence: create exact home → save settings → emit. Delete sequence: same-volume stage rename → save catalog removal/system fallback → delete staged directory. Restore on save failure. If final removal fails, emit deleted and retry only validated tombstones next startup. Never log paths or names.

- [ ] **Step 5: Ensure exactly one settings writer exists**

Replace app-facing `AsyncSettingsWriter` use with this ordered service or route preference saves into the same queue. Keep `SettingsStore::save` atomic and public for tests; do not leave two runtime workers writing `settings.json`.

- [ ] **Step 6: Run tests and commit**

Run: `cargo test --test config_runtime --test profile_runtime && cargo fmt --all -- --check`

Expected: PASS with no `.settings.tmp-*` files and correct rollback/tombstone behavior.

```powershell
git add src/profile_settings.rs src/config.rs src/profiles.rs src/lib.rs tests/config_runtime.rs tests/profile_runtime.rs
git commit -m "feat: Add transactional usage profile storage"
```

### Task 6: Localized Profile View Models and Dynamic Tray Commands

**Files:**
- Modify: `src/localization.rs`
- Modify: `src/windows/mod.rs:14-77,254-437`
- Modify: `src/windows/tray.rs`
- Modify: `src/windows/tray/platform.rs`
- Modify: `tests/localization_runtime.rs`
- Modify: `tests/windows_app.rs`

**Interfaces:**
- Consumes: `UsageProfileId`, `UiAction`, `UiSettings`, tray tree.
- Produces: `UsageProfileView`, `TrayMenuModel`, dynamic action lookup, profile-aware widget/tooltip fields.

- [ ] **Step 1: Add localization keys and stable expectations**

Add these keys to the enum and `ALL`: `MenuUsageProfiles`, `MenuAddUsageProfile`, `MenuManageUsageProfiles`, `UsageProfileSystem`, `UsageProfileDisplayed`, `UsageProfileCliUnchanged`, `UsageProfileLoginRequired`, `UsageProfileAddTitle`, `UsageProfileConfirmBrowserAccount`, `UsageProfileRename`, `UsageProfileLogin`, `UsageProfileLogout`, `UsageProfileDelete`, `UsageProfileDeleteConfirm`, `UsageProfileLimitReached`.

```rust
assert_eq!(localized_text(LocalizationKey::MenuUsageProfiles, Language::Korean), "사용량 프로필");
assert_eq!(localized_text(LocalizationKey::MenuUsageProfiles, Language::English), "Usage profiles");
assert_eq!(localized_text(LocalizationKey::UsageProfileCliUnchanged, Language::English), "Codex CLI sign-in is unchanged");
```

Update completeness tests so all 15 keys are nonempty in all 12 languages.

- [ ] **Step 2: Run localization tests and verify failure**

Run: `cargo test --test localization_runtime`

Expected: FAIL until every language table contains every key.

- [ ] **Step 3: Add profile view types and typed actions**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageProfileView {
    pub id: UsageProfileId,
    pub label: String,
    pub summary: String,
    pub selected: bool,
    pub login_required: bool,
    pub managed: bool,
}
```

Add `UiAction::{SelectUsageProfile, AddUsageProfile, RenameUsageProfile, LoginUsageProfile, LogoutUsageProfile, DeleteUsageProfile}` carrying IDs/labels. Add profiles and mutation-pending flag to `UiSettings`; add `usage_profile_label` to `WidgetViewModel`. Preserve every existing variant/field.

Render `usage_profile_label` as a persistent header in the floating widget. Do not add it to compact taskbar body geometry; prepend it to the taskbar tooltip together with the localized CLI-unchanged notice.

- [ ] **Step 4: Build a popup-lifetime dynamic action model**

```rust
pub struct TrayMenuModel {
    pub entries: Vec<TrayMenuEntry>,
    actions: Vec<(u16, UiAction)>,
}

impl TrayMenuModel {
    pub fn action(&self, id: u16) -> Option<UiAction> {
        self.actions.iter().find_map(|(candidate, action)| {
            (*candidate == id).then(|| action.clone())
        })
    }
}
```

Reserve IDs `1000..=1007` inside one popup model. Retain that model until `TrackPopupMenu` returns, then resolve against it; never rebuild from mutable state. Static IDs keep existing mapping.

- [ ] **Step 5: Add tray identity and tooltip tests**

```rust
#[test]
fn popup_profile_action_keeps_the_profile_identity() {
    let model = tray_menu_model(&tray_settings_with_profiles());
    assert_eq!(
        model.action(1001),
        Some(UiAction::SelectUsageProfile(UsageProfileId::Managed(1)))
    );
}
```

Assert the tooltip includes localized selected label and CLI-unchanged notice, while compact taskbar dimensions remain unchanged. Update all existing `UiSettings` fixtures with a system profile.

- [ ] **Step 6: Run UI/localization tests and commit**

Run: `cargo test --test localization_runtime --test windows_app && cargo fmt --all -- --check`

Expected: PASS.

```powershell
git add src/localization.rs src/windows/mod.rs src/windows/tray.rs src/windows/tray/platform.rs tests/localization_runtime.rs tests/windows_app.rs
git commit -m "feat: Add localized usage profile menu"
```

### Task 7: Native Profile Management Dialogs

**Files:**
- Create: `src/windows/profile_dialog.rs`
- Create: `src/windows/profile_dialog/platform.rs`
- Modify: `src/windows/mod.rs:1-10`
- Modify: `src/windows/native.rs`
- Modify: `src/windows/native/platform.rs`
- Modify: `tests/windows_app.rs`

**Interfaces:**
- Consumes: `UsageProfileView`, `UsageProfileId`, `ProfileValidationError`, localized labels.
- Produces: `ProfileDialogAction`, add/rename input, login/delete confirmation.

- [ ] **Step 1: Add failing platform-independent dialog tests**

```rust
#[test]
fn system_profile_never_offers_rename_or_delete() {
    let actions = available_profile_actions(&system_profile_view());
    assert!(!actions.contains(&ProfileDialogCommand::Rename));
    assert!(!actions.contains(&ProfileDialogCommand::Delete));
    assert!(actions.contains(&ProfileDialogCommand::Login));
}

#[test]
fn dialog_labels_use_shared_validation() {
    assert_eq!(validated_label("  Work  ").unwrap(), "Work");
    assert_eq!(validated_label("bad\\name"), Err(ProfileValidationError::InvalidLabel));
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --test windows_app system_profile_never_offers_rename_or_delete dialog_labels_use_shared_validation`

Expected: FAIL because dialog contracts are absent.

- [ ] **Step 3: Add the platform-independent API**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileDialogAction {
    Add(String),
    Rename(UsageProfileId, String),
    Login(UsageProfileId),
    Logout(UsageProfileId),
    Delete(UsageProfileId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileDialogCommand {
    Rename,
    Login,
    Logout,
    Delete,
}

pub fn available_profile_actions(profile: &UsageProfileView) -> Vec<ProfileDialogCommand>;
pub fn validated_label(value: &str) -> Result<String, ProfileValidationError>;

pub fn show_profile_manager(
    profiles: &[UsageProfileView],
    mutation_pending: bool,
    language: Language,
) -> io::Result<Option<ProfileDialogAction>>;

pub fn confirm_profile_login(language: Language) -> io::Result<bool>;
pub fn confirm_profile_delete(label: &str, language: Language) -> io::Result<bool>;
```

Non-Windows implementations return `Unsupported` without filesystem or environment effects.

- [ ] **Step 4: Build the owned modal Win32 dialog**

Create one modal top-level window owned by the hidden message-loop window. Use a listbox for profiles, an edit control for add/rename, localized add/rename/login/logout/delete/close buttons, and return one action before destruction. Disable rename/delete for system and all mutation buttons while pending.

Apply `EM_SETLIMITTEXT(40)`, retrieve into a fixed 41-UTF-16-unit buffer, then call `normalize_profile_label`. Never put labels into class names, command IDs, paths, or logs.

- [ ] **Step 5: Add confirmations with no external process mutation**

Use owned `MessageBoxW`: OK/Cancel before login with browser-account warning; Yes/No before delete stating local profile data is unrecoverable. Cancellation dispatches nothing. Never close terminal, IDE, Codex app, or alter their environment.

- [ ] **Step 6: Compile/test and commit**

Run: `cargo test --test windows_app && cargo check --all-targets && cargo fmt --all -- --check`

Expected: PASS with no new `windows` feature required.

```powershell
git add src/windows/profile_dialog.rs src/windows/profile_dialog/platform.rs src/windows/mod.rs src/windows/native.rs src/windows/native/platform.rs tests/windows_app.rs
git commit -m "feat: Add native usage profile manager"
```

### Task 8: Runtime Integration and Safe Diagnostics

**Files:**
- Modify: `src/app.rs:33-363,1120-1200`
- Modify: `src/diagnostics.rs`
- Modify: `src/windows/native/platform.rs`
- Modify: `src/lib.rs`
- Modify: `tests/diagnostics_runtime.rs`
- Modify: `tests/profile_runtime.rs`
- Modify: `tests/profile_poller_runtime.rs`
- Modify: `tests/windows_app.rs`

**Interfaces:**
- Consumes: services and view models from Tasks 1-7.
- Produces: complete add/login/select/rename/logout/delete flow, selected-profile rendering, aggregate diagnostics, shutdown cleanup.

- [ ] **Step 1: Add failing runtime event-order tests**

Extract `ProfileRuntimeState` and test durable ordering:

```rust
#[test]
fn added_profile_is_logged_in_only_after_durable_settings_success() {
    let mut runtime = profile_runtime_fixture();
    runtime.request_add("개인".into()).unwrap();
    assert!(runtime.poll_commands().is_empty());
    runtime.apply_settings_event(ProfileSettingsEvent::Added {
        settings: settings_with_profile(1, "개인"),
        id: UsageProfileId::Managed(1),
    });
    assert_eq!(runtime.poll_commands(), ["add:managed-1", "login:managed-1"]);
}

#[test]
fn delete_waits_for_quiesce_before_storage_delete() {
    let mut runtime = selected_managed_profile_fixture();
    runtime.request_delete(UsageProfileId::Managed(1)).unwrap();
    assert_eq!(runtime.poll_commands(), ["quiesce:managed-1"]);
    assert!(runtime.settings_commands().is_empty());
    runtime.apply_poll_event(ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(1)));
    assert_eq!(runtime.settings_commands(), ["delete:managed-1"]);
}
```

Define the test-only helpers `profile_runtime_fixture`, `selected_managed_profile_fixture`, and `settings_with_profile` in `tests/profile_runtime.rs`; each uses recording command sinks and temporary settings roots, never a real Codex account or auth file.

Add tests for selection save failure retaining old render, successful login selecting then forced-refreshing, cancelled login retaining `login_required`, and independent profile errors.

- [ ] **Step 2: Run runtime tests and verify failure**

Run: `cargo test --test profile_runtime --test profile_poller_runtime runtime`

Expected: FAIL because runtime state/event integration is absent.

- [ ] **Step 3: Replace single-profile runtime fields and preserve startup order**

```rust
profile_settings: ProfileSettingsService,
profile_poller: ProfilePollingService,
profile_state: ProfileRuntimeState,
```

Startup order: acquire single instance → load/migrate settings → build `UsageProfileRoot` → resolve system/managed execution contexts → start settings service → start selected-first profile poller → update check → UI. Remove `login_refresh_pending`; login completion arrives through `ProfilePollEvent`.

- [ ] **Step 4: Drain events without UI-thread I/O**

At the start of `snapshot`, `settings`, and `dispatch`, drain both event queues. Apply exact transitions: Added → update settings/add poll context/enqueue login; Selected → update settings/select/refresh; Renamed → label update; Quiesced during deletion → submit settings delete; Deleted → remove poll context and select system; login success → submit durable selection then forced refresh; login false/error → retain profile and safe login-required/error; settings failure → retain prior state and record stable diagnostic.

- [ ] **Step 5: Render selected profile and all profile summaries**

Use only `profile_poller.snapshot(selected_id)` for `WidgetViewModel`. Build `UiSettings.usage_profiles` from every profile snapshot with localized system label and one summary: weekly remaining, refreshing, login required, or unavailable. Never infer/display email or account ID. Selection immediately renders cached data; no cache renders loading/login-required.

- [ ] **Step 6: Wire native dialog actions**

Open the manager from the tray model, convert its result into typed `UiAction`, and require login/delete confirmations. All actions enqueue worker commands and return an updated view immediately; none waits for I/O.

- [ ] **Step 7: Add aggregate-only profile diagnostics**

```rust
Profiles {
    settings_valid: bool,
    configured: u8,
    ok: u8,
    login_required: u8,
    request_failed: u8,
}
```

Clamp counts to 8. Never include labels, IDs, paths, or ordered per-profile tuples. Add redaction tests using a fixture label/path and assert neither appears in serialized diagnostics/log text. `--diagnose` may aggregate auth existence only and never read contents.

- [ ] **Step 8: Verify shutdown and existing behavior**

Test `Stop`, bounded worker cleanup, and Job Object coverage. Rerun Explorer lifecycle, autostart, update, language, taskbar display, show-remaining, and single-instance tests.

Run: `cargo test --all-targets && cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 9: Commit**

```powershell
git add src/app.rs src/diagnostics.rs src/windows/native/platform.rs src/lib.rs tests/diagnostics_runtime.rs tests/profile_runtime.rs tests/profile_poller_runtime.rs tests/windows_app.rs
git commit -m "feat: Integrate multi-account usage profiles"
```

### Task 9: Documentation, Security Contract, and Release Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/translations/README.ar.md`
- Modify: `docs/translations/README.de.md`
- Modify: `docs/translations/README.es.md`
- Modify: `docs/translations/README.fr.md`
- Modify: `docs/translations/README.hi.md`
- Modify: `docs/translations/README.id.md`
- Modify: `docs/translations/README.ja.md`
- Modify: `docs/translations/README.ko.md`
- Modify: `docs/translations/README.pt-BR.md`
- Modify: `docs/translations/README.tr.md`
- Modify: `docs/translations/README.vi.md`
- Modify: `docs/INSTALL.md`
- Modify: `SECURITY.md`
- Modify: `docs/RELEASE_CHECKLIST.md`

**Interfaces:**
- Consumes: verified behavior from Tasks 1-8.
- Produces: localized guidance, security disclosure, release checks, final verification.

- [ ] **Step 1: Update all README variants**

In each language state: profiles use separate Codex homes; labels are user-provided because CodexPeek does not inspect email/ID; selection changes only CodexPeek display/polling; terminal, IDE, Codex app, WSL, Remote SSH, and Dev Containers are unchanged. Do not describe automatic selection or recommend limit bypass.

- [ ] **Step 2: Update install/security documentation**

Document add/login/select/delete and browser-account verification in `docs/INSTALL.md`. In `SECURITY.md`, document child-only `CODEX_HOME`, managed file credential storage, auth-file non-reading, exact validated delete, and no CLI/IDE mutation.

- [ ] **Step 3: Extend release checklist**

Add manual checks for distinct and accidental duplicate account login, cancel/retry/offline logout, one failing profile, deletion rollback/tombstone cleanup, unchanged CLI/VS Code login, Windows 10/11, 100/125/150/200% DPI, RTL, 40-character label, multi-monitor, auto-hide, Explorer restart.

- [ ] **Step 4: Run local-link and sensitive-text checks**

Run a PowerShell relative-link resolver across the default and 11 translated READMEs; expected broken count `0`.

Run: `rg -n -S "accessToken|refresh_token|account_id|Authorization: Bearer" src tests README.md SECURITY.md docs --glob '!docs/superpowers/**'`

Expected: only existing intentional redaction/security tests and explanatory prose; no new DTO, fixture, log, or error retains these fields.

- [ ] **Step 5: Run full automated verification**

```powershell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
git diff --check
```

Expected: every command exits `0`.

- [ ] **Step 6: Record manual verification gaps**

Execute the updated Windows checklist where available. List every unexecuted matrix item in the handoff; do not declare release readiness from automated tests alone.

- [ ] **Step 7: Commit**

```powershell
git add README.md docs/translations docs/INSTALL.md SECURITY.md docs/RELEASE_CHECKLIST.md
git commit -m "docs: Document multi-account usage profiles"
```

## Spec Coverage Review

- 목적·CLI/IDE 비변경·자동 로테이션 제외: Global Constraints, Tasks 3, 8, 9.
- 프로필 UX·명확한 용어·플로팅/툴팁 표시: Tasks 6, 7, 8.
- 시스템/관리 모델·8개 제한·안전 경로: Tasks 1, 2, 5.
- 스키마 마이그레이션·단일 writer·파일 트랜잭션: Tasks 2, 5.
- child-only `CODEX_HOME`·파일 인증·logout·취소: Tasks 3, 4.
- 선택 우선·직렬 폴링·독립 백오프·전역 쿨다운: Task 4.
- 추가·브라우저 확인·실패 프로필 보존·관리/삭제: Tasks 5, 7, 8.
- 민감 필드 비역직렬화·집계 진단: Tasks 3, 8.
- 결정적 자동 테스트·Windows 수동 매트릭스·문서화: Tasks 1–9, 특히 Task 9.

Self-review found no uncovered spec requirement; implementation execution must stop and revise this plan if a task reveals a contract conflict.

## Final Review Gate

- [ ] Every requirement in `docs/superpowers/specs/2026-07-28-multi-account-usage-profiles-design.md` maps to a task above.
- [ ] No code changes Windows user/system environment, `PATH`, CLI settings, IDE settings, or default authentication files.
- [ ] All app-server operations are serialized and every profile has independent last-good/backoff state.
- [ ] Delete validates exact managed path, rejects reparse points, and has rollback/tombstone recovery.
- [ ] Public/complex APIs have Korean rustdoc and all 12 languages include every new key.
- [ ] Worktree is clean and nine logical commits are reviewable before publish/deploy.
