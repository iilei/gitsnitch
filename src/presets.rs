use std::collections::BTreeMap;

use serde::Deserialize;

use crate::config;

const PRESET_FILES: &[(&str, &str)] = &[
    (
        "conventional_commits",
        include_str!("presets_data/conventional_commits.toml"),
    ),
    (
        "title_body_seperator",
        include_str!("presets_data/title_body_seperator.toml"),
    ),
    ("forbid_wip", include_str!("presets_data/forbid_wip.toml")),
    (
        "security_related_edits_mention",
        include_str!("presets_data/security_related_edits_mention.toml"),
    ),
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetConfig {
    #[serde(default)]
    assertions: Vec<config::Assertion>,
}

fn load_presets() -> Result<BTreeMap<String, Vec<config::Assertion>>, config::ConfigError> {
    let mut presets = BTreeMap::new();

    for (name, raw_content) in PRESET_FILES {
        let parsed: PresetConfig =
            toml::from_str(raw_content).map_err(config::ConfigError::Toml)?;
        config::validate_assertions(&parsed.assertions)?;

        if presets
            .insert((*name).to_owned(), parsed.assertions)
            .is_some()
        {
            return Err(config::ConfigError::Semantic(format!(
                "duplicate embedded preset name: '{name}'"
            )));
        }
    }

    Ok(presets)
}

fn normalize_cli_preset_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('-', "_")
}

pub fn validate_cli_preset_names(names: &[String]) -> Result<(), config::ConfigError> {
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(config::ConfigError::Semantic(
                "preset name cannot be empty".to_owned(),
            ));
        }

        if !trimmed.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        }) {
            return Err(config::ConfigError::Semantic(format!(
                "invalid preset name '{trimmed}': use lowercase letters, digits, and dashes"
            )));
        }
    }

    Ok(())
}

pub fn select_assertions_from_presets(
    selected_names: &[String],
) -> Result<Vec<config::Assertion>, config::ConfigError> {
    let registry = load_presets()?;
    let mut merged = Vec::new();

    for name in selected_names {
        let normalized = normalize_cli_preset_name(name);
        let Some(assertions) = registry.get(&normalized) else {
            return Err(config::ConfigError::Semantic(format!(
                "unknown preset: '{name}'"
            )));
        };

        for assertion in assertions {
            merged.push(config::Assertion {
                alias: assertion.alias.clone(),
                skip: assertion.skip,
                description: assertion.description.clone(),
                banner: assertion.banner.clone(),
                hint: assertion.hint.clone(),
                severity: assertion.severity,
                must_satisfy: config::ConditionContainer {
                    condition: clone_condition(&assertion.must_satisfy.condition),
                },
                skip_if: assertion
                    .skip_if
                    .as_ref()
                    .map(|skip_if| config::ConditionContainer {
                        condition: clone_condition(&skip_if.condition),
                    }),
                custom_meta: assertion.custom_meta.clone(),
            });
        }
    }

    Ok(merged)
}

fn clone_condition(condition: &config::Condition) -> config::Condition {
    match condition {
        config::Condition::MsgMatchAny(value) => {
            config::Condition::MsgMatchAny(config::MsgMatchCondition {
                name: value.name.clone(),
                mode: match value.mode {
                    config::MsgMode::Raw => config::MsgMode::Raw,
                    config::MsgMode::Title => config::MsgMode::Title,
                    config::MsgMode::Body => config::MsgMode::Body,
                },
                patterns: value.patterns.clone(),
            })
        }
        config::Condition::MsgMatchNone(value) => {
            config::Condition::MsgMatchNone(config::MsgMatchCondition {
                name: value.name.clone(),
                mode: match value.mode {
                    config::MsgMode::Raw => config::MsgMode::Raw,
                    config::MsgMode::Title => config::MsgMode::Title,
                    config::MsgMode::Body => config::MsgMode::Body,
                },
                patterns: value.patterns.clone(),
            })
        }
        config::Condition::DiffMatchAny(value) => {
            config::Condition::DiffMatchAny(config::DiffMatchCondition {
                name: value.name.clone(),
                mode: match value.mode {
                    config::DiffMode::Raw => config::DiffMode::Raw,
                    config::DiffMode::File => config::DiffMode::File,
                    config::DiffMode::Line => config::DiffMode::Line,
                },
                patterns: value.patterns.clone(),
            })
        }
        config::Condition::DiffMatchNone(value) => {
            config::Condition::DiffMatchNone(config::DiffMatchCondition {
                name: value.name.clone(),
                mode: match value.mode {
                    config::DiffMode::Raw => config::DiffMode::Raw,
                    config::DiffMode::File => config::DiffMode::File,
                    config::DiffMode::Line => config::DiffMode::Line,
                },
                patterns: value.patterns.clone(),
            })
        }
        config::Condition::BranchMatch(value) => {
            config::Condition::BranchMatch(config::BranchMatchCondition {
                name: value.name.clone(),
                patterns: value.patterns.clone(),
            })
        }
        config::Condition::ThresholdCompare(value) => {
            config::Condition::ThresholdCompare(config::ThresholdCondition {
                name: value.name.clone(),
                metric: match value.metric {
                    config::ThresholdMetric::LineCount => config::ThresholdMetric::LineCount,
                    config::ThresholdMetric::FileCount => config::ThresholdMetric::FileCount,
                },
                operator: match value.operator {
                    config::ThresholdOperator::Lte => config::ThresholdOperator::Lte,
                    config::ThresholdOperator::Gte => config::ThresholdOperator::Gte,
                },
                value: value.value,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_cli_preset_name, select_assertions_from_presets, validate_cli_preset_names,
    };

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
            "title-body-seperator".to_owned(),
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
}
