use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use codex_usage_monitor::{
    windows::{
        autostart::{autostart_command, set_autostart, RegistryBackend},
        initial_widget_visible, is_exact_github_tag_page, is_valid_chatgpt_login_url,
        lifecycle::{CleanupAction, NativeLifecycle, RecoveryDecision, RecoveryEvent},
        menu_action,
        native::{
            profile_dialog_ui_action, profile_login_confirmation_request, ProfileLoginDispatch,
        },
        profile_dialog::{
            add_profile_dialog_monitor_anchor, add_profile_prompt_result,
            available_profile_actions, profile_delete_confirmation, profile_dialog_keyboard_result,
            profile_login_confirmation, profile_manager_control_enabled,
            profile_manager_control_spec, profile_manager_dialog_monitor_anchor,
            profile_manager_row_label, validated_label, AddProfilePromptCommand,
            AddProfilePromptState, DialogMonitorAnchor, DialogWindowSize, ModalCleanupAction,
            ModalDialogLifecycle, ProfileDialogAction, ProfileDialogCommand,
            ProfileDialogController, ProfileDialogKeyboardCommand, ProfileDialogKeyboardResult,
            ProfileManagerControl, ProfileManagerDialogState, PROFILE_LABEL_MAX_UTF16_UNITS,
            PROFILE_MANAGER_CONTROLS,
        },
        profile_taskbar_tooltip, resolve_windows_language, startup_plan,
        taskbar::{
            place_taskbar_widget, reconcile_widget_surfaces, run_taskbar_attachment,
            taskbar_widget_size, TaskbarAttachmentBackend, TaskbarAttachmentStage, TaskbarGeometry,
            TaskbarPlacementError, WidgetSurface, WidgetSurfaceBackend,
        },
        taskbar_widget::{
            profile_header_text, select_weekly_row, widget_surface_layout, HoverTransition,
            TaskbarLayout, TaskbarLayoutMode, TaskbarRisk,
        },
        tray::{
            language_menu_label, tray_menu_entries, tray_menu_model, update_menu_text,
            TrayMenuEntry,
        },
        widget::{logical_to_physical, Rect},
        LaunchMode, StartupStep, UiAction, UiSettings, UsageProfileView, WidgetDataState,
        WidgetViewModel, MENU_ADD_USAGE_PROFILE, MENU_AUTH_REFRESH, MENU_AUTOSTART,
        MENU_AUTO_AUTH_REFRESH, MENU_DIAGNOSTICS, MENU_EXIT, MENU_INTERVAL_1, MENU_INTERVAL_10,
        MENU_INTERVAL_15, MENU_INTERVAL_30, MENU_INTERVAL_5, MENU_LANGUAGE_ARABIC,
        MENU_LANGUAGE_AUTO, MENU_LANGUAGE_ENGLISH, MENU_LANGUAGE_FRENCH, MENU_LANGUAGE_GERMAN,
        MENU_LANGUAGE_HINDI, MENU_LANGUAGE_INDONESIAN, MENU_LANGUAGE_JAPANESE,
        MENU_LANGUAGE_KOREAN, MENU_LANGUAGE_PORTUGUESE_BRAZIL, MENU_LANGUAGE_SPANISH,
        MENU_LANGUAGE_TURKISH, MENU_LANGUAGE_VIETNAMESE, MENU_LOGIN, MENU_MANAGE_USAGE_PROFILES,
        MENU_REFRESH, MENU_SHOW_REMAINING, MENU_STARTUP_TRAY, MENU_STARTUP_WIDGET,
        MENU_TASKBAR_ALL, MENU_TASKBAR_PRIMARY, MENU_UPDATE_CHECK, MENU_WIDGET_VISIBLE,
    },
    Language, LanguagePreference, ProfileValidationError, StartupView, TaskbarDisplayMode,
    UpdatePresentationStatus, UsageProfileId,
};
use windows::Win32::Foundation::HWND;

fn system_profile_view() -> UsageProfileView {
    UsageProfileView {
        id: UsageProfileId::System,
        label: "Default Codex account".to_string(),
        summary: "Displayed".to_string(),
        selected: true,
        login_required: false,
        managed: false,
    }
}

#[test]
fn profile_dialog_centering_uses_the_cursor_or_a_live_owner_as_its_monitor_anchor() {
    let owner = HWND(42_usize as _);

    assert_eq!(
        profile_manager_dialog_monitor_anchor(),
        DialogMonitorAnchor::Cursor
    );
    assert_eq!(
        add_profile_dialog_monitor_anchor(owner, Some(DialogWindowSize::new(560, 180))),
        DialogMonitorAnchor::Owner(owner)
    );
    assert_eq!(
        add_profile_dialog_monitor_anchor(owner, Some(DialogWindowSize::new(0, 180))),
        DialogMonitorAnchor::Cursor
    );
    assert_eq!(
        add_profile_dialog_monitor_anchor(HWND::default(), Some(DialogWindowSize::new(560, 180))),
        DialogMonitorAnchor::Cursor
    );
    assert_eq!(
        add_profile_dialog_monitor_anchor(owner, None),
        DialogMonitorAnchor::Cursor
    );
}

#[test]
fn system_profile_offers_rename_but_not_logout_or_delete() {
    let actions = available_profile_actions(&system_profile_view());

    assert!(actions.contains(&ProfileDialogCommand::Rename));
    assert!(actions.contains(&ProfileDialogCommand::Login));
    assert!(!actions.contains(&ProfileDialogCommand::Delete));
    assert!(!actions.contains(&ProfileDialogCommand::Logout));

    let mut inconsistent_view = system_profile_view();
    inconsistent_view.managed = true;
    let actions = available_profile_actions(&inconsistent_view);
    assert!(actions.contains(&ProfileDialogCommand::Rename));
    assert!(!actions.contains(&ProfileDialogCommand::Delete));
    assert!(!actions.contains(&ProfileDialogCommand::Logout));
}

#[test]
fn dialog_labels_use_shared_validation() {
    assert_eq!(validated_label("  Work  ").unwrap(), "Work");
    assert_eq!(
        validated_label("bad\\name"),
        Err(ProfileValidationError::InvalidLabel)
    );
}

#[test]
fn add_prompt_cancel_emits_no_action_and_submit_validates_the_name() {
    use codex_usage_monitor::windows::profile_dialog::ProfileDialogAction;

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

#[test]
fn profile_manager_controls_exclude_bottom_add_and_close() {
    assert_eq!(
        PROFILE_MANAGER_CONTROLS,
        [
            ProfileManagerControl::AddBelowList,
            ProfileManagerControl::Rename,
            ProfileManagerControl::Login,
            ProfileManagerControl::Logout,
            ProfileManagerControl::Delete,
        ]
    );
}

#[test]
fn profile_manager_add_enablement_follows_can_add_exactly() {
    let available = ProfileDialogController::new(&[system_profile_view()], false);
    let pending = ProfileDialogController::new(&[system_profile_view()], true);
    let mut full_profiles = vec![system_profile_view()];
    for sequence in 1..8 {
        full_profiles.push(UsageProfileView {
            id: UsageProfileId::Managed(sequence),
            label: format!("Profile {sequence}"),
            summary: String::new(),
            selected: false,
            login_required: true,
            managed: true,
        });
    }
    let full = ProfileDialogController::new(&full_profiles, false);

    assert!(profile_manager_control_enabled(
        &available,
        ProfileManagerControl::AddBelowList
    ));
    assert!(!profile_manager_control_enabled(
        &pending,
        ProfileManagerControl::AddBelowList
    ));
    assert!(!profile_manager_control_enabled(
        &full,
        ProfileManagerControl::AddBelowList
    ));
}

#[test]
fn active_add_prompt_rejects_a_second_child() {
    let mut manager = ProfileManagerDialogState::new();

    assert!(manager.begin_add_prompt(true));
    assert!(!manager.accepts_manager_commands());
    assert!(!manager.begin_add_prompt(true));
}

#[test]
fn add_prompt_rejects_reentrant_submit_and_close_while_warning_is_open() {
    let mut prompt = AddProfilePromptState::new();

    assert!(prompt.begin_command());
    assert!(!prompt.accepts_commands());
    assert!(!prompt.begin_command());
    assert!(prompt.begin_warning());
    assert!(!prompt.begin_command());
    assert!(prompt.finish_warning());
    assert!(prompt.accepts_commands());

    assert!(prompt.begin_command());
    assert!(prompt.finish_close());
    assert!(!prompt.accepts_commands());
    assert!(!prompt.begin_command());
}

#[test]
fn add_control_keeps_plus_text_and_uses_the_localized_add_description() {
    for language in Language::ALL {
        let spec = profile_manager_control_spec(ProfileManagerControl::AddBelowList, *language);

        assert_eq!(spec.visible_text, "+");
        assert_eq!(
            spec.accessible_description,
            Some(codex_usage_monitor::localized_text(
                codex_usage_monitor::LocalizationKey::MenuAddUsageProfile,
                *language,
            ))
        );
    }
}

#[test]
fn cancelled_add_prompt_restores_the_live_manager() {
    let mut manager = ProfileManagerDialogState::new();
    assert!(manager.begin_add_prompt(true));

    assert!(manager.finish_add_prompt(None));
    assert!(manager.accepts_manager_commands());
    assert_eq!(manager.take_result(), None);
}

#[test]
fn submitted_add_prompt_returns_exactly_one_action() {
    let mut manager = ProfileManagerDialogState::new();
    assert!(manager.begin_add_prompt(true));

    assert!(manager.finish_add_prompt(Some(ProfileDialogAction::Add("Work".to_owned()))));
    assert!(!manager.finish_add_prompt(Some(ProfileDialogAction::Add("Duplicate".to_owned()))));
    assert!(!manager.accepts_manager_commands());
    assert_eq!(
        manager.take_result(),
        Some(ProfileDialogAction::Add("Work".to_owned()))
    );
    assert_eq!(manager.take_result(), None);
}

#[test]
fn dialog_edit_capacity_preserves_labels_across_the_old_astral_boundary() {
    let twenty_astral = "😀".repeat(20);
    let twenty_one_astral = "😀".repeat(21);

    assert_eq!(twenty_astral.encode_utf16().count(), 40);
    assert_eq!(twenty_one_astral.encode_utf16().count(), 42);
    assert_eq!(validated_label(&twenty_astral).unwrap(), twenty_astral);
    assert_eq!(
        validated_label(&twenty_one_astral).unwrap(),
        twenty_one_astral
    );
    assert!(twenty_one_astral.encode_utf16().count() <= PROFILE_LABEL_MAX_UTF16_UNITS);
}

#[test]
fn dialog_edit_capacity_preserves_the_worst_case_forty_scalar_label() {
    let forty_astral = "😀".repeat(40);

    assert_eq!(forty_astral.chars().count(), 40);
    assert_eq!(
        forty_astral.encode_utf16().count(),
        PROFILE_LABEL_MAX_UTF16_UNITS
    );
    assert_eq!(validated_label(&forty_astral).unwrap(), forty_astral);
}

#[test]
fn pending_profile_mutation_disables_every_mutating_control() {
    let controller = ProfileDialogController::new(&[system_profile_view()], true);

    assert!(!controller.can_add());
    for command in [
        ProfileDialogCommand::Rename,
        ProfileDialogCommand::Login,
        ProfileDialogCommand::Logout,
        ProfileDialogCommand::Delete,
    ] {
        assert!(!controller.command_enabled(command));
    }
}

#[test]
fn modal_cleanup_destroys_live_window_and_restores_only_an_owner_it_disabled() {
    let mut active_owner = ModalDialogLifecycle::new(true, true);
    active_owner.window_created();
    assert!(active_owner.should_disable_owner());
    active_owner.owner_disabled();
    assert_eq!(
        active_owner.cleanup_actions(),
        vec![
            ModalCleanupAction::ClearWindowState,
            ModalCleanupAction::DestroyWindow,
            ModalCleanupAction::RestoreOwner,
        ]
    );

    active_owner.window_destroyed();
    assert_eq!(
        active_owner.cleanup_actions(),
        vec![ModalCleanupAction::RestoreOwner]
    );

    let mut already_disabled_owner = ModalDialogLifecycle::new(true, false);
    already_disabled_owner.window_created();
    assert!(!already_disabled_owner.should_disable_owner());
    assert_eq!(
        already_disabled_owner.cleanup_actions(),
        vec![
            ModalCleanupAction::ClearWindowState,
            ModalCleanupAction::DestroyWindow,
        ]
    );
}

#[test]
fn dialog_keyboard_cancel_closes_without_action_and_accept_is_explicitly_ignored() {
    assert_eq!(
        profile_dialog_keyboard_result(ProfileDialogKeyboardCommand::Cancel),
        ProfileDialogKeyboardResult::CloseWithoutAction
    );
    assert_eq!(
        profile_dialog_keyboard_result(ProfileDialogKeyboardCommand::Accept),
        ProfileDialogKeyboardResult::Ignore
    );
}

#[test]
fn profile_dialog_enforces_the_eight_profile_limit() {
    let mut profiles = vec![system_profile_view()];
    for sequence in 1..8 {
        profiles.push(UsageProfileView {
            id: UsageProfileId::Managed(sequence),
            label: format!("Profile {sequence}"),
            summary: String::new(),
            selected: false,
            login_required: true,
            managed: true,
        });
    }

    assert!(!ProfileDialogController::new(&profiles, false).can_add());
    profiles.pop();
    assert!(ProfileDialogController::new(&profiles, false).can_add());
}

#[test]
fn profile_dialog_controls_follow_the_selected_profile_state() {
    let profiles = vec![
        system_profile_view(),
        UsageProfileView {
            id: UsageProfileId::Managed(1),
            label: "Signed in".to_string(),
            summary: String::new(),
            selected: false,
            login_required: false,
            managed: true,
        },
        UsageProfileView {
            id: UsageProfileId::Managed(2),
            label: "Signed out".to_string(),
            summary: String::new(),
            selected: false,
            login_required: true,
            managed: true,
        },
    ];
    let mut controller = ProfileDialogController::new(&profiles, false);

    assert!(controller.command_enabled(ProfileDialogCommand::Rename));
    assert!(!controller.command_enabled(ProfileDialogCommand::Logout));
    assert!(!controller.command_enabled(ProfileDialogCommand::Delete));

    assert!(controller.select(1));
    assert!(controller.command_enabled(ProfileDialogCommand::Rename));
    assert!(controller.command_enabled(ProfileDialogCommand::Login));
    assert!(controller.command_enabled(ProfileDialogCommand::Logout));
    assert!(controller.command_enabled(ProfileDialogCommand::Delete));

    assert!(controller.select(2));
    assert!(controller.command_enabled(ProfileDialogCommand::Rename));
    assert!(controller.command_enabled(ProfileDialogCommand::Login));
    assert!(!controller.command_enabled(ProfileDialogCommand::Logout));
    assert!(controller.command_enabled(ProfileDialogCommand::Delete));
}

#[test]
fn profile_dialog_actions_use_the_current_selection_without_stale_identity() {
    use codex_usage_monitor::windows::profile_dialog::ProfileDialogAction;

    let profiles = vec![
        system_profile_view(),
        UsageProfileView {
            id: UsageProfileId::Managed(11),
            label: "One".to_string(),
            summary: String::new(),
            selected: false,
            login_required: false,
            managed: true,
        },
        UsageProfileView {
            id: UsageProfileId::Managed(12),
            label: "Two".to_string(),
            summary: String::new(),
            selected: false,
            login_required: true,
            managed: true,
        },
    ];
    let mut controller = ProfileDialogController::new(&profiles, false);

    assert!(controller.select(1));
    assert_eq!(
        controller.submit_rename("First"),
        Ok(Some(ProfileDialogAction::Rename(
            UsageProfileId::Managed(11),
            "First".to_string(),
        )))
    );

    assert!(controller.select(2));
    assert_eq!(
        controller.confirmed_command(ProfileDialogCommand::Delete, true),
        Some(ProfileDialogAction::Delete(UsageProfileId::Managed(12)))
    );
    assert!(!controller.select(99));
    assert_eq!(
        controller.confirmed_command(ProfileDialogCommand::Login, true),
        Some(ProfileDialogAction::Login(UsageProfileId::Managed(12)))
    );
}

#[test]
fn dialog_controller_emits_only_confirmed_typed_profile_actions() {
    let managed = UsageProfileView {
        id: UsageProfileId::Managed(7),
        label: "Work".to_string(),
        summary: "Displayed".to_string(),
        selected: true,
        login_required: false,
        managed: true,
    };
    let controller = ProfileDialogController::new(&[managed], false);

    assert_eq!(
        controller.submit_add("  Personal  "),
        Ok(Some(
            codex_usage_monitor::windows::profile_dialog::ProfileDialogAction::Add(
                "Personal".to_string()
            )
        ))
    );
    assert_eq!(
        controller.submit_rename("  Team  "),
        Ok(Some(
            codex_usage_monitor::windows::profile_dialog::ProfileDialogAction::Rename(
                UsageProfileId::Managed(7),
                "Team".to_string()
            )
        ))
    );
    assert_eq!(
        controller.confirmed_command(ProfileDialogCommand::Login, false),
        None
    );
    assert_eq!(
        controller.confirmed_command(ProfileDialogCommand::Delete, true),
        Some(
            codex_usage_monitor::windows::profile_dialog::ProfileDialogAction::Delete(
                UsageProfileId::Managed(7)
            )
        )
    );
}

#[test]
fn profile_confirmations_name_the_chosen_profile_and_scope_side_effects() {
    let login = profile_login_confirmation("Work", Language::English);
    let delete = profile_delete_confirmation("Work", Language::English);

    assert!(login.contains("Work"));
    assert!(login.contains("browser"));
    assert!(login.contains("CLI and IDE sign-ins are unchanged"));
    assert!(delete.contains("Work"));
    assert!(delete.contains("local profile data cannot be recovered"));

    for language in Language::ALL {
        assert!(profile_login_confirmation("Work", *language).contains("Work"));
        assert!(profile_delete_confirmation("Work", *language).contains("Work"));
    }
}

#[test]
fn profile_dialog_actions_map_to_task_six_ui_intents() {
    use codex_usage_monitor::windows::profile_dialog::ProfileDialogAction;

    let id = UsageProfileId::Managed(3);
    let cases = [
        (
            ProfileDialogAction::Add("Work".to_string()),
            UiAction::AddUsageProfile("Work".to_string()),
        ),
        (
            ProfileDialogAction::Rename(id, "Team".to_string()),
            UiAction::RenameUsageProfile(id, "Team".to_string()),
        ),
        (
            ProfileDialogAction::Login(id),
            UiAction::LoginUsageProfile(id),
        ),
        (
            ProfileDialogAction::Logout(id),
            UiAction::LogoutUsageProfile(id),
        ),
        (
            ProfileDialogAction::Delete(id),
            UiAction::DeleteUsageProfile(id),
        ),
    ];

    for (dialog, expected) in cases {
        assert_eq!(profile_dialog_ui_action(dialog), expected);
    }
}

#[test]
fn add_profile_confirmation_identifies_label_and_cancel_still_creates_without_login() {
    let settings = tray_settings_with_profiles();
    let action = UiAction::AddUsageProfile("Personal".to_string());
    let request = profile_login_confirmation_request(&action, &settings).unwrap();

    assert_eq!(request.label(), "Personal");
    assert_eq!(
        request.clone().resolve(false),
        Some(ProfileLoginDispatch::Normal(action.clone()))
    );
    assert_eq!(
        request.resolve(true),
        Some(ProfileLoginDispatch::Confirmed(action))
    );
}

#[test]
fn top_level_login_confirmation_uses_selected_profile_and_cancel_dispatches_nothing() {
    let mut settings = tray_settings_with_profiles();
    settings.usage_profiles[0].selected = false;
    settings.usage_profiles[1].selected = true;
    let request = profile_login_confirmation_request(&UiAction::Login, &settings).unwrap();

    assert_eq!(request.label(), "Work");
    assert_eq!(request.clone().resolve(false), None);
    assert_eq!(
        request.resolve(true),
        Some(ProfileLoginDispatch::Confirmed(
            UiAction::LoginUsageProfile(UsageProfileId::Managed(1))
        ))
    );
}

#[test]
fn manager_login_confirmation_is_centralized_and_resolves_once() {
    use codex_usage_monitor::windows::profile_dialog::ProfileDialogAction;

    let settings = tray_settings_with_profiles();
    let action = profile_dialog_ui_action(ProfileDialogAction::Login(UsageProfileId::Managed(1)));
    let request = profile_login_confirmation_request(&action, &settings).unwrap();

    assert_eq!(request.label(), "Work");
    assert_eq!(request.clone().resolve(false), None);
    assert_eq!(
        request.resolve(true),
        Some(ProfileLoginDispatch::Confirmed(action))
    );
}

#[test]
fn update_menu_labels_surface_every_presentation_status() {
    let cases = [
        (UpdatePresentationStatus::Idle, "Check for updates"),
        (UpdatePresentationStatus::Checking, "Checking for updates"),
        (
            UpdatePresentationStatus::Available,
            "An update is available",
        ),
        (UpdatePresentationStatus::Current, "You are up to date"),
        (UpdatePresentationStatus::Failed, "Update check failed"),
    ];
    for (status, expected) in cases {
        assert_eq!(update_menu_text(status, Language::English), expected);
        assert!(!update_menu_text(status, Language::Korean).is_empty());
    }
}

#[test]
fn language_menu_labels_always_show_endonyms() {
    assert_eq!(
        language_menu_label(LanguagePreference::Auto, Language::Korean),
        "언어: 자동"
    );
    assert_eq!(
        language_menu_label(LanguagePreference::Korean, Language::Korean),
        "언어: 한국어"
    );
    assert_eq!(
        language_menu_label(LanguagePreference::English, Language::Korean),
        "언어: English"
    );
    assert_eq!(
        language_menu_label(LanguagePreference::Auto, Language::English),
        "Language: automatic"
    );
    assert_eq!(
        language_menu_label(LanguagePreference::Korean, Language::English),
        "Language: 한국어"
    );
    assert_eq!(
        language_menu_label(LanguagePreference::English, Language::English),
        "Language: English"
    );
}

#[test]
fn tray_menu_groups_major_settings_into_submenus() {
    let entries = tray_menu_entries(&tray_settings(Language::English));
    let submenus = entries
        .iter()
        .filter_map(|entry| match entry {
            TrayMenuEntry::Submenu(submenu) => Some(submenu),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        submenus
            .iter()
            .map(|submenu| submenu.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Usage profiles",
            "Refresh interval",
            "Startup view",
            "Language",
            "Widget placement"
        ]
    );
    assert_eq!(
        submenu_command_ids(submenus[0]),
        [1000, MENU_MANAGE_USAGE_PROFILES]
    );
    assert_eq!(
        submenu_command_ids(submenus[1]),
        [
            MENU_INTERVAL_1,
            MENU_INTERVAL_5,
            MENU_INTERVAL_10,
            MENU_INTERVAL_15,
            MENU_INTERVAL_30
        ]
    );
    assert_eq!(
        submenu_command_ids(submenus[2]),
        [MENU_STARTUP_WIDGET, MENU_STARTUP_TRAY]
    );
    assert_eq!(
        submenu_command_ids(submenus[3]),
        [
            MENU_LANGUAGE_AUTO,
            MENU_LANGUAGE_KOREAN,
            MENU_LANGUAGE_ENGLISH,
            MENU_LANGUAGE_SPANISH,
            MENU_LANGUAGE_PORTUGUESE_BRAZIL,
            MENU_LANGUAGE_INDONESIAN,
            MENU_LANGUAGE_JAPANESE,
            MENU_LANGUAGE_HINDI,
            MENU_LANGUAGE_GERMAN,
            MENU_LANGUAGE_FRENCH,
            MENU_LANGUAGE_VIETNAMESE,
            MENU_LANGUAGE_TURKISH,
            MENU_LANGUAGE_ARABIC,
        ]
    );
    assert_eq!(
        submenu_command_ids(submenus[4]),
        [MENU_TASKBAR_ALL, MENU_TASKBAR_PRIMARY]
    );
}

#[test]
fn tray_menu_entries_localize_english_labels_and_preserve_state() {
    let settings = tray_settings(Language::English);
    let commands = tray_commands(&settings);

    assert_eq!(separator_count(&settings), 3);
    assert_eq!(
        commands,
        vec![
            (1000, "Default Codex account    Displayed".to_string(), true),
            (
                MENU_MANAGE_USAGE_PROFILES,
                "Manage usage profiles".to_string(),
                false
            ),
            (MENU_REFRESH, "Refresh now".to_string(), false),
            (MENU_INTERVAL_1, "1 min".to_string(), false),
            (MENU_INTERVAL_5, "5 min".to_string(), false),
            (MENU_INTERVAL_10, "10 min".to_string(), false),
            (MENU_INTERVAL_15, "15 min".to_string(), true),
            (MENU_INTERVAL_30, "30 min".to_string(), false),
            (MENU_AUTOSTART, "Start with Windows".to_string(), true),
            (MENU_STARTUP_WIDGET, "Show widget".to_string(), false),
            (MENU_STARTUP_TRAY, "Tray only".to_string(), true),
            (
                MENU_AUTH_REFRESH,
                "Refresh authentication".to_string(),
                false
            ),
            (
                MENU_AUTO_AUTH_REFRESH,
                "Automatic authentication refresh".to_string(),
                true,
            ),
            (MENU_LANGUAGE_AUTO, "Automatic".to_string(), false),
            (MENU_LANGUAGE_KOREAN, "한국어".to_string(), false),
            (MENU_LANGUAGE_ENGLISH, "English".to_string(), true),
            (MENU_LANGUAGE_SPANISH, "Español".to_string(), false),
            (
                MENU_LANGUAGE_PORTUGUESE_BRAZIL,
                "Português (Brasil)".to_string(),
                false,
            ),
            (
                MENU_LANGUAGE_INDONESIAN,
                "Bahasa Indonesia".to_string(),
                false,
            ),
            (MENU_LANGUAGE_JAPANESE, "日本語".to_string(), false),
            (MENU_LANGUAGE_HINDI, "हिन्दी".to_string(), false),
            (MENU_LANGUAGE_GERMAN, "Deutsch".to_string(), false),
            (MENU_LANGUAGE_FRENCH, "Français".to_string(), false),
            (MENU_LANGUAGE_VIETNAMESE, "Tiếng Việt".to_string(), false),
            (MENU_LANGUAGE_TURKISH, "Türkçe".to_string(), false),
            (MENU_LANGUAGE_ARABIC, "العربية".to_string(), false),
            (MENU_SHOW_REMAINING, "Show weekly usage".to_string(), false),
            (MENU_DIAGNOSTICS, "Diagnostics".to_string(), false),
            (MENU_UPDATE_CHECK, "Update check failed".to_string(), false),
            (MENU_WIDGET_VISIBLE, "Show widget".to_string(), false),
            (MENU_TASKBAR_ALL, "All monitors".to_string(), false),
            (
                MENU_TASKBAR_PRIMARY,
                "Primary monitor only".to_string(),
                true
            ),
            (MENU_EXIT, "Exit".to_string(), false),
        ]
    );
}

#[test]
fn tray_menu_entries_localize_korean_labels_and_preserve_endonyms() {
    let mut settings = tray_settings(Language::Korean);
    settings.language = LanguagePreference::Auto;
    settings.widget_visible = true;
    settings.show_remaining_percent = false;
    settings.taskbar_display_mode = TaskbarDisplayMode::All;

    let commands = tray_commands(&settings);

    assert!(commands.contains(&(MENU_REFRESH, "지금 갱신".to_string(), false)));
    assert!(commands.contains(&(MENU_INTERVAL_15, "15분".to_string(), true)));
    assert!(commands.contains(&(MENU_AUTOSTART, "Windows 시작 시 실행".to_string(), true)));
    assert!(commands.contains(&(MENU_STARTUP_TRAY, "트레이에만 표시".to_string(), true)));
    assert!(commands.contains(&(MENU_AUTH_REFRESH, "인증 갱신".to_string(), false)));
    assert!(commands.contains(&(MENU_LANGUAGE_AUTO, "자동".to_string(), true)));
    assert!(commands.contains(&(MENU_LANGUAGE_KOREAN, "한국어".to_string(), false)));
    assert!(commands.contains(&(MENU_LANGUAGE_ENGLISH, "English".to_string(), false)));
    assert!(commands.contains(&(MENU_SHOW_REMAINING, "남은 사용량 표시".to_string(), false)));
    assert!(commands.contains(&(MENU_WIDGET_VISIBLE, "위젯 숨기기".to_string(), true)));
    assert!(commands.contains(&(MENU_TASKBAR_ALL, "모든 모니터".to_string(), true)));
    assert!(commands.contains(&(MENU_EXIT, "종료".to_string(), false)));
}

#[test]
fn tray_menu_entries_offer_login_instead_of_auth_refresh_when_signed_out() {
    let mut settings = tray_settings(Language::English);
    settings.login_required = true;

    let commands = tray_commands(&settings);

    assert!(commands.contains(&(MENU_LOGIN, "Sign in to Codex".to_string(), false)));
    assert!(!commands.iter().any(|(id, _, _)| *id == MENU_AUTH_REFRESH));
}

fn tray_settings(language: Language) -> UiSettings {
    UiSettings {
        widget_visible: false,
        refresh_interval_minutes: 15,
        start_with_windows: true,
        startup_view: StartupView::TrayOnly,
        auto_auth_refresh: true,
        language: LanguagePreference::English,
        resolved_language: language,
        taskbar_offset: 0,
        taskbar_display_mode: TaskbarDisplayMode::Primary,
        update_status: UpdatePresentationStatus::Failed,
        show_remaining_percent: true,
        login_required: false,
        usage_profiles: vec![UsageProfileView {
            id: UsageProfileId::System,
            label: codex_usage_monitor::localized_text(
                codex_usage_monitor::LocalizationKey::UsageProfileSystem,
                language,
            )
            .to_string(),
            summary: if language == Language::Korean {
                "표시 중".to_string()
            } else {
                "Displayed".to_string()
            },
            selected: true,
            login_required: false,
            managed: false,
        }],
        usage_profile_mutation_pending: false,
    }
}

fn tray_settings_with_profiles() -> UiSettings {
    let mut settings = tray_settings(Language::English);
    settings.usage_profiles.push(UsageProfileView {
        id: UsageProfileId::Managed(1),
        label: "Work".to_string(),
        summary: "Weekly 72% remaining".to_string(),
        selected: false,
        login_required: false,
        managed: true,
    });
    settings
}

#[test]
fn popup_profile_action_keeps_the_profile_identity() {
    let model = tray_menu_model(&tray_settings_with_profiles());

    assert_eq!(
        model.action(1001),
        Some(UiAction::SelectUsageProfile(UsageProfileId::Managed(1)))
    );
    assert_eq!(model.action(MENU_ADD_USAGE_PROFILE), None);
    assert_eq!(
        model.action(MENU_MANAGE_USAGE_PROFILES),
        Some(UiAction::OpenManageUsageProfiles)
    );
}

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

    let default_system = UsageProfileView {
        label: "Default Codex account".to_owned(),
        ..system.clone()
    };
    assert_eq!(
        profile_manager_row_label(&default_system, Language::English),
        "Default Codex account"
    );

    let managed = UsageProfileView {
        id: UsageProfileId::Managed(1),
        ..system.clone()
    };
    assert_eq!(
        profile_manager_row_label(&managed, Language::English),
        "Main"
    );
}

#[test]
fn detached_widget_persistently_consumes_the_selected_profile_label() {
    let view = WidgetViewModel {
        usage_profile_label: "Work".to_string(),
        primary: None,
        secondary: None,
        status: "Polling".to_string(),
        last_success: String::new(),
        is_stale: false,
        taskbar_label: "Weekly usage".to_string(),
        taskbar_tooltip: String::new(),
        reset_credits_text: None,
        data_state: WidgetDataState::Loading,
    };
    let attached = widget_surface_layout(208, 48, 96, true);
    let detached = widget_surface_layout(208, 48, 96, false);

    assert_eq!(attached.profile_header, None);
    assert_eq!(attached.content, Rect::new(0, 0, 208, 48));
    assert_eq!(profile_header_text(&view, attached), None);
    assert_eq!(detached.profile_header, Some(Rect::new(8, 2, 200, 18)));
    assert_eq!(detached.content, Rect::new(0, 18, 208, 48));
    assert_eq!(profile_header_text(&view, detached), Some("Work"));
}

#[test]
fn profile_mutation_actions_keep_ids_and_validated_labels_typed() {
    let id = UsageProfileId::Managed(7);
    let actions = [
        UiAction::AddUsageProfile("Personal".to_string()),
        UiAction::RenameUsageProfile(id, "Work".to_string()),
        UiAction::LoginUsageProfile(id),
        UiAction::LogoutUsageProfile(id),
        UiAction::DeleteUsageProfile(id),
    ];

    assert_eq!(
        actions[0],
        UiAction::AddUsageProfile("Personal".to_string())
    );
    assert_eq!(
        actions[1],
        UiAction::RenameUsageProfile(id, "Work".to_string())
    );
    assert_eq!(actions[2], UiAction::LoginUsageProfile(id));
    assert_eq!(actions[3], UiAction::LogoutUsageProfile(id));
    assert_eq!(actions[4], UiAction::DeleteUsageProfile(id));
}

#[test]
fn profile_tooltip_adds_identity_without_changing_taskbar_dimensions() {
    let tooltip =
        profile_taskbar_tooltip("Work", "Codex 7d usage\nRemaining: 72%", Language::English);

    assert!(tooltip.starts_with("Usage profiles: Work\nCodex CLI sign-in is unchanged\n"));
    assert!(tooltip.ends_with("Codex 7d usage\nRemaining: 72%"));
    assert_eq!(taskbar_widget_size(48, 96), Ok((208, 48)));
}

fn tray_commands(settings: &UiSettings) -> Vec<(u16, String, bool)> {
    fn collect(entries: &[TrayMenuEntry], commands: &mut Vec<(u16, String, bool)>) {
        for entry in entries {
            match entry {
                TrayMenuEntry::Command(command) => {
                    commands.push((command.id, command.label.clone(), command.checked));
                }
                TrayMenuEntry::Submenu(submenu) => collect(&submenu.entries, commands),
                TrayMenuEntry::Separator => {}
            }
        }
    }

    let entries = tray_menu_entries(settings);
    let mut commands = Vec::new();
    collect(&entries, &mut commands);
    commands
}

fn separator_count(settings: &UiSettings) -> usize {
    tray_menu_entries(settings)
        .into_iter()
        .filter(|entry| matches!(entry, TrayMenuEntry::Separator))
        .count()
}

fn submenu_command_ids(submenu: &codex_usage_monitor::windows::tray::TraySubmenu) -> Vec<u16> {
    submenu
        .entries
        .iter()
        .map(|entry| match entry {
            TrayMenuEntry::Command(command) => command.id,
            _ => panic!("settings submenus must contain commands only"),
        })
        .collect()
}

fn usage_profile_submenu(
    model: &codex_usage_monitor::windows::tray::TrayMenuModel,
) -> &codex_usage_monitor::windows::tray::TraySubmenu {
    model
        .entries
        .iter()
        .find_map(|entry| match entry {
            TrayMenuEntry::Submenu(submenu) if submenu.label == "Usage profiles" => Some(submenu),
            _ => None,
        })
        .expect("usage profile submenu is present")
}

#[test]
fn every_menu_command_maps_to_a_typed_action() {
    let cases = [
        (MENU_REFRESH, UiAction::Refresh),
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
        (MENU_LOGIN, UiAction::Login),
        (MENU_AUTO_AUTH_REFRESH, UiAction::ToggleAutoAuthRefresh),
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
        (
            MENU_LANGUAGE_SPANISH,
            UiAction::SetLanguage(LanguagePreference::Spanish),
        ),
        (
            MENU_LANGUAGE_PORTUGUESE_BRAZIL,
            UiAction::SetLanguage(LanguagePreference::PortugueseBrazil),
        ),
        (
            MENU_LANGUAGE_INDONESIAN,
            UiAction::SetLanguage(LanguagePreference::Indonesian),
        ),
        (
            MENU_LANGUAGE_JAPANESE,
            UiAction::SetLanguage(LanguagePreference::Japanese),
        ),
        (
            MENU_LANGUAGE_HINDI,
            UiAction::SetLanguage(LanguagePreference::Hindi),
        ),
        (
            MENU_LANGUAGE_GERMAN,
            UiAction::SetLanguage(LanguagePreference::German),
        ),
        (
            MENU_LANGUAGE_FRENCH,
            UiAction::SetLanguage(LanguagePreference::French),
        ),
        (
            MENU_LANGUAGE_VIETNAMESE,
            UiAction::SetLanguage(LanguagePreference::Vietnamese),
        ),
        (
            MENU_LANGUAGE_TURKISH,
            UiAction::SetLanguage(LanguagePreference::Turkish),
        ),
        (
            MENU_LANGUAGE_ARABIC,
            UiAction::SetLanguage(LanguagePreference::Arabic),
        ),
        (MENU_DIAGNOSTICS, UiAction::RunDiagnostics),
        (MENU_UPDATE_CHECK, UiAction::CheckForUpdates),
        (MENU_WIDGET_VISIBLE, UiAction::ToggleWidget),
        (MENU_SHOW_REMAINING, UiAction::ToggleShowRemaining),
        (
            MENU_TASKBAR_ALL,
            UiAction::SetTaskbarDisplayMode(TaskbarDisplayMode::All),
        ),
        (
            MENU_TASKBAR_PRIMARY,
            UiAction::SetTaskbarDisplayMode(TaskbarDisplayMode::Primary),
        ),
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
    let langid_cases = [
        (0x0412, Language::Korean),
        (0x0409, Language::English),
        (0x0c0a, Language::Spanish),
        (0x0416, Language::PortugueseBrazil),
        (0x0421, Language::Indonesian),
        (0x0411, Language::Japanese),
        (0x0439, Language::Hindi),
        (0x0407, Language::German),
        (0x040c, Language::French),
        (0x042a, Language::Vietnamese),
        (0x041f, Language::Turkish),
        (0x0401, Language::Arabic),
    ];
    for (langid, expected) in langid_cases {
        assert_eq!(
            resolve_windows_language(LanguagePreference::Auto, Some(langid), None),
            expected,
            "LANGID {langid:#06x}"
        );
    }

    let locale_cases = [
        ("ko_KR", Language::Korean),
        ("EN-us", Language::English),
        ("es-MX", Language::Spanish),
        ("pt-BR", Language::PortugueseBrazil),
        ("pt_br", Language::PortugueseBrazil),
        ("id-ID", Language::Indonesian),
        ("ja-JP", Language::Japanese),
        ("hi-IN", Language::Hindi),
        ("de-DE", Language::German),
        ("fr-CA", Language::French),
        ("vi-VN", Language::Vietnamese),
        ("tr-TR", Language::Turkish),
        ("ar-SA", Language::Arabic),
    ];
    for (locale, expected) in locale_cases {
        assert_eq!(
            resolve_windows_language(LanguagePreference::Auto, None, Some(locale)),
            expected,
            "locale {locale}"
        );
    }

    assert_eq!(
        resolve_windows_language(LanguagePreference::Auto, Some(0x0816), Some("pt-PT")),
        Language::English
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Auto, None, Some("pt")),
        Language::English
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Korean, Some(0x0409), Some("en-US")),
        Language::Korean
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::English, Some(0x0412), Some("ko-KR")),
        Language::English
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Arabic, Some(0x0409), Some("en-US")),
        Language::Arabic
    );
    assert_eq!(
        resolve_windows_language(LanguagePreference::Auto, None, None),
        Language::English
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
fn periodic_recovery_keeps_a_valid_taskbar_attachment_stable() {
    let mut lifecycle = NativeLifecycle::default();
    lifecycle.owner_created();
    lifecycle.timer_started();
    lifecycle.widget_created();
    lifecycle.widget_attached_to_taskbar();

    assert_eq!(
        lifecycle.recovery_decision(RecoveryEvent::Timer, true),
        RecoveryDecision::Keep
    );
}

#[derive(Default)]
struct RecordingWidgetSurfaceBackend {
    surfaces: Vec<(u32, WidgetSurface<u32>)>,
    next_window: u32,
    failed_targets: Vec<u32>,
    operations: Vec<&'static str>,
}

impl WidgetSurfaceBackend for RecordingWidgetSurfaceBackend {
    type Error = &'static str;
    type Target = u32;
    type Window = u32;

    fn surfaces(&self) -> Vec<(Self::Window, WidgetSurface<Self::Target>)> {
        self.surfaces.clone()
    }

    fn create_detached(&mut self) -> Result<Self::Window, Self::Error> {
        self.operations.push("create_detached");
        self.next_window += 1;
        self.surfaces
            .push((self.next_window, WidgetSurface::Detached));
        Ok(self.next_window)
    }

    fn attach(&mut self, window: Self::Window, target: Self::Target) -> Result<(), Self::Error> {
        self.operations.push("attach");
        if self.failed_targets.contains(&target) {
            return Err("attach failed");
        }
        self.surfaces
            .iter_mut()
            .find(|surface| surface.0 == window)
            .unwrap()
            .1 = WidgetSurface::Attached(target);
        Ok(())
    }

    fn detach(&mut self, window: Self::Window) -> Result<(), Self::Error> {
        self.operations.push("detach");
        self.surfaces
            .iter_mut()
            .find(|surface| surface.0 == window)
            .unwrap()
            .1 = WidgetSurface::Detached;
        Ok(())
    }

    fn destroy(&mut self, window: Self::Window) -> Result<(), Self::Error> {
        self.operations.push("destroy");
        self.surfaces.retain(|surface| surface.0 != window);
        Ok(())
    }
}

#[test]
fn attachment_failure_keeps_one_detached_widget_and_later_reuses_it() {
    let mut backend = RecordingWidgetSurfaceBackend {
        failed_targets: vec![7],
        ..Default::default()
    };

    assert_eq!(
        reconcile_widget_surfaces(&mut backend, &[7]),
        Ok(vec!["attach failed"])
    );
    assert_eq!(backend.surfaces, [(1, WidgetSurface::Detached)]);
    assert_eq!(backend.operations, ["create_detached", "attach"]);

    let view = WidgetViewModel {
        usage_profile_label: "Work".to_string(),
        primary: None,
        secondary: None,
        status: "Polling".to_string(),
        last_success: String::new(),
        is_stale: false,
        taskbar_label: "Weekly usage".to_string(),
        taskbar_tooltip: String::new(),
        reset_credits_text: None,
        data_state: WidgetDataState::Loading,
    };
    let surface = widget_surface_layout(208, 72, 96, false);
    assert_eq!(profile_header_text(&view, surface), Some("Work"));

    backend.failed_targets.clear();
    backend.operations.clear();
    assert_eq!(reconcile_widget_surfaces(&mut backend, &[7]), Ok(vec![]));
    assert_eq!(backend.surfaces, [(1, WidgetSurface::Attached(7))]);
    assert_eq!(backend.operations, ["attach"]);
}

#[test]
fn no_taskbar_target_preserves_exactly_one_detached_widget() {
    let mut backend = RecordingWidgetSurfaceBackend::default();
    assert_eq!(reconcile_widget_surfaces(&mut backend, &[]), Ok(vec![]));
    assert_eq!(backend.surfaces, [(1, WidgetSurface::Detached)]);

    reconcile_widget_surfaces(&mut backend, &[9]).unwrap();
    backend.operations.clear();
    assert_eq!(reconcile_widget_surfaces(&mut backend, &[]), Ok(vec![]));
    assert_eq!(backend.surfaces, [(1, WidgetSurface::Detached)]);
    assert_eq!(backend.operations, ["detach"]);
}

#[test]
fn partial_multi_monitor_failure_keeps_one_fallback_and_reuses_every_live_window() {
    let mut backend = RecordingWidgetSurfaceBackend {
        failed_targets: vec![2, 3],
        ..Default::default()
    };

    assert_eq!(
        reconcile_widget_surfaces(&mut backend, &[1, 2, 3]),
        Ok(vec!["attach failed", "attach failed"])
    );
    assert_eq!(
        backend.surfaces,
        [
            (1, WidgetSurface::Attached(1)),
            (2, WidgetSurface::Detached),
        ]
    );
    assert_eq!(
        backend.operations,
        [
            "create_detached",
            "attach",
            "create_detached",
            "attach",
            "create_detached",
            "attach",
            "destroy",
        ]
    );

    let view = WidgetViewModel {
        usage_profile_label: "Work".to_string(),
        primary: None,
        secondary: None,
        status: "Polling".to_string(),
        last_success: String::new(),
        is_stale: false,
        taskbar_label: "Weekly usage".to_string(),
        taskbar_tooltip: String::new(),
        reset_credits_text: None,
        data_state: WidgetDataState::Loading,
    };
    let detached = widget_surface_layout(208, 72, 96, false);
    assert_eq!(profile_header_text(&view, detached), Some("Work"));

    backend.failed_targets.clear();
    backend.operations.clear();
    assert_eq!(
        reconcile_widget_surfaces(&mut backend, &[1, 2, 3]),
        Ok(vec![])
    );
    assert_eq!(
        backend.surfaces,
        [
            (1, WidgetSurface::Attached(1)),
            (2, WidgetSurface::Attached(2)),
            (4, WidgetSurface::Attached(3)),
        ]
    );
    assert_eq!(backend.operations, ["attach", "create_detached", "attach"]);
}

#[test]
fn detached_widget_is_destroyed_before_its_owner_during_shutdown() {
    let mut lifecycle = NativeLifecycle::default();
    lifecycle.owner_created();
    lifecycle.timer_started();
    lifecycle.tray_created();
    lifecycle.widget_created();

    assert_eq!(
        lifecycle.cleanup_actions(),
        [
            CleanupAction::StopTimer,
            CleanupAction::RemoveTray,
            CleanupAction::DestroyWidget,
            CleanupAction::DestroyOwner,
        ]
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
fn chatgpt_login_url_validation_allows_only_known_https_hosts() {
    assert!(is_valid_chatgpt_login_url(
        "https://chatgpt.com/auth/login?redirect_uri=http%3A%2F%2Flocalhost%3A1234"
    ));
    assert!(is_valid_chatgpt_login_url(
        "https://auth.openai.com/authorize?state=opaque"
    ));
    for unsafe_url in [
        "http://chatgpt.com/auth/login",
        "https://chatgpt.com.evil.example/auth/login",
        "https://chatgpt.com@evil.example/auth/login",
        "https://chatgpt.com:443/auth/login",
        "https://auth.openai.com\nhttps://evil.example",
    ] {
        assert!(!is_valid_chatgpt_login_url(unsafe_url), "{unsafe_url}");
    }
}

#[test]
fn taskbar_placement_handles_offsets_secondary_and_rejections() {
    let primary = TaskbarGeometry {
        taskbar: Rect::new(0, 1040, 1920, 1080),
        notification: Rect::new(1700, 1040, 1920, 1080),
        occupied: None,
    };
    assert_eq!(
        place_taskbar_widget(primary, (380, 40), 88, 0),
        Ok(Rect::new(1320, 1040, 1700, 1080))
    );
    assert_eq!(
        place_taskbar_widget(primary, (380, 40), 88, -1),
        Err(TaskbarPlacementError::InsufficientSpace)
    );
    let secondary = TaskbarGeometry {
        taskbar: Rect::new(-1280, 984, 0, 1024),
        notification: Rect::new(-180, 984, 0, 1024),
        occupied: None,
    };
    assert_eq!(
        place_taskbar_widget(secondary, (380, 40), 88, 12),
        Ok(Rect::new(-572, 984, -192, 1024))
    );
    let vertical = TaskbarGeometry {
        taskbar: Rect::new(0, 0, 48, 1080),
        notification: Rect::new(0, 900, 48, 1080),
        occupied: None,
    };
    assert_eq!(
        place_taskbar_widget(vertical, (380, 48), 88, 0),
        Err(TaskbarPlacementError::VerticalTaskbar)
    );
    let narrow = TaskbarGeometry {
        taskbar: Rect::new(0, 0, 500, 40),
        notification: Rect::new(300, 0, 500, 40),
        occupied: None,
    };
    assert_eq!(
        place_taskbar_widget(narrow, (380, 40), 88, 0),
        Ok(Rect::new(0, 0, 300, 40))
    );
}

#[test]
fn taskbar_placement_shrinks_to_the_gap_after_the_last_task_button() {
    let geometry = TaskbarGeometry {
        taskbar: Rect::new(1920, 1235, 3000, 1283),
        notification: Rect::new(2820, 1235, 3000, 1283),
        occupied: Some(Rect::new(1920, 1235, 2727, 1283)),
    };
    assert_eq!(
        place_taskbar_widget(geometry, (208, 48), 88, 0),
        Ok(Rect::new(2727, 1235, 2820, 1283))
    );

    let crowded = TaskbarGeometry {
        occupied: Some(Rect::new(1920, 1235, 2733, 1283)),
        ..geometry
    };
    assert_eq!(
        place_taskbar_widget(crowded, (208, 48), 88, 0),
        Err(TaskbarPlacementError::InsufficientSpace)
    );
}

#[test]
fn taskbar_attachment_adapts_to_compact_taskbar_height() {
    assert_eq!(
        taskbar_widget_size(35, 96),
        Err(TaskbarPlacementError::InsufficientSpace)
    );
    assert_eq!(taskbar_widget_size(40, 96), Ok((208, 40)));
    assert_eq!(taskbar_widget_size(48, 96), Ok((208, 48)));
    assert_eq!(taskbar_widget_size(48, 120), Ok((260, 48)));
    assert_eq!(taskbar_widget_size(60, 120), Ok((260, 60)));
}

#[test]
fn taskbar_weekly_row_prefers_secondary_and_falls_back_to_primary() {
    let primary = codex_usage_monitor::windows::UsageRowView {
        label: "5시간".to_owned(),
        used_percent: 20.0,
        display_percent: 20.0,
        percent_text: "20%".to_owned(),
        reset_text: "2시간".to_owned(),
        level: codex_usage_monitor::UsageLevel::Stable,
    };
    let secondary = codex_usage_monitor::windows::UsageRowView {
        label: "7일".to_owned(),
        used_percent: 80.0,
        display_percent: 80.0,
        percent_text: "80%".to_owned(),
        reset_text: "3일".to_owned(),
        level: codex_usage_monitor::UsageLevel::Caution,
    };

    assert_eq!(
        select_weekly_row(Some(&primary), Some(&secondary)),
        Some(&secondary)
    );
    assert_eq!(select_weekly_row(Some(&primary), None), Some(&primary));
}

#[test]
fn taskbar_risk_uses_the_compact_widget_thresholds() {
    assert_eq!(TaskbarRisk::from_percent(0.0), TaskbarRisk::Healthy);
    assert_eq!(TaskbarRisk::from_percent(69.0), TaskbarRisk::Healthy);
    assert_eq!(TaskbarRisk::from_percent(70.0), TaskbarRisk::Warning);
    assert_eq!(TaskbarRisk::from_percent(89.0), TaskbarRisk::Warning);
    assert_eq!(TaskbarRisk::from_percent(90.0), TaskbarRisk::Critical);
    assert_eq!(TaskbarRisk::from_percent(125.0), TaskbarRisk::Critical);
}

#[test]
fn compact_taskbar_layout_fits_supported_dpis() {
    for dpi in [96, 120, 144, 192] {
        for (logical_width, expected_mode) in [
            (88, TaskbarLayoutMode::Minimal),
            (110, TaskbarLayoutMode::Compact),
            (208, TaskbarLayoutMode::Full),
        ] {
            let width = logical_to_physical(logical_width, dpi);
            let height = logical_to_physical(48, dpi);
            let layout = TaskbarLayout::for_size(width, height, dpi);

            assert_eq!(layout.mode, expected_mode);
            if let Some(dot) = layout.dot {
                assert!(dot.is_inside(layout.window));
            }
            if let Some(label) = layout.label {
                assert!(label.is_inside(layout.window));
                assert!(!label.intersects(layout.percent));
            }
            assert!(layout.percent.is_inside(layout.window));
            assert!(layout.progress.is_inside(layout.window));
        }
    }
}

#[test]
fn taskbar_layout_hides_details_as_space_decreases() {
    let full = TaskbarLayout::for_size(140, 48, 96);
    assert!(full.dot.is_some());
    assert!(full.label.is_some());

    let compact = TaskbarLayout::for_size(100, 48, 96);
    assert!(compact.dot.is_some());
    assert!(compact.label.is_none());

    let minimal = TaskbarLayout::for_size(99, 48, 96);
    assert!(minimal.dot.is_none());
    assert!(minimal.label.is_none());
}

#[test]
fn hover_transition_reverses_from_the_current_value_without_jumping() {
    let mut hover = HoverTransition::default();
    hover.set_hovered(true);
    for _ in 0..4 {
        assert!(hover.tick());
    }
    let reached = hover.value();
    assert!(reached > 0 && reached < 255);

    hover.set_hovered(false);
    assert_eq!(hover.value(), reached);
    assert!(hover.tick());
    assert!(hover.value() < reached);
    while hover.tick() {}
    assert_eq!(hover.value(), 0);
}

const ORIGINAL_STYLE: u32 = 0x8001_0000;
const CHILD_STYLE: u32 = 0x4401_0000;
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
    assert_eq!(backend.style, CHILD_STYLE);
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
    let path = PathBuf::from(r"C:\Program Files\Codex Usage Monitor\codex-peek.exe");
    let expected = r#""C:\Program Files\Codex Usage Monitor\codex-peek.exe" --startup"#;
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
