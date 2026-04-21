use super::incremental_deepen_step;
use crate::config;

fn sample_commit_context() -> super::CommitContext {
    super::CommitContext {
        raw_message: "feat(parser): add tokenizer\n\nline one\nline two".to_owned(),
        title: "feat(parser): add tokenizer".to_owned(),
        body: "line one\nline two".to_owned(),
        diff_raw: "diff --git a/src/main.rs b/src/main.rs\n+let value = 1;".to_owned(),
        diff_files_joined: "src/main.rs\nsrc/lib.rs".to_owned(),
        diff_lines_joined: "+let value = 1;\n-let old = 0;".to_owned(),
        line_count: 12,
        file_count: 2,
        branches_joined: "main\nfeature/decorative".to_owned(),
    }
}

fn sample_assertion(condition: config::Condition) -> config::Assertion {
    config::Assertion {
        alias: "a1".to_owned(),
        skip: false,
        description: "desc".to_owned(),
        banner: String::new(),
        hint: String::new(),
        severity: 10,
        must_satisfy: config::ConditionContainer { condition },
        skip_if: None,
        custom_meta: config::CustomMeta::new(),
    }
}

#[test]
fn evaluate_condition_supports_message_modes() {
    let commit = sample_commit_context();

    let raw = config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: "raw".to_owned(),
        mode: config::MsgMode::Raw,
        patterns: vec!["tokenizer".to_owned()],
    });
    let title = config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: "title".to_owned(),
        mode: config::MsgMode::Title,
        patterns: vec!["^feat".to_owned()],
    });
    let body = config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: "body".to_owned(),
        mode: config::MsgMode::Body,
        patterns: vec!["line two".to_owned()],
    });

    assert_eq!(super::evaluate_condition(&raw, &commit).ok(), Some(true));
    assert_eq!(super::evaluate_condition(&title, &commit).ok(), Some(true));
    assert_eq!(super::evaluate_condition(&body, &commit).ok(), Some(true));
}

#[test]
fn evaluate_condition_supports_message_none_mode() {
    let commit = sample_commit_context();
    let condition = config::Condition::MsgMatchNone(config::MsgMatchCondition {
        name: "body-none".to_owned(),
        mode: config::MsgMode::Body,
        patterns: vec!["DO NOT MERGE".to_owned()],
    });

    assert_eq!(
        super::evaluate_condition(&condition, &commit).ok(),
        Some(true)
    );
}

#[test]
fn evaluate_condition_supports_diff_modes() {
    let commit = sample_commit_context();
    let raw = config::Condition::DiffMatchAny(config::DiffMatchCondition {
        name: "diff-raw".to_owned(),
        mode: config::DiffMode::Raw,
        patterns: vec!["diff --git".to_owned()],
    });
    let file = config::Condition::DiffMatchAny(config::DiffMatchCondition {
        name: "diff-file".to_owned(),
        mode: config::DiffMode::File,
        patterns: vec!["src/lib\\.rs".to_owned()],
    });
    let line = config::Condition::DiffMatchAny(config::DiffMatchCondition {
        name: "diff-line".to_owned(),
        mode: config::DiffMode::Line,
        patterns: vec!["\\+let value".to_owned()],
    });

    assert_eq!(super::evaluate_condition(&raw, &commit).ok(), Some(true));
    assert_eq!(super::evaluate_condition(&file, &commit).ok(), Some(true));
    assert_eq!(super::evaluate_condition(&line, &commit).ok(), Some(true));
}

#[test]
fn evaluate_condition_supports_diff_none_mode() {
    let commit = sample_commit_context();
    let condition = config::Condition::DiffMatchNone(config::DiffMatchCondition {
        name: "diff-none".to_owned(),
        mode: config::DiffMode::Line,
        patterns: vec!["password".to_owned()],
    });

    assert_eq!(
        super::evaluate_condition(&condition, &commit).ok(),
        Some(true)
    );
}

#[test]
fn evaluate_condition_supports_branch_match() {
    let commit = sample_commit_context();
    let condition = config::Condition::BranchMatch(config::BranchMatchCondition {
        name: "branch".to_owned(),
        patterns: vec!["feature/decorative".to_owned()],
    });

    assert_eq!(
        super::evaluate_condition(&condition, &commit).ok(),
        Some(true)
    );
}

#[test]
fn evaluate_condition_supports_threshold_compare_for_both_metrics() {
    let commit = sample_commit_context();
    let line_threshold = config::Condition::ThresholdCompare(config::ThresholdCondition {
        name: "line-count".to_owned(),
        metric: config::ThresholdMetric::LineCount,
        operator: config::ThresholdOperator::Gte,
        value: 10,
    });
    let file_threshold = config::Condition::ThresholdCompare(config::ThresholdCondition {
        name: "file-count".to_owned(),
        metric: config::ThresholdMetric::FileCount,
        operator: config::ThresholdOperator::Lte,
        value: 2,
    });

    assert_eq!(
        super::evaluate_condition(&line_threshold, &commit).ok(),
        Some(true)
    );
    assert_eq!(
        super::evaluate_condition(&file_threshold, &commit).ok(),
        Some(true)
    );
}

#[test]
fn evaluate_condition_returns_error_for_invalid_regex() {
    let commit = sample_commit_context();
    let condition = config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: "invalid".to_owned(),
        mode: config::MsgMode::Title,
        patterns: vec!["(".to_owned()],
    });

    let result = super::evaluate_condition(&condition, &commit);
    assert!(result.is_err());

    let error_message = match result {
        Err(crate::AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(error_message.contains("invalid regex pattern"));
}

#[test]
fn body_from_raw_message_handles_title_only_and_title_plus_body() {
    let title_only = super::body_from_raw_message("feat: title only");
    let with_body = super::body_from_raw_message("feat: title\n\nbody line\nsecond");

    assert!(title_only.is_empty());
    assert_eq!(with_body, "\nbody line\nsecond");
}

#[test]
fn collect_diff_lines_ignores_headers_and_keeps_added_removed_lines() {
    let diff = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,2 +1,2 @@\n-old\n+new\n context";
    let lines = super::collect_diff_lines(diff);
    assert_eq!(lines, "-old\n+new");
}

#[test]
fn parse_numstat_totals_handles_regular_and_binary_rows() {
    let numstat = "1\t2\tsrc/main.rs\n-\t-\tassets/logo.png\n";
    let totals = super::parse_numstat_totals(numstat);
    assert_eq!(totals.ok(), Some((3, 2)));
}

#[test]
fn parse_numstat_totals_rejects_invalid_added_number() {
    let numstat = "abc\t2\tsrc/main.rs\n";
    let result = super::parse_numstat_totals(numstat);
    assert!(result.is_err());

    let message = match result {
        Err(crate::AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(message.contains("failed to parse numstat added value"));
}

#[test]
fn parse_numstat_totals_rejects_invalid_removed_number() {
    let numstat = "1\txyz\tsrc/main.rs\n";
    let result = super::parse_numstat_totals(numstat);
    assert!(result.is_err());

    let message = match result {
        Err(crate::AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(message.contains("failed to parse numstat removed value"));
}

#[test]
fn matches_any_regex_returns_false_when_no_patterns_match() {
    let patterns = vec!["foo".to_owned(), "bar".to_owned()];
    let result = super::matches_any_regex(&patterns, "baz");
    assert_eq!(result.ok(), Some(false));
}

#[test]
fn add_context_prefixes_message_errors() {
    let error = crate::AppError::Message("inner".to_owned());
    let contextual = super::add_context(error, "outer");

    let message = match contextual {
        crate::AppError::Message(message) => message,
        _ => String::new(),
    };
    assert_eq!(message, "outer: inner");
}

#[test]
fn assertion_violated_returns_false_when_assertion_is_skipped() {
    let commit = sample_commit_context();
    let mut assertion =
        sample_assertion(config::Condition::MsgMatchAny(config::MsgMatchCondition {
            name: String::new(),
            mode: config::MsgMode::Title,
            patterns: vec!["^feat".to_owned()],
        }));
    assertion.skip = true;

    let result = super::assertion_violated(&assertion, &commit);
    assert_eq!(result.ok(), Some(false));
}

#[test]
fn assertion_violated_respects_skip_if() {
    let commit = sample_commit_context();
    let mut assertion =
        sample_assertion(config::Condition::MsgMatchAny(config::MsgMatchCondition {
            name: String::new(),
            mode: config::MsgMode::Title,
            patterns: vec!["^fix".to_owned()],
        }));
    assertion.skip_if = Some(config::ConditionContainer {
        condition: config::Condition::BranchMatch(config::BranchMatchCondition {
            name: String::new(),
            patterns: vec!["feature/decorative".to_owned()],
        }),
    });

    let result = super::assertion_violated(&assertion, &commit);
    assert_eq!(result.ok(), Some(false));
}

#[test]
fn assertion_violated_returns_true_when_must_satisfy_fails() {
    let commit = sample_commit_context();
    let assertion = sample_assertion(config::Condition::MsgMatchAny(config::MsgMatchCondition {
        name: String::new(),
        mode: config::MsgMode::Title,
        patterns: vec!["^fix".to_owned()],
    }));

    let result = super::assertion_violated(&assertion, &commit);
    assert_eq!(result.ok(), Some(true));
}

#[test]
fn collect_violations_returns_empty_when_no_assertions_are_provided() {
    let scope = crate::LintScope::CommitSha("deadbeef".to_owned());
    let history = config::History::default();
    let result = super::collect_violations(&scope, &[], &history, 0);
    assert!(result.is_ok());

    let lint = result.unwrap_or(super::LintResult {
        violations: Vec::new(),
    });
    assert!(lint.violations.is_empty());
}

#[test]
fn incremental_deepen_step_grows_exponentially_from_base_shift() {
    let first = incremental_deepen_step(10, 0);
    let second = incremental_deepen_step(10, 1);
    let third = incremental_deepen_step(10, 2);

    assert_eq!(first.ok(), Some(10));
    assert_eq!(second.ok(), Some(20));
    assert_eq!(third.ok(), Some(40));
}

#[test]
fn incremental_deepen_step_returns_error_on_overflow() {
    let value = incremental_deepen_step(u32::MAX, 1);
    assert!(value.is_err());
}
