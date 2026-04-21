use super::{normalize_cli_preset_name, select_assertions_from_presets, validate_cli_preset_names};
use crate::config;

#[test]
fn clone_condition_clones_message_conditions() {
    let any = config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: "msg-any".to_owned(),
        mode: config::MsgMode::Title,
        patterns: vec!["^feat".to_owned()],
    });
    let none = config::Condition::MsgMatchNone(config::MsgMatchCondition {
        name: "msg-none".to_owned(),
        mode: config::MsgMode::Body,
        patterns: vec!["WIP".to_owned()],
    });

    let cloned_any = super::clone_condition(&any);
    let cloned_none = super::clone_condition(&none);

    let msg_any = match cloned_any {
        config::Condition::MsgMatchAny(value) => Some(value),
        _ => None,
    };
    assert!(msg_any.is_some());
    let msg_any = msg_any.unwrap_or(config::MsgMatchCondition {
        name: String::new(),
        mode: config::MsgMode::Raw,
        patterns: Vec::new(),
    });
    assert_eq!(msg_any.name, "msg-any");
    assert_eq!(msg_any.patterns, vec!["^feat".to_owned()]);
    assert!(matches!(msg_any.mode, config::MsgMode::Title));

    let msg_none = match cloned_none {
        config::Condition::MsgMatchNone(value) => Some(value),
        _ => None,
    };
    assert!(msg_none.is_some());
    let msg_none = msg_none.unwrap_or(config::MsgMatchCondition {
        name: String::new(),
        mode: config::MsgMode::Raw,
        patterns: Vec::new(),
    });
    assert_eq!(msg_none.name, "msg-none");
    assert_eq!(msg_none.patterns, vec!["WIP".to_owned()]);
    assert!(matches!(msg_none.mode, config::MsgMode::Body));
}

#[test]
fn clone_condition_clones_diff_conditions() {
    let any = config::Condition::DiffMatchAny(config::DiffMatchCondition {
        name: "diff-any".to_owned(),
        mode: config::DiffMode::File,
        patterns: vec!["src/main\\.rs".to_owned()],
    });
    let none = config::Condition::DiffMatchNone(config::DiffMatchCondition {
        name: "diff-none".to_owned(),
        mode: config::DiffMode::Line,
        patterns: vec!["password".to_owned()],
    });

    let cloned_any = super::clone_condition(&any);
    let cloned_none = super::clone_condition(&none);

    let diff_any = match cloned_any {
        config::Condition::DiffMatchAny(value) => Some(value),
        _ => None,
    };
    assert!(diff_any.is_some());
    let diff_any = diff_any.unwrap_or(config::DiffMatchCondition {
        name: String::new(),
        mode: config::DiffMode::Raw,
        patterns: Vec::new(),
    });
    assert_eq!(diff_any.name, "diff-any");
    assert_eq!(diff_any.patterns, vec!["src/main\\.rs".to_owned()]);
    assert!(matches!(diff_any.mode, config::DiffMode::File));

    let diff_none = match cloned_none {
        config::Condition::DiffMatchNone(value) => Some(value),
        _ => None,
    };
    assert!(diff_none.is_some());
    let diff_none = diff_none.unwrap_or(config::DiffMatchCondition {
        name: String::new(),
        mode: config::DiffMode::Raw,
        patterns: Vec::new(),
    });
    assert_eq!(diff_none.name, "diff-none");
    assert_eq!(diff_none.patterns, vec!["password".to_owned()]);
    assert!(matches!(diff_none.mode, config::DiffMode::Line));
}

#[test]
fn clone_condition_clones_branch_and_threshold_conditions() {
    let branch = config::Condition::BranchMatch(config::BranchMatchCondition {
        name: "branch".to_owned(),
        patterns: vec!["main".to_owned(), "release/.*".to_owned()],
    });
    let threshold = config::Condition::ThresholdCompare(config::ThresholdCondition {
        name: "threshold".to_owned(),
        metric: config::ThresholdMetric::FileCount,
        operator: config::ThresholdOperator::Gte,
        value: 3,
    });

    let cloned_branch = super::clone_condition(&branch);
    let cloned_threshold = super::clone_condition(&threshold);

    let branch_value = match cloned_branch {
        config::Condition::BranchMatch(value) => Some(value),
        _ => None,
    };
    assert!(branch_value.is_some());
    let branch_value = branch_value.unwrap_or(config::BranchMatchCondition {
        name: String::new(),
        patterns: Vec::new(),
    });
    assert_eq!(branch_value.name, "branch");
    assert_eq!(branch_value.patterns.len(), 2);

    let threshold_value = match cloned_threshold {
        config::Condition::ThresholdCompare(value) => Some(value),
        _ => None,
    };
    assert!(threshold_value.is_some());
    let threshold_value = threshold_value.unwrap_or(config::ThresholdCondition {
        name: String::new(),
        metric: config::ThresholdMetric::LineCount,
        operator: config::ThresholdOperator::Lte,
        value: 0,
    });
    assert_eq!(threshold_value.name, "threshold");
    assert_eq!(threshold_value.value, 3);
    assert!(matches!(
        threshold_value.metric,
        config::ThresholdMetric::FileCount
    ));
    assert!(matches!(
        threshold_value.operator,
        config::ThresholdOperator::Gte
    ));
}

#[test]
fn clone_condition_preserves_raw_modes_and_linecount_lte_thresholds() {
    let msg_any = config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: "msg-raw".to_owned(),
        mode: config::MsgMode::Raw,
        patterns: vec!["raw".to_owned()],
    });
    let diff_any = config::Condition::DiffMatchAny(config::DiffMatchCondition {
        name: "diff-raw".to_owned(),
        mode: config::DiffMode::Raw,
        patterns: vec!["diff".to_owned()],
    });
    let threshold = config::Condition::ThresholdCompare(config::ThresholdCondition {
        name: "threshold-lte".to_owned(),
        metric: config::ThresholdMetric::LineCount,
        operator: config::ThresholdOperator::Lte,
        value: 7,
    });

    let cloned_msg = super::clone_condition(&msg_any);
    let cloned_diff = super::clone_condition(&diff_any);
    let cloned_threshold = super::clone_condition(&threshold);

    let msg_value = match cloned_msg {
        config::Condition::MsgMatchAny(value) => Some(value),
        _ => None,
    };
    assert!(msg_value.is_some());
    if let Some(value) = msg_value {
        assert!(matches!(value.mode, config::MsgMode::Raw));
        assert_eq!(value.patterns, vec!["raw".to_owned()]);
    }

    let diff_value = match cloned_diff {
        config::Condition::DiffMatchAny(value) => Some(value),
        _ => None,
    };
    assert!(diff_value.is_some());
    if let Some(value) = diff_value {
        assert!(matches!(value.mode, config::DiffMode::Raw));
        assert_eq!(value.patterns, vec!["diff".to_owned()]);
    }

    let threshold_value = match cloned_threshold {
        config::Condition::ThresholdCompare(value) => Some(value),
        _ => None,
    };
    assert!(threshold_value.is_some());
    if let Some(value) = threshold_value {
        assert!(matches!(value.metric, config::ThresholdMetric::LineCount));
        assert!(matches!(value.operator, config::ThresholdOperator::Lte));
        assert_eq!(value.value, 7);
    }
}

#[test]
fn load_presets_returns_expected_embedded_registry_keys() {
    let presets = super::load_presets();
    assert!(presets.is_ok());

    let presets = presets.unwrap_or_default();
    assert_eq!(presets.len(), 4);
    assert!(presets.contains_key("conventional_commits"));
    assert!(presets.contains_key("title_body_separator"));
    assert!(presets.contains_key("forbid_wip"));
    assert!(presets.contains_key("security_related_edits_mention"));
}

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
