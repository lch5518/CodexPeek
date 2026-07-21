use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use codex_usage_monitor::{
    windows::{
        autostart::{autostart_command, set_autostart, RegistryBackend},
        initial_widget_visible, menu_action,
        taskbar::{place_taskbar_widget, TaskbarGeometry, TaskbarPlacementError},
        widget::{
            clamp_floating_position, logical_to_physical, physical_to_logical, Rect, WidgetLayout,
        },
        LaunchMode, UiAction, MENU_ALWAYS_ON_TOP, MENU_AUTH_REFRESH, MENU_AUTOSTART,
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
fn taskbar_placement_handles_offsets_secondary_and_rejections() {
    let primary = TaskbarGeometry {
        taskbar: Rect::new(0, 1040, 1920, 1080),
        notification: Rect::new(1700, 1040, 1920, 1080),
    };
    assert_eq!(
        place_taskbar_widget(primary, (380, 40), 0),
        Ok(Rect::new(1320, 1040, 1700, 1080))
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
