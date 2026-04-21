use super::{
    Assertion, BranchMatchCondition, Condition, ConditionContainer, ConfigError,
    DiffMatchCondition, DiffMode, SeverityBands, ThresholdCondition, ThresholdMetric,
    ThresholdOperator, parse, validate_assertion_patterns, validate_assertion_severities,
    validate_assertions, validate_severity_bands,
};

fn minimal_toml_with_assertion() -> String {
    "api_version = \"pre\"\n\n[[assertions]]\nalias = \"a1\"\nseverity = 10\n[assertions.must_satisfy]\n[assertions.must_satisfy.condition]\ntype = \"msg_match_any\"\nmode = \"raw\"\npatterns = [\"^feat\"]\n"
        .to_owned()
}

fn minimal_assertion_with_pattern(alias: &str, severity: u8, pattern: &str) -> Assertion {
    Assertion {
        alias: alias.to_owned(),
        skip: false,
        description: String::new(),
        banner: String::new(),
        hint: String::new(),
        severity,
        must_satisfy: ConditionContainer {
            condition: Condition::DiffMatchAny(DiffMatchCondition {
                name: String::new(),
                mode: DiffMode::File,
                patterns: vec![pattern.to_owned()],
            }),
        },
        skip_if: None,
        custom_meta: std::collections::HashMap::new(),
    }
}

fn minimal_assertion(alias: &str, severity: u8) -> Assertion {
    minimal_assertion_with_pattern(alias, severity, "^src/")
}

#[test]
fn severity_bands_accepts_values_up_to_250() {
    let bands = SeverityBands {
        fatal: 250,
        error: 10,
        warning: 1,
        information: 0,
    };

    let result = validate_severity_bands(&bands);
    assert!(result.is_ok());
}

#[test]
fn severity_bands_rejects_values_above_250() {
    let bands = SeverityBands {
        fatal: 251,
        error: 10,
        warning: 1,
        information: 0,
    };

    let result = validate_severity_bands(&bands);
    assert!(result.is_err());
}

#[test]
fn assertion_severity_accepts_values_up_to_250() {
    let assertions = vec![minimal_assertion("ok", 250)];

    let result = validate_assertion_severities(&assertions);
    assert!(result.is_ok());
}

#[test]
fn assertion_severity_rejects_values_above_250() {
    let assertions = vec![minimal_assertion("too-high", 251)];

    let result = validate_assertion_severities(&assertions);
    assert!(result.is_err());
}

#[test]
fn assertion_patterns_accept_valid_regex() {
    let assertions = vec![minimal_assertion_with_pattern(
        "valid-regex",
        10,
        "^src/.*$",
    )];

    let result = validate_assertion_patterns(&assertions);
    assert!(result.is_ok());
}

#[test]
fn assertion_patterns_reject_invalid_regex() {
    let assertions = vec![minimal_assertion_with_pattern("invalid-regex", 10, "(")];

    let result = validate_assertion_patterns(&assertions);
    assert!(result.is_err());
}

#[test]
fn validate_assertions_rejects_duplicate_aliases() {
    let assertions = vec![minimal_assertion("dup", 10), minimal_assertion("dup", 20)];

    let result = validate_assertions(&assertions);
    assert!(result.is_err());

    let message = match result {
        Err(ConfigError::Semantic(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };
    assert!(message.contains("duplicate assertion alias"));
}

#[test]
fn validate_assertion_patterns_rejects_invalid_skip_if_regex() {
    let mut assertion = minimal_assertion("skip-if-invalid", 10);
    assertion.skip_if = Some(ConditionContainer {
        condition: Condition::BranchMatch(BranchMatchCondition {
            name: String::new(),
            patterns: vec!["(".to_owned()],
        }),
    });

    let result = validate_assertion_patterns(&[assertion]);
    assert!(result.is_err());

    let message = match result {
        Err(ConfigError::Semantic(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };
    assert!(message.contains("skip_if.patterns[0]"));
}

#[test]
fn validate_assertion_patterns_ignores_threshold_conditions() {
    let assertion = Assertion {
        alias: "threshold-only".to_owned(),
        skip: false,
        description: String::new(),
        banner: String::new(),
        hint: String::new(),
        severity: 10,
        must_satisfy: ConditionContainer {
            condition: Condition::ThresholdCompare(ThresholdCondition {
                name: String::new(),
                metric: ThresholdMetric::LineCount,
                operator: ThresholdOperator::Gte,
                value: 1,
            }),
        },
        skip_if: None,
        custom_meta: std::collections::HashMap::new(),
    };

    let result = validate_assertion_patterns(&[assertion]);
    assert!(result.is_ok());
}

#[test]
fn severity_bands_reject_fatal_not_greater_than_error() {
    let bands = SeverityBands {
        fatal: 10,
        error: 10,
        warning: 1,
        information: 0,
    };

    let result = validate_severity_bands(&bands);
    assert!(result.is_err());
}

#[test]
fn severity_bands_reject_error_not_greater_than_warning() {
    let bands = SeverityBands {
        fatal: 250,
        error: 1,
        warning: 1,
        information: 0,
    };

    let result = validate_severity_bands(&bands);
    assert!(result.is_err());
}

#[test]
fn severity_bands_reject_warning_below_information() {
    let bands = SeverityBands {
        fatal: 250,
        error: 10,
        warning: 0,
        information: 1,
    };

    let result = validate_severity_bands(&bands);
    assert!(result.is_err());
}

#[test]
fn parse_defaults_to_toml_when_source_path_is_none() {
    let result = parse(&minimal_toml_with_assertion(), None);
    assert!(result.is_ok());
}

#[test]
fn parse_supports_json_extension() {
    let content = r#"{
  "api_version": "pre",
  "assertions": [
    {
      "alias": "a1",
      "severity": 10,
      "must_satisfy": {
        "condition": {
          "type": "msg_match_any",
          "mode": "raw",
          "patterns": ["^feat"]
        }
      }
    }
  ]
}"#;

    let result = parse(content, Some(std::path::Path::new("config.json")));
    assert!(result.is_ok());
}

#[test]
fn parse_supports_json5_extension() {
    let content = r"{
      api_version: 'pre',
      assertions: [{
        alias: 'a1',
        severity: 10,
        must_satisfy: {
          condition: {
            type: 'msg_match_any',
            mode: 'raw',
            patterns: ['^feat'],
          },
        },
            }],
        }";

    let result = parse(content, Some(std::path::Path::new("config.json5")));
    assert!(result.is_ok());
}

#[test]
fn parse_supports_yaml_extension() {
    let content = "api_version: pre\nassertions:\n  - alias: a1\n    severity: 10\n    must_satisfy:\n      condition:\n        type: msg_match_any\n        mode: raw\n        patterns:\n          - '^feat'\n";

    let result = parse(content, Some(std::path::Path::new("config.yaml")));
    assert!(result.is_ok());
}

#[test]
fn parse_rejects_invalid_json_for_json_extension() {
    let result = parse("not valid json", Some(std::path::Path::new("config.json")));
    assert!(result.is_err());
}

#[test]
fn parse_rejects_invalid_yaml_for_yaml_extension() {
    let result = parse("api_version: [", Some(std::path::Path::new("config.yaml")));
    assert!(result.is_err());
}
