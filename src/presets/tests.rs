use super::{normalize_cli_preset_name, select_assertions_from_presets, validate_cli_preset_names};

#[test]
fn normalize_cli_preset_name_maps_dashes_to_underscores() {
    assert_eq!(
        normalize_cli_preset_name("conventional-commits"),
        "conventional_commits"
    );
}

#[test]
fn validate_cli_preset_names_rejects_empty() {
    let result = validate_cli_preset_names(&[" ".to_owned()]);
    assert!(result.is_err());
}

#[test]
fn validate_cli_preset_names_rejects_invalid_characters() {
    let result = validate_cli_preset_names(&["Conventional_Commits".to_owned()]);
    assert!(result.is_err());
}

#[test]
fn select_assertions_from_presets_returns_assertions_for_known_preset() {
    let result = select_assertions_from_presets(&["conventional-commits".to_owned()]);
    assert!(result.is_ok());

    let assertions = result.unwrap_or_default();
    assert_eq!(assertions.len(), 1);
    assert_eq!(
        assertions.first().map(|assertion| assertion.alias.as_str()),
        Some("preset_conventional_title")
    );
}

#[test]
fn select_assertions_from_presets_rejects_unknown_preset() {
    let result = select_assertions_from_presets(&["does-not-exist".to_owned()]);
    assert!(result.is_err());
}

#[test]
fn select_assertions_from_presets_resolves_all_embedded_presets() {
    let result = select_assertions_from_presets(&[
        "conventional-commits".to_owned(),
        "title-body-separator".to_owned(),
        "forbid-wip".to_owned(),
        "security-related-edits-mention".to_owned(),
    ]);
    assert!(result.is_ok());

    let assertions = result.unwrap_or_default();
    assert_eq!(assertions.len(), 4);
    assert_eq!(
        assertions.first().map(|assertion| assertion.alias.as_str()),
        Some("preset_conventional_title")
    );
}
