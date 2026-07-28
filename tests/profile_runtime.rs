use codex_usage_monitor::{
    normalize_profile_label, ProfileValidationError, UsageProfileCatalog, UsageProfileId,
    UsageProfileRoot, MAX_USAGE_PROFILES,
};

#[test]
fn profile_labels_are_trimmed_bounded_and_case_insensitively_unique() {
    assert_eq!(normalize_profile_label("  개인  ").unwrap(), "개인");
    assert_eq!(
        normalize_profile_label(".."),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label("bad/name"),
        Err(ProfileValidationError::InvalidLabel)
    );

    let mut catalog = UsageProfileCatalog::default();
    assert_eq!(
        catalog.add("Work").unwrap().id(),
        UsageProfileId::Managed(1)
    );
    assert_eq!(
        catalog.add(" work "),
        Err(ProfileValidationError::DuplicateLabel)
    );
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

#[test]
fn catalog_rejects_an_eighth_managed_profile() {
    let mut catalog = UsageProfileCatalog::default();

    for sequence in 1..MAX_USAGE_PROFILES {
        catalog.add(&format!("Profile {sequence}")).unwrap();
    }

    assert_eq!(
        catalog.add("Overflow"),
        Err(ProfileValidationError::TooManyProfiles)
    );
}

#[test]
fn catalog_maintains_selection_and_rejects_system_mutations() {
    let mut catalog = UsageProfileCatalog::default();
    let profile_id = catalog.add("Work").unwrap().id();

    catalog.select(profile_id).unwrap();
    catalog.rename(profile_id, "Office").unwrap();
    assert_eq!(catalog.managed()[0].label(), "Office");

    catalog.remove(profile_id).unwrap();
    assert_eq!(catalog.selected(), UsageProfileId::System);
    assert_eq!(
        catalog.rename(UsageProfileId::System, "Changed"),
        Err(ProfileValidationError::SystemProfileImmutable)
    );
    assert_eq!(
        catalog.remove(UsageProfileId::System),
        Err(ProfileValidationError::SystemProfileImmutable)
    );
}

#[test]
fn labels_reject_invalid_unicode_scalar_counts_and_control_characters() {
    assert_eq!(
        normalize_profile_label("\nwork"),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label(" "),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label(&"a".repeat(41)),
        Err(ProfileValidationError::InvalidLabel)
    );
}

#[test]
fn catalog_validation_rejects_noncanonical_labels_and_wrapped_sequences() {
    let noncanonical: UsageProfileCatalog = serde_json::from_str(
        r#"{"managed":[{"sequence":1,"label":" Work "}],"selected":{"managed":1},"next_sequence":2}"#,
    )
    .unwrap();
    let wrapped_sequence: UsageProfileCatalog =
        serde_json::from_str(r#"{"managed":[],"selected":"system","next_sequence":0}"#).unwrap();

    assert_eq!(
        noncanonical.validate(),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        wrapped_sequence.validate(),
        Err(ProfileValidationError::InvalidId)
    );
}
