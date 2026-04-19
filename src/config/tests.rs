use super::{
    Assertion, Condition, ConditionContainer, DiffMatchCondition, DiffMode, SeverityBands,
    validate_assertion_patterns, validate_assertion_severities, validate_severity_bands,
};

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
