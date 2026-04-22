use super::{
    AppError, Args, ConfigSource, DEFAULT_ENV_PREFIX, RenderOutput, autodiscover_config,
    check_is_repo_at, git_repo_root_at, parse_remap_env_vars, read_config_content,
    read_config_content_from_reader, remapped_or_prefixed_env_non_empty_with_lookup,
    severity_band_label, validate_env_resolution_mode,
};
use crate::config;
use clap::Parser;
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_args() -> Args {
    Args {
        config: None,
        verbose: 0,
        output_format: RenderOutput::Json,
        violation_severity_as_exit_code: 0,
        no_violation_severity_as_exit_code: 0,
        custom_meta: vec![],
        preset: vec![],
        commit_sha: None,
        source_ref: None,
        target_ref: None,
        default_branch: None,
        env_prefix: DEFAULT_ENV_PREFIX.to_owned(),
        remap_env_var: vec![],
    }
}

#[test]
fn validate_custom_meta_accepts_valid_entries() {
    let entries = vec!["team=platform".to_owned(), "env=ci".to_owned()];
    let result = super::validate_custom_meta(&entries);
    assert!(result.is_ok());
}

#[test]
fn validate_custom_meta_rejects_entry_without_separator() {
    let entries = vec!["team-platform".to_owned()];
    let result = super::validate_custom_meta(&entries);
    assert!(result.is_err());

    let message = match result {
        Err(AppError::Message(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };
    assert!(message.contains("expected key=value"));
}

#[test]
fn validate_custom_meta_rejects_empty_key() {
    let entries = vec!["   =value".to_owned()];
    let result = super::validate_custom_meta(&entries);
    assert!(result.is_err());

    let message = match result {
        Err(AppError::Message(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };
    assert!(message.contains("key cannot be empty"));
}

#[test]
fn validate_custom_meta_rejects_empty_value() {
    let entries = vec!["key=   ".to_owned()];
    let result = super::validate_custom_meta(&entries);
    assert!(result.is_err());

    let message = match result {
        Err(AppError::Message(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };
    assert!(message.contains("value cannot be empty"));
}

#[test]
fn resolve_violation_exit_code_returns_zero_when_disabled() {
    let exit_code = super::resolve_violation_exit_code(false, &[10, 20, 30]);
    assert_eq!(exit_code, 0);
}

#[test]
fn resolve_violation_exit_code_returns_zero_when_all_zero() {
    let exit_code = super::resolve_violation_exit_code(true, &[0, 0, 0]);
    assert_eq!(exit_code, 0);
}

#[test]
fn resolve_violation_exit_code_returns_max_violation_severity_when_enabled() {
    let exit_code = super::resolve_violation_exit_code(true, &[100, 200, 40]);
    assert_eq!(exit_code, 200);
}

#[test]
fn resolve_violation_severity_exit_switch_prefers_cli_override() {
    let value = super::resolve_violation_severity_exit_switch(Some(false), Some(true));
    assert!(!value);
}

#[test]
fn resolve_violation_severity_exit_switch_uses_config_when_no_cli_override() {
    let value = super::resolve_violation_severity_exit_switch(None, Some(true));
    assert!(value);
}

#[test]
fn resolve_violation_severity_exit_switch_defaults_to_false() {
    let value = super::resolve_violation_severity_exit_switch(None, None);
    assert!(!value);
}

#[test]
fn resolve_toggle_override_returns_some_true_when_enable_flag_set() {
    let value = super::resolve_toggle_override(true, false);
    assert_eq!(value, Some(true));
}

#[test]
fn resolve_toggle_override_returns_some_false_when_disable_flag_set() {
    let value = super::resolve_toggle_override(false, true);
    assert_eq!(value, Some(false));
}

#[test]
fn resolve_toggle_override_returns_none_when_no_flags_set() {
    let value = super::resolve_toggle_override(false, false);
    assert_eq!(value, None);
}

#[test]
fn resolve_lint_scope_uses_commit_sha_when_provided() {
    let mut args = test_args();
    args.commit_sha = Some("abc123".to_owned());
    let remap = BTreeMap::new();

    let scope = super::resolve_lint_scope(&args, &remap);
    assert!(scope.is_ok());
    let scope = scope.ok();
    assert!(matches!(scope, Some(super::LintScope::CommitSha(_))));
    if let Some(super::LintScope::CommitSha(sha)) = scope {
        assert_eq!(sha, "abc123");
    }
}

#[test]
fn resolve_lint_scope_uses_ref_range_when_both_refs_are_provided() {
    let mut args = test_args();
    args.source_ref = Some("feature".to_owned());
    args.target_ref = Some("main".to_owned());
    let remap = BTreeMap::new();

    let scope = super::resolve_lint_scope(&args, &remap);
    assert!(scope.is_ok());

    let scope = scope.ok();
    assert!(matches!(scope, Some(super::LintScope::RefRange { .. })));
    if let Some(super::LintScope::RefRange {
        source_ref,
        target_ref,
    }) = scope
    {
        assert_eq!(source_ref, "feature");
        assert_eq!(target_ref, "main");
    }
}

#[test]
fn resolve_lint_scope_rejects_mixing_commit_and_ref_range_modes() {
    let mut args = test_args();
    args.commit_sha = Some("abc123".to_owned());
    args.source_ref = Some("feature".to_owned());
    args.target_ref = Some("main".to_owned());
    let remap = BTreeMap::new();

    let result = super::resolve_lint_scope(&args, &remap);
    assert!(result.is_err());

    let message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(message.contains("mutually exclusive"));
}

#[test]
fn resolve_lint_scope_rejects_partial_ref_range() {
    let mut args = test_args();
    args.source_ref = Some("feature".to_owned());
    let remap = BTreeMap::new();

    let result = super::resolve_lint_scope(&args, &remap);
    assert!(result.is_err());

    let message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(message.contains("requires both --source-ref and --target-ref"));
}

#[test]
fn resolve_lint_scope_rejects_missing_scope() {
    let args = test_args();
    let remap = BTreeMap::new();

    let result = super::resolve_lint_scope(&args, &remap);
    assert!(result.is_err());

    let message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(message.contains("no lint scope provided"));
}

#[test]
fn terminal_supports_color_respects_no_color_precedence() {
    let value = super::terminal_supports_color_from_inputs(
        true,
        Some("xterm-256color"),
        Some("1"),
        Some("1"),
        true,
    );
    assert!(!value);
}

#[test]
fn terminal_supports_color_disables_for_term_dumb() {
    let value = super::terminal_supports_color_from_inputs(false, Some("dumb"), None, None, true);
    assert!(!value);
}

#[test]
fn terminal_supports_color_enables_for_clicolor_force() {
    let value = super::terminal_supports_color_from_inputs(
        false,
        Some("xterm"),
        Some("1"),
        Some("0"),
        false,
    );
    assert!(value);
}

#[test]
fn terminal_supports_color_disables_for_clicolor_zero() {
    let value =
        super::terminal_supports_color_from_inputs(false, Some("xterm"), None, Some("0"), true);
    assert!(!value);
}

#[test]
fn terminal_supports_color_falls_back_to_tty_state() {
    let tty_true = super::terminal_supports_color_from_inputs(false, None, None, None, true);
    let tty_false = super::terminal_supports_color_from_inputs(false, None, None, None, false);
    assert!(tty_true);
    assert!(!tty_false);
}

#[test]
fn args_parser_accepts_text_decorative_output_format() {
    let parsed = Args::try_parse_from([
        "gitsnitch",
        "--output-format",
        "text-decorative",
        "--commit-sha",
        "deadbeef",
    ]);
    assert!(parsed.is_ok());

    let args = parsed.unwrap_or_else(|_| test_args());
    assert!(matches!(args.output_format, RenderOutput::TextDecorative));
}

#[test]
fn emit_report_supports_plain_and_decorative_text_variants() {
    let violations = Vec::<crate::violations::Violation>::new();
    let severity_bands = config::SeverityBands::default();
    let custom_meta = config::CustomMeta::new();
    let scope = super::LintScope::CommitSha("abc1234".to_owned());

    let plain = super::emit_report(
        &violations,
        &severity_bands,
        false,
        RenderOutput::TextPlain,
        &custom_meta,
        "pre",
        &scope,
    );
    assert!(plain.is_ok());

    let decorative = super::emit_report(
        &violations,
        &severity_bands,
        false,
        RenderOutput::TextDecorative,
        &custom_meta,
        "pre",
        &scope,
    );
    assert!(decorative.is_ok());
}

#[test]
fn parse_remap_env_vars_accepts_supported_keys() {
    let entries = vec![
        "GITSNITCH_SOURCE_REF=PRE_COMMIT_TO_REF".to_owned(),
        "GITSNITCH_TARGET_REF=PRE_COMMIT_FROM_REF".to_owned(),
        "GITSNITCH_COMMIT_SHA=CI_COMMIT_SHA".to_owned(),
        "GITSNITCH_CONFIG_ROOT=CI_CONFIG_ROOT".to_owned(),
    ];

    let result = parse_remap_env_vars(&entries);
    assert!(result.is_ok());

    let remap = result.unwrap_or_default();
    assert_eq!(
        remap.get("GITSNITCH_SOURCE_REF"),
        Some(&"PRE_COMMIT_TO_REF".to_owned())
    );
    assert_eq!(
        remap.get("GITSNITCH_TARGET_REF"),
        Some(&"PRE_COMMIT_FROM_REF".to_owned())
    );
    assert_eq!(
        remap.get("GITSNITCH_COMMIT_SHA"),
        Some(&"CI_COMMIT_SHA".to_owned())
    );
    assert_eq!(
        remap.get("GITSNITCH_CONFIG_ROOT"),
        Some(&"CI_CONFIG_ROOT".to_owned())
    );
}

#[test]
fn remap_supported_keys_set_is_intentionally_limited() {
    let expected = [
        "GITSNITCH_SOURCE_REF",
        "GITSNITCH_TARGET_REF",
        "GITSNITCH_COMMIT_SHA",
        "GITSNITCH_CONFIG_ROOT",
    ];

    assert_eq!(super::REMAP_SUPPORTED_KEYS, expected);
}

#[test]
fn parse_remap_env_vars_rejects_unsupported_key() {
    let entries = vec!["SOURCE_REF=MY_SOURCE".to_owned()];

    let result = parse_remap_env_vars(&entries);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("invalid --remap-env-var key 'SOURCE_REF'"));
}

#[test]
fn parse_remap_env_vars_rejects_duplicate_keys() {
    let entries = vec![
        "GITSNITCH_SOURCE_REF=A".to_owned(),
        "GITSNITCH_SOURCE_REF=B".to_owned(),
    ];

    let result = parse_remap_env_vars(&entries);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("duplicate --remap-env-var key 'GITSNITCH_SOURCE_REF'"));
}

#[test]
fn parse_remap_env_vars_rejects_entry_without_separator() {
    let entries = vec!["GITSNITCH_SOURCE_REF".to_owned()];

    let result = parse_remap_env_vars(&entries);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("expected KEY=ENV_VAR"));
}

#[test]
fn parse_remap_env_vars_rejects_empty_key() {
    let entries = vec!["   =PRE_COMMIT_TO_REF".to_owned()];

    let result = parse_remap_env_vars(&entries);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("key cannot be empty"));
}

#[test]
fn parse_remap_env_vars_rejects_empty_env_var_name() {
    let entries = vec!["GITSNITCH_SOURCE_REF=   ".to_owned()];

    let result = parse_remap_env_vars(&entries);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("env var cannot be empty"));
}

#[test]
fn validate_env_resolution_mode_rejects_custom_prefix_with_remap() {
    let mut args = test_args();
    args.env_prefix = "MY_CUSTOM_".to_owned();
    args.remap_env_var = vec!["GITSNITCH_SOURCE_REF=PRE_COMMIT_TO_REF".to_owned()];

    let result = validate_env_resolution_mode(&args);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };

    assert!(error_message.contains("mutually exclusive"));
}

#[test]
fn remapped_lookup_prefers_remapped_env_var_over_prefixed_env_var() {
    let mut remap = BTreeMap::new();
    remap.insert(
        "GITSNITCH_SOURCE_REF".to_owned(),
        "PRE_COMMIT_TO_REF".to_owned(),
    );

    let mut env_map = BTreeMap::new();
    env_map.insert("PRE_COMMIT_TO_REF".to_owned(), "abc123".to_owned());
    env_map.insert("GITSNITCH_SOURCE_REF".to_owned(), "fallback".to_owned());

    let resolved = remapped_or_prefixed_env_non_empty_with_lookup(
        DEFAULT_ENV_PREFIX,
        "SOURCE_REF",
        &remap,
        |key| env_map.get(key).cloned(),
    );

    assert_eq!(resolved, Some("abc123".to_owned()));
}

#[test]
fn remapped_lookup_does_not_fallback_when_remapped_env_var_is_empty() {
    let mut remap = BTreeMap::new();
    remap.insert(
        "GITSNITCH_SOURCE_REF".to_owned(),
        "PRE_COMMIT_TO_REF".to_owned(),
    );

    let mut env_map = BTreeMap::new();
    env_map.insert("PRE_COMMIT_TO_REF".to_owned(), "   ".to_owned());
    env_map.insert("GITSNITCH_SOURCE_REF".to_owned(), "fallback".to_owned());

    let resolved = remapped_or_prefixed_env_non_empty_with_lookup(
        DEFAULT_ENV_PREFIX,
        "SOURCE_REF",
        &remap,
        |key| env_map.get(key).cloned(),
    );

    assert_eq!(resolved, None);
}

#[test]
fn remapped_lookup_uses_prefix_when_key_is_not_remapped() {
    let remap = BTreeMap::new();
    let mut env_map = BTreeMap::new();
    env_map.insert("GITSNITCH_TARGET_REF".to_owned(), "main".to_owned());

    let resolved = remapped_or_prefixed_env_non_empty_with_lookup(
        DEFAULT_ENV_PREFIX,
        "TARGET_REF",
        &remap,
        |key| env_map.get(key).cloned(),
    );

    assert_eq!(resolved, Some("main".to_owned()));
}

#[test]
fn remapped_lookup_supports_config_root_key() {
    let mut remap = BTreeMap::new();
    remap.insert(
        "GITSNITCH_CONFIG_ROOT".to_owned(),
        "MY_CONFIG_ROOT".to_owned(),
    );

    let mut env_map = BTreeMap::new();
    env_map.insert("MY_CONFIG_ROOT".to_owned(), "/tmp/config".to_owned());
    env_map.insert(
        "GITSNITCH_CONFIG_ROOT".to_owned(),
        "/tmp/fallback".to_owned(),
    );

    let resolved = remapped_or_prefixed_env_non_empty_with_lookup(
        DEFAULT_ENV_PREFIX,
        "CONFIG_ROOT",
        &remap,
        |key| env_map.get(key).cloned(),
    );

    assert_eq!(resolved, Some("/tmp/config".to_owned()));
}

#[test]
fn severity_band_label_resolves_expected_band() {
    let bands = config::SeverityBands {
        fatal: 200,
        error: 10,
        warning: 2,
        information: 0,
    };

    assert_eq!(severity_band_label(220, &bands), "Fatal");
    assert_eq!(severity_band_label(10, &bands), "Error");
    assert_eq!(severity_band_label(5, &bands), "Warning");
    assert_eq!(severity_band_label(1, &bands), "Information");
}

#[test]
fn render_template_includes_violation_context_values() {
    let payload = serde_json::json!({
        "assertion_alias": "conventional-title",
        "commit_sha": "abc123",
        "commit_sha_short": "abc123",
        "commit_title": "feat: add lint",
        "description": "desc",
        "severity": 10,
        "severity_band": "Error",
        "text": "[Error:10] conventional-title",
    });
    let template =
        "title={{ violation.commit_title }} band={{ violation.severity_band }}".to_owned();

    let payloads = vec![payload.clone()];
    let result = super::render_banner_template(&template, &payload, &payloads);
    let rendered = match result {
        Ok(Some(value)) => value,
        Ok(None) | Err(_) => String::new(),
    };

    assert_eq!(rendered, "title=feat: add lint band=Error");
}

#[test]
fn render_template_supports_loop_over_violations() {
    let payload = serde_json::json!({
        "assertion_alias": "conventional-title",
        "commit_sha": "abc123",
        "commit_sha_short": "abc123",
        "commit_title": "feat: add lint",
        "description": "desc",
        "severity": 10,
        "severity_band": "Error",
        "text": "[Error:10] conventional-title",
    });
    let template = "{% for v in violations %}{{ v.assertion_alias }}{% endfor %}".to_owned();

    let payloads = vec![payload.clone()];
    let result = super::render_banner_template(&template, &payload, &payloads);
    let rendered = match result {
        Ok(Some(value)) => value,
        Ok(None) | Err(_) => String::new(),
    };

    assert_eq!(rendered, "conventional-title");
}

#[test]
fn plain_text_report_template_renders_expected_core_fields() {
    let report = serde_json::json!({
        "schema_version": "pre",
        "generated_at": "2026-01-01T00:00:00Z",
        "gitsnitch_version": "0.0.0-test",
        "violation_severity_as_exit_code": true,
        "custom_meta": {"team": "platform"},
        "violation_banners": [
            {
                "assertion_alias": "forbid-wip",
                "text": "Avoid WIP titles",
                "hint": "Use feat/fix prefix",
                "severity": 10,
                "severity_band": "Error",
                "code": "[Error:10]",
                "description": ""
            }
        ],
        "violations": {
            "Fatal": [],
            "Error": [
                {
                    "assertion_alias": "forbid-wip",
                    "commit_sha": "abc123",
                    "commit_sha_short": "abc123",
                    "commit_title": "wip"
                }
            ],
            "Warning": [],
            "Information": []
        }
    });

    let rendered = minijinja::Environment::new().render_str(
        super::TEXT_REPORT_TEMPLATE,
        minijinja::context!(
            report => report,
            terminal => serde_json::json!({"supports_color": false, "is_ci": false})
        ),
    );
    assert!(
        rendered.is_ok(),
        "plain text template failed to render: {}",
        rendered
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );

    let output = rendered.unwrap_or_default();
    assert!(output.contains("GitSnitch"));
    assert!(output.contains("[Error:10]"));
}

#[test]
fn decorative_text_report_template_renders_with_terminal_context() {
    let report = serde_json::json!({
        "schema_version": "pre",
        "generated_at": "2026-01-01T00:00:00Z",
        "gitsnitch_version": "0.0.0-test",
        "violation_severity_as_exit_code": false,
        "custom_meta": {},
        "violation_banners": [
            {
                "assertion_alias": "forbid-wip",
                "text": "Avoid WIP titles",
                "hint": "Use feat/fix prefix",
                "severity": 10,
                "severity_band": "Error",
                "code": "[Error:10]",
                "description": ""
            }
        ],
        "violations": {
            "Fatal": [],
            "Error": [
                {
                    "assertion_alias": "forbid-wip",
                    "commit_sha": "abc123",
                    "commit_sha_short": "abc123",
                    "commit_title": "wip"
                }
            ],
            "Warning": [],
            "Information": []
        }
    });

    let rendered = minijinja::Environment::new().render_str(
        super::TEXT_REPORT_TEMPLATE,
        minijinja::context!(
            report => report,
            terminal => serde_json::json!({"supports_color": false, "is_ci": false})
        ),
    );
    assert!(
        rendered.is_ok(),
        "decorative text template failed to render: {}",
        rendered
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );

    let output = rendered.unwrap_or_default();
    assert!(output.contains("GitSnitch"));
    assert!(output.contains("[Error:10]"));
}

#[test]
fn read_config_content_returns_none_for_auto_discover() {
    let result = read_config_content(&ConfigSource::AutoDiscover);
    assert!(result.is_ok());

    let content = result.unwrap_or_default();
    assert!(content.is_none());
}

#[test]
fn read_config_content_reads_file_content() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-read-config-file-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));
    let create_dir = fs::create_dir_all(&root);
    assert!(create_dir.is_ok());

    let config_path = root.join(".gitsnitch.toml");
    let expected = "api_version = \"pre\"\n";
    let write_result = fs::write(&config_path, expected);
    assert!(write_result.is_ok());

    let result = read_config_content(&ConfigSource::File(config_path));
    let _ = fs::remove_dir_all(&root);

    assert!(result.is_ok());
    let content = result.unwrap_or_default();
    assert_eq!(content, Some(expected.to_owned()));
}

#[test]
fn read_config_content_returns_error_when_file_does_not_exist() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let missing_path = std::env::temp_dir().join(format!(
        "gitsnitch-read-config-missing-{}-{}-missing.toml",
        std::process::id(),
        duration.as_nanos()
    ));

    let result = read_config_content(&ConfigSource::File(missing_path));
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("failed to read config file"));
}

#[test]
fn read_config_content_from_reader_returns_error_when_stdin_is_blank() {
    let stdin_data = Cursor::new("   \n\t");

    let result = read_config_content_from_reader(stdin_data);
    assert!(result.is_err());

    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };

    assert!(error_message.contains("stdin was empty"));
}

#[test]
fn read_config_content_from_reader_returns_ok_for_non_empty_stdin() {
    let expected = "api_version = \"pre\"\n";
    let stdin_data = Cursor::new(expected);

    let result = read_config_content_from_reader(stdin_data);
    assert!(result.is_ok());

    let content = result.unwrap_or_default();
    assert_eq!(content, Some(expected.to_owned()));
}

#[test]
fn check_is_repo_at_returns_error_when_git_reports_false() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let bare_repo_path = std::env::temp_dir().join(format!(
        "gitsnitch-bare-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&bare_repo_path);
    assert!(create_dir.is_ok());

    let init_status = Command::new("git")
        .args(["init", "--bare"])
        .current_dir(&bare_repo_path)
        .status();
    assert!(init_status.is_ok());
    let Ok(status) = init_status else {
        let _ = fs::remove_dir_all(&bare_repo_path);
        return;
    };
    assert!(status.success());

    let result = check_is_repo_at(PathBuf::as_path(&bare_repo_path));
    let _ = fs::remove_dir_all(&bare_repo_path);

    assert!(result.is_err());
    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(()) | Err(_) => String::new(),
    };
    assert!(error_message.contains("current directory is not a git repository"));
}

#[test]
fn git_repo_root_at_returns_repo_root_from_nested_directory() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let repo_path = std::env::temp_dir().join(format!(
        "gitsnitch-root-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&repo_path);
    assert!(create_dir.is_ok());

    let init_status = Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .status();
    assert!(init_status.is_ok());
    let Ok(status) = init_status else {
        let _ = fs::remove_dir_all(&repo_path);
        return;
    };
    assert!(status.success());

    let nested_path = repo_path.join("nested").join("deeper");
    let nested_create = fs::create_dir_all(&nested_path);
    assert!(nested_create.is_ok());

    let result = git_repo_root_at(PathBuf::as_path(&nested_path));
    let _ = fs::remove_dir_all(&repo_path);

    assert!(result.is_ok());
    let Ok(root) = result else {
        return;
    };
    assert_eq!(root, repo_path);
}

#[test]
fn git_repo_root_at_returns_error_outside_repo() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let outside_path = std::env::temp_dir().join(format!(
        "gitsnitch-nonrepo-root-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&outside_path);
    assert!(create_dir.is_ok());

    let result = git_repo_root_at(PathBuf::as_path(&outside_path));
    let _ = fs::remove_dir_all(&outside_path);

    assert!(result.is_err());
    let error_message = match result {
        Err(AppError::Message(message)) => message,
        Ok(_) | Err(_) => String::new(),
    };
    assert!(error_message.contains("failed to determine git repository root"));
}

#[test]
fn autodiscover_config_returns_none_when_no_candidates_exist() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-autodiscover-none-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&root);
    assert!(create_dir.is_ok());

    let found = autodiscover_config(PathBuf::as_path(&root));
    let _ = fs::remove_dir_all(&root);

    assert!(found.is_none());
}

#[test]
fn autodiscover_config_prefers_highest_precedence_candidate() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-autodiscover-precedence-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&root);
    assert!(create_dir.is_ok());

    let lower_candidate = root.join(".gitsnitch.json");
    let higher_candidate = root.join(".gitsnitch.toml");

    let write_lower = fs::write(&lower_candidate, "{}");
    assert!(write_lower.is_ok());
    let write_higher = fs::write(&higher_candidate, "api_version = \"pre\"\n");
    assert!(write_higher.is_ok());

    let found = autodiscover_config(PathBuf::as_path(&root));
    let _ = fs::remove_dir_all(&root);

    assert_eq!(found, Some(higher_candidate));
}

#[test]
fn autodiscover_config_ignores_directory_candidates() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-autodiscover-directory-candidate-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&root);
    assert!(create_dir.is_ok());

    let higher_candidate_dir = root.join(".gitsnitch.toml");
    let create_candidate_dir = fs::create_dir_all(&higher_candidate_dir);
    assert!(create_candidate_dir.is_ok());

    let next_candidate_file = root.join(".gitsnitchrc");
    let write_selected = fs::write(&next_candidate_file, "api_version = \"pre\"\n");
    assert!(write_selected.is_ok());

    let found = autodiscover_config(PathBuf::as_path(&root));
    let _ = fs::remove_dir_all(&root);

    assert_eq!(found, Some(next_candidate_file));
}

#[test]
fn autodiscover_config_falls_back_to_next_available_candidate() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-autodiscover-fallback-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));

    let create_dir = fs::create_dir_all(&root);
    assert!(create_dir.is_ok());

    let selected_candidate = root.join(".gitsnitchrc");
    let write_selected = fs::write(&selected_candidate, "api_version = \"pre\"\n");
    assert!(write_selected.is_ok());

    let found = autodiscover_config(PathBuf::as_path(&root));
    let _ = fs::remove_dir_all(&root);

    assert_eq!(found, Some(selected_candidate));
}

#[test]
fn resolve_config_source_distinguishes_auto_discover_file_and_stdin() {
    let auto = super::resolve_config_source(None);
    assert!(matches!(auto, ConfigSource::AutoDiscover));

    let file_path = PathBuf::from("config.toml");
    let file = super::resolve_config_source(Some(&file_path));
    assert!(matches!(file, ConfigSource::File(_)));

    let stdin_path = PathBuf::from("-");
    let stdin = super::resolve_config_source(Some(&stdin_path));
    assert!(matches!(stdin, ConfigSource::Stdin));
}

#[test]
fn load_runtime_config_reads_explicit_file_and_preserves_settings() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-load-runtime-config-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));
    assert!(fs::create_dir_all(&root).is_ok());

    let config_path = root.join("runtime.toml");
    let config = "api_version = \"pre\"\nviolation_severity_as_exit_code = true\n\n[custom_meta]\nteam = \"platform\"\n\n[severity_bands]\nFatal = 200\nError = 100\nWarning = 10\nInformation = 0\n\n[history]\nautoheal_shallow = \"full\"\nautoheal_shallow_shift = 3\nautoheal_shallow_tries = 2\n\n[[assertions]]\nalias = \"a1\"\nseverity = 10\n[assertions.must_satisfy]\n[assertions.must_satisfy.condition]\ntype = \"msg_match_any\"\nmode = \"raw\"\npatterns = [\"^feat\"]\n";
    assert!(fs::write(&config_path, config).is_ok());

    let mut args = test_args();
    args.config = Some(config_path);
    let remap = BTreeMap::new();

    let loaded = super::load_runtime_config(&args, &remap);
    let _ = fs::remove_dir_all(&root);

    assert!(loaded.is_ok());
    let Ok(loaded) = loaded else {
        return;
    };
    assert_eq!(loaded.assertions.len(), 1);
    assert!(matches!(
        loaded.history.autoheal_shallow,
        config::AutohealShallow::Full
    ));
    assert_eq!(loaded.history.autoheal_shallow_shift, 3);
    assert_eq!(loaded.history.autoheal_shallow_tries, 2);
    assert_eq!(loaded.severity_bands.fatal, 200);
    assert_eq!(loaded.custom_meta.get("team"), Some(&"platform".to_owned()));
    assert_eq!(loaded.violation_severity_as_exit_code, Some(true));
}

#[test]
fn load_runtime_config_returns_config_error_for_invalid_explicit_file() {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH);
    assert!(since_epoch.is_ok());
    let Ok(duration) = since_epoch else {
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "gitsnitch-load-runtime-config-invalid-{}-{}",
        std::process::id(),
        duration.as_nanos()
    ));
    assert!(fs::create_dir_all(&root).is_ok());

    let config_path = root.join("runtime.toml");
    assert!(fs::write(&config_path, "this is not valid toml").is_ok());

    let mut args = test_args();
    args.config = Some(config_path);
    let remap = BTreeMap::new();

    let loaded = super::load_runtime_config(&args, &remap);
    let _ = fs::remove_dir_all(&root);

    assert!(loaded.is_err());
    assert!(matches!(loaded, Err(AppError::Config(_))));
}

#[test]
fn build_violation_banners_deduplicates_aliases_and_collects_short_shas() {
    let violations = vec![
        crate::violations::Violation {
            commit_sha: "1234567890abcdef".to_owned(),
            commit_title: "feat: one".to_owned(),
            assertion_alias: "same".to_owned(),
            assertion_description: "desc".to_owned(),
            severity: 120,
            banner: "{{ violation.commit_sha_short }} / {{ violations | length }}".to_owned(),
            hint: "hint".to_owned(),
        },
        crate::violations::Violation {
            commit_sha: "abcdef1234567890".to_owned(),
            commit_title: "feat: two".to_owned(),
            assertion_alias: "same".to_owned(),
            assertion_description: "desc".to_owned(),
            severity: 120,
            banner: String::new(),
            hint: "hint".to_owned(),
        },
    ];
    let bands = config::SeverityBands::default();
    let entries = super::build_violation_context_entries(&violations, &bands);
    let by_band = super::group_entries_by_band(&entries);
    let payloads = super::serialize_violation_payloads(&entries);
    assert!(payloads.is_ok());

    let banners = super::build_violation_banners(&by_band, &payloads.unwrap_or_default());
    assert!(banners.is_ok());
    let banners = banners.unwrap_or_default();

    assert_eq!(banners.len(), 1);
    let banner = banners.first();
    assert!(banner.is_some());
    let Some(banner) = banner else {
        return;
    };
    assert_eq!(banner.assertion_alias, "same");
    assert_eq!(banner.commit_sha_shorts.len(), 2);
    assert!(banner.text.contains("1234567 / 2"));
}

#[test]
fn emit_report_supports_json_variants() {
    let violations = Vec::<crate::violations::Violation>::new();
    let severity_bands = config::SeverityBands::default();
    let custom_meta = config::CustomMeta::new();
    let scope = super::LintScope::CommitSha("def5678".to_owned());

    let json = super::emit_report(
        &violations,
        &severity_bands,
        false,
        RenderOutput::Json,
        &custom_meta,
        "pre",
        &scope,
    );
    assert!(json.is_ok());

    let json_compact = super::emit_report(
        &violations,
        &severity_bands,
        false,
        RenderOutput::JsonCompact,
        &custom_meta,
        "pre",
        &scope,
    );
    assert!(json_compact.is_ok());
}
