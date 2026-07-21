use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use codex_usage_monitor::{
    windows::{
        autostart::{autostart_command, set_autostart, RegistryBackend},
        initial_widget_visible, is_exact_github_tag_page,
        lifecycle::{
            CleanupAction, DetachOutcome, FloatingTransition, NativeLifecycle, RecoveryDecision,
            RecoveryEvent,
        },
        menu_action, resolve_windows_language, startup_plan,
        taskbar::{
            place_taskbar_widget, run_taskbar_attachment, taskbar_child_style, taskbar_widget_size,
            TaskbarAttachmentBackend, TaskbarAttachmentStage, TaskbarGeometry,
            TaskbarPlacementError,
        },
        widget::{
            clamp_floating_position, logical_to_physical, physical_to_logical,
            restore_monitor_relative_position, save_monitor_relative_position, Rect, WidgetLayout,
        },
        LaunchMode, StartupStep, UiAction, MENU_ALWAYS_ON_TOP, MENU_AUTH_REFRESH, MENU_AUTOSTART,
        MENU_AUTO_AUTH_REFRESH, MENU_DIAGNOSTICS, MENU_DISPLAY_FLOATING, MENU_DISPLAY_TASKBAR,
        MENU_EXIT, MENU_INTERVAL_1, MENU_INTERVAL_10, MENU_INTERVAL_15, MENU_INTERVAL_30,
        MENU_INTERVAL_5, MENU_LANGUAGE_AUTO, MENU_LANGUAGE_ENGLISH, MENU_LANGUAGE_KOREAN,
        MENU_POSITION_RESET, MENU_REFRESH, MENU_STARTUP_TRAY, MENU_STARTUP_WIDGET,
        MENU_UPDATE_CHECK, MENU_WIDGET_VISIBLE,
    },
    DisplayMode, LanguagePreference, StartupView,
};

#[test]
fn every_menu_command_maps_to_a_typed_action() {
    let cases = [
        (MENU_REFRESH, UiAction::Refresh),
        (
            MENU_DISPLAY_TASKBAR,
            UiAction::SetDisplayMode(DisplayMode::Taskbar),
        ),
        (
            MENU_DISPLAY_FLOATING,
            UiAction::SetDisplayMode(DisplayMode::Floating),
        ),
        (MENU_INTERVAL_1, UiAction::SetRefreshInterval(1)),
        (MENU_INTERVAL_5, UiAction::SetRefreshInterval(5)),
        (MENU_INTERVAL_10, UiAction::SetRefreshInterval(10)),
        (MENU_INTERVAL_15, UiAction::SetRefreshInterval(15)),
        (MENU_INTERVAL_30, UiAction::SetRefreshInterval(30)),
        (MENU_AUTOSTART, UiAction::ToggleAutostart),
        (
            MENU_STARTUP_WIDGET,
            UiAction::SetStartupView(StartupView::Widget),
        ),
        (
            MENU_STARTUP_TRAY,
            UiAction::SetStartupView(StartupView::TrayOnly),
        ),
        (MENU_AUTH_REFRESH, UiAction::RefreshWithAuth),
        (MENU_AUTO_AUTH_REFRESH, UiAction::ToggleAutoAuthRefresh),
        (MENU_ALWAYS_ON_TOP, UiAction::ToggleAlwaysOnTop),
        (
            MENU_LANGUAGE_AUTO,
            UiAction::SetLanguage(LanguagePreference::Auto),
        ),
        (
            MENU_LANGUAGE_KOREAN,
            UiAction::SetLanguage(LanguagePreference::Korean),
        ),
        (
            MENU_LANGUAGE_ENGLISH,
            UiAction::SetLanguage(LanguagePreference::English),
        ),
        (MENU_POSITION_RESET, UiAction::ResetPosition),
        (MENU_DIAGNOSTICS, UiAction::RunDiagnostics),
        (MENU_UPDATE_CHECK, UiAction::CheckForUpdates),
        (MENU_WIDGET_VISIBLE, UiAction::ToggleWidget),
        (MENU_EXIT, UiAction::Exit),
    ];
    for (id, expected) in cases {
        assert_eq!(menu_action(id), Some(expected), "menu id {id}");
    }
    assert_eq!(menu_action(u16::MAX), None);
}

#[test]
fn launch_arguments_are_strict_and_diagnose_wins() {
    assert_eq!(LaunchMode::parse([] as [&str; 0]), Ok(LaunchMode::Normal));
    assert_eq!(LaunchMode::parse(["--startup"]), Ok(LaunchMode::Startup));
    assert_eq!(LaunchMode::parse(["--diagnose"]), Ok(LaunchMode::Diagnose));
    assert!(LaunchMode::parse(["--unknown"]).is_err());
    assert!(LaunchMode::parse(["--startup", "--diagnose"]).is_err());
}

#[test]
fn startup_tray_only_hides_without_changing_the_saved_visibility_preference() {
    assert!(initial_widget_visible(
        LaunchMode::Normal,
        StartupView::TrayOnly,
        true
    ));
    assert!(!initial_widget_visible(
        LaunchMode::Startup,
        StartupView::TrayOnly,
        true
    ));
    assert!(initial_widget_visible(
        LaunchMode::Startup,
        StartupView::Widget,
        true
    ));
    assert!(!initial_widget_visible(
        LaunchMode::Startup,
        StartupView::Widget,
        false
    ));
}

#[test]
fn normal_startup_acquires_instance_before_any_side_effect() {
    assert_eq!(
        startup_plan(LaunchMode::Normal),
        &[
            StartupStep::AcquireSingleInstance,
            StartupStep::LoadSettings,
            StartupStep::StartPoller,
            StartupStep::StartUpdateCheck,
            StartupStep::RunUi,
        ]
    );
    assert_eq!(
        startup_plan(LaunchMode::Diagnose),
        &[StartupStep::RunDiagnostics]
    );
}

#[test]
fn windows_ui_language_resolves_auto_without_process_environment() {
    assert_eq!(
        resolve_windows_language(LanguagePreference::Auto, Some(0x0412), Some("en-US")),
        codex_usage_monitor::Language::Korean
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Auto, None, Some("ko-KR")),
        codex_usage_monitor::Language::Korean
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Auto, Some(0x0409), Some("en-US")),
        codex_usage_monitor::Language::English
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Korean, Some(0x0409), Some("en-US")),
        codex_usage_monitor::Language::Korean
    );
}

#[test]
fn widget_layout_scales_consistently_at_supported_dpis() {
    for (dpi, width, height) in [
        (96, 380, 112),
        (120, 475, 140),
        (144, 570, 168),
        (192, 760, 224),
    ] {
        let layout = WidgetLayout::for_dpi(dpi);
        assert_eq!(
            (layout.window.width(), layout.window.height()),
            (width, height)
        );
        assert!(layout.primary_bar.is_inside(layout.window));
        assert!(layout.secondary_bar.is_inside(layout.window));
        assert!(layout.status.is_inside(layout.window));
        assert!(!layout.primary_bar.intersects(layout.secondary_bar));
        assert!(!layout.secondary_bar.intersects(layout.status));
    }
}

#[test]
fn floating_position_is_clamped_into_the_work_area() {
    let work = Rect::new(100, 50, 1100, 850);
    assert_eq!(
        clamp_floating_position((-50, 900), (380, 112), work),
        (100, 738)
    );
    assert_eq!(
        clamp_floating_position((350, 300), (380, 112), work),
        (350, 300)
    );
}

#[test]
fn floating_coordinates_round_trip_between_logical_and_physical_dpi() {
    assert_eq!(logical_to_physical(240, 144), 360);
    assert_eq!(physical_to_logical(360, 144), 240);
    assert_eq!(physical_to_logical(-151, 120), -121);
}

#[test]
fn positions_are_saved_relative_to_negative_origin_and_restore_at_mixed_dpi() {
    let saved = save_monitor_relative_position((-1_800, 150), (-1_920, 0), 144);
    assert_eq!(
        saved,
        codex_usage_monitor::LogicalPosition { x: 80, y: 100 }
    );
    assert_eq!(
        restore_monitor_relative_position(saved, (0, -900), 120),
        (100, -775)
    );
}

#[test]
fn lifecycle_recreates_destroyed_taskbar_widget_and_cleans_in_safe_order() {
    let mut lifecycle = NativeLifecycle::default();
    lifecycle.owner_created();
    lifecycle.timer_started();
    lifecycle.tray_created();
    lifecycle.widget_created();
    lifecycle.widget_attached_to_taskbar();
    lifecycle.widget_destroyed();

    assert_eq!(
        lifecycle.recovery_decision(RecoveryEvent::TaskbarCreated, true),
        RecoveryDecision::RecreateAndApply
    );
    assert_eq!(
        lifecycle.cleanup_actions(),
        vec![
            CleanupAction::StopTimer,
            CleanupAction::RemoveTray,
            CleanupAction::DestroyOwner,
        ]
    );
}

#[test]
fn floating_transition_recreates_when_detach_is_not_verified() {
    assert_eq!(
        NativeLifecycle::floating_transition(DetachOutcome::DetachedAndVerified),
        FloatingTransition::ReuseAndPlace
    );
    assert_eq!(
        NativeLifecycle::floating_transition(DetachOutcome::ParentRemains),
        FloatingTransition::RecreateAndPlace
    );
    assert_eq!(
        NativeLifecycle::floating_transition(DetachOutcome::ApiFailed),
        FloatingTransition::RecreateAndPlace
    );
}

#[test]
fn release_page_validation_requires_an_exact_github_tag_path() {
    assert!(is_exact_github_tag_page(
        "https://github.com/openai/codex/releases/tag/v1.2.3"
    ));
    for unsafe_url in [
        "https://github.com/openai/codex/releases/tag/v1.2.3/assets",
        "https://github.com/openai/codex/releases/tag/v1.2.3?download=1",
        "https://github.com/openai/codex/releases/tag/../settings",
        "https://github.com@evil.example/openai/codex/releases/tag/v1.2.3",
    ] {
        assert!(!is_exact_github_tag_page(unsafe_url), "{unsafe_url}");
    }
}

#[test]
fn taskbar_placement_handles_offsets_secondary_and_rejections() {
    let primary = TaskbarGeometry {
        taskbar: Rect::new(0, 1040, 1920, 1080),
        notification: Rect::new(1700, 1040, 1920, 1080),
    };
    assert_eq!(
        place_taskbar_widget(primary, (380, 40), 0),
        Ok(Rect::new(1320, 1040, 1700, 1080))
    );
    assert_eq!(
        place_taskbar_widget(primary, (380, 40), -1),
        Err(TaskbarPlacementError::InsufficientSpace)
    );
    let secondary = TaskbarGeometry {
        taskbar: Rect::new(-1280, 984, 0, 1024),
        notification: Rect::new(-180, 984, 0, 1024),
    };
    assert_eq!(
        place_taskbar_widget(secondary, (380, 40), 12),
        Ok(Rect::new(-572, 984, -192, 1024))
    );
    let vertical = TaskbarGeometry {
        taskbar: Rect::new(0, 0, 48, 1080),
        notification: Rect::new(0, 900, 48, 1080),
    };
    assert_eq!(
        place_taskbar_widget(vertical, (380, 48), 0),
        Err(TaskbarPlacementError::VerticalTaskbar)
    );
    let narrow = TaskbarGeometry {
        taskbar: Rect::new(0, 0, 500, 40),
        notification: Rect::new(300, 0, 500, 40),
    };
    assert_eq!(
        place_taskbar_widget(narrow, (380, 40), 0),
        Err(TaskbarPlacementError::InsufficientSpace)
    );
}

#[test]
fn taskbar_attachment_requires_full_height_and_child_style() {
    assert_eq!(
        taskbar_widget_size(40, 96),
        Err(TaskbarPlacementError::InsufficientSpace)
    );
    assert_eq!(taskbar_widget_size(48, 96), Ok((380, 48)));

    const WS_POPUP: u32 = 0x8000_0000;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
    let style = taskbar_child_style(WS_POPUP | 0x0001_0000);
    assert_eq!(style & WS_POPUP, 0);
    assert_eq!(
        style & (WS_CHILD | WS_CLIPSIBLINGS),
        WS_CHILD | WS_CLIPSIBLINGS
    );
}

const ORIGINAL_STYLE: u32 = 0x8001_0000;
const ORIGINAL_PARENT: u8 = 1;
const TARGET_PARENT: u8 = 2;

struct FakeAttachmentBackend {
    style: u32,
    parent: Option<u8>,
    calls: Vec<&'static str>,
    failures: Vec<&'static str>,
    style_reads: usize,
    parent_reads: usize,
    frame_refreshes: usize,
}

impl FakeAttachmentBackend {
    fn new(failures: &[&'static str]) -> Self {
        Self {
            style: ORIGINAL_STYLE,
            parent: Some(ORIGINAL_PARENT),
            calls: Vec::new(),
            failures: failures.to_vec(),
            style_reads: 0,
            parent_reads: 0,
            frame_refreshes: 0,
        }
    }

    fn fails(&self, operation: &str) -> bool {
        self.failures.contains(&operation)
    }
}

impl TaskbarAttachmentBackend for FakeAttachmentBackend {
    type Parent = u8;
    type Error = &'static str;

    fn read_style(&mut self) -> Result<u32, Self::Error> {
        self.style_reads += 1;
        let operation = match self.style_reads {
            1 => "read_original_style",
            2 => "verify_child_style",
            _ => "verify_rollback_style",
        };
        self.calls.push(operation);
        if operation == "verify_child_style" && self.fails(operation) {
            Ok(ORIGINAL_STYLE)
        } else {
            Ok(self.style)
        }
    }

    fn read_parent(&mut self) -> Result<Option<Self::Parent>, Self::Error> {
        self.parent_reads += 1;
        let operation = match self.parent_reads {
            1 => "read_original_parent",
            2 => "verify_target_parent",
            _ => "verify_rollback_parent",
        };
        self.calls.push(operation);
        if operation == "verify_target_parent" && self.fails(operation) {
            Ok(Some(ORIGINAL_PARENT))
        } else {
            Ok(self.parent)
        }
    }

    fn set_style(&mut self, style: u32) -> Result<(), Self::Error> {
        let operation = if style == ORIGINAL_STYLE {
            "rollback_style"
        } else {
            "set_child_style"
        };
        self.calls.push(operation);
        if self.fails(operation) {
            Err(operation)
        } else {
            self.style = style;
            Ok(())
        }
    }

    fn set_parent(&mut self, parent: Option<Self::Parent>) -> Result<(), Self::Error> {
        let operation = if parent == Some(TARGET_PARENT) {
            "set_target_parent"
        } else {
            "rollback_parent"
        };
        self.calls.push(operation);
        if self.fails(operation) {
            Err(operation)
        } else {
            self.parent = parent;
            Ok(())
        }
    }

    fn set_position(&mut self) -> Result<(), Self::Error> {
        self.calls.push("set_position");
        if self.fails("set_position") {
            Err("set_position")
        } else {
            Ok(())
        }
    }

    fn refresh_frame(&mut self) -> Result<(), Self::Error> {
        self.calls.push("refresh_frame");
        self.frame_refreshes += 1;
        if self.fails("refresh_frame") {
            Err("refresh_frame")
        } else {
            Ok(())
        }
    }
}

#[test]
fn taskbar_attachment_transaction_uses_the_verified_production_order() {
    let mut backend = FakeAttachmentBackend::new(&[]);
    run_taskbar_attachment(&mut backend, TARGET_PARENT).unwrap();

    assert_eq!(
        backend.calls,
        vec![
            "read_original_style",
            "read_original_parent",
            "set_child_style",
            "verify_child_style",
            "set_target_parent",
            "verify_target_parent",
            "set_position",
        ]
    );
    assert_eq!(backend.style, taskbar_child_style(ORIGINAL_STYLE));
    assert_eq!(backend.parent, Some(TARGET_PARENT));
}

#[test]
fn taskbar_attachment_transaction_rolls_back_every_failed_stage() {
    let cases = [
        ("set_child_style", TaskbarAttachmentStage::ApplyChildStyle),
        (
            "verify_child_style",
            TaskbarAttachmentStage::VerifyChildStyle,
        ),
        ("set_target_parent", TaskbarAttachmentStage::SetParent),
        ("verify_target_parent", TaskbarAttachmentStage::VerifyParent),
        ("set_position", TaskbarAttachmentStage::SetPosition),
    ];
    for (failure, expected_stage) in cases {
        let mut backend = FakeAttachmentBackend::new(&[failure]);
        let error = run_taskbar_attachment(&mut backend, TARGET_PARENT).unwrap_err();

        assert_eq!(error.failed_stage(), expected_stage, "{failure}");
        assert!(!error.rollback_failed(), "{failure}: {error}");
        assert_eq!(backend.parent, Some(ORIGINAL_PARENT), "{failure}");
        assert_eq!(backend.style, ORIGINAL_STYLE, "{failure}");
        assert_eq!(backend.frame_refreshes, 1, "{failure}");
    }
}

#[test]
fn taskbar_attachment_transaction_reports_rollback_failure_and_keeps_cleaning() {
    let mut backend = FakeAttachmentBackend::new(&["set_position", "rollback_parent"]);
    let error = run_taskbar_attachment(&mut backend, TARGET_PARENT).unwrap_err();

    assert_eq!(error.failed_stage(), TaskbarAttachmentStage::SetPosition);
    assert!(error.rollback_failed());
    assert!(error.to_string().contains("rollback_parent"));
    assert_eq!(backend.style, ORIGINAL_STYLE);
    assert_eq!(backend.frame_refreshes, 1);
}

#[derive(Default)]
struct MemoryRegistry {
    value: Mutex<Option<String>>,
    writes: Mutex<Vec<String>>,
}

impl RegistryBackend for MemoryRegistry {
    fn write(&self, value: &str) -> std::io::Result<()> {
        self.writes.lock().unwrap().push(value.to_owned());
        *self.value.lock().unwrap() = Some(value.to_owned());
        Ok(())
    }

    fn read(&self) -> std::io::Result<Option<String>> {
        Ok(self.value.lock().unwrap().clone())
    }

    fn remove(&self) -> std::io::Result<()> {
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}

#[test]
fn autostart_quotes_exact_executable_and_verifies_round_trip() {
    let path = PathBuf::from(r"C:\Program Files\Codex Usage Monitor\codex-usage-monitor.exe");
    let expected = r#""C:\Program Files\Codex Usage Monitor\codex-usage-monitor.exe" --startup"#;
    assert_eq!(autostart_command(&path).unwrap(), expected);

    let registry = MemoryRegistry::default();
    set_autostart(&registry, true, &path).unwrap();
    assert_eq!(registry.read().unwrap().as_deref(), Some(expected));
    set_autostart(&registry, false, &path).unwrap();
    assert_eq!(registry.read().unwrap(), None);
}

#[test]
fn autostart_rejects_quote_in_executable_path_before_registry_write() {
    let registry = MemoryRegistry::default();
    let error = set_autostart(&registry, true, Path::new("C:\\bad\"path\\app.exe"))
        .expect_err("unsafe path must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(registry.writes.lock().unwrap().is_empty());
}
