use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use clap::{ArgAction, Parser};
use minijinja::Environment;
use serde_json::json;
use thiserror::Error;

mod config;
mod exit_codes;
mod violations;

const EXIT_INTERNAL_GENERIC: i32 = 251;
const EXIT_INTERNAL_CONFIG: i32 = 252;
const EXIT_INTERNAL_DEPENDENCY: i32 = 253;
const EXIT_INTERNAL_IO: i32 = 254;
const EXIT_INTERNAL_UNEXPECTED: i32 = 255;
const DEFAULT_ENV_PREFIX: &str = "GITSNITCH_";

#[derive(Debug, Parser)]
#[command(name = "gitsnitch")]
#[command(version)]
#[command(about = "Git commit history linter for local and CI")]
struct Args {
    /// Config file path (default: auto-discover)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Increase verbosity (-v, -vv, ...)
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,

    /// Override config for whether to use max violation severity as process exit code.
    #[arg(long)]
    violation_severity_as_exit_code: Option<bool>,

    /// Add root-level custom metadata entry (key=value), repeatable.
    #[arg(long = "custom-meta")]
    custom_meta: Vec<String>,

    /// Commit SHA to lint
    #[arg(long)]
    commit_sha: Option<String>,

    /// Source ref to lint against target ref.
    ///
    /// Must be used together with --target-ref.
    #[arg(long)]
    source_ref: Option<String>,

    /// Target ref to compare against source ref.
    ///
    /// Must be used together with --source-ref.
    #[arg(long)]
    target_ref: Option<String>,

    /// Default/main branch name
    #[arg(long)]
    default_branch: Option<String>,

    /// Prefix for environment variable lookups (default: GITSNITCH_).
    ///
    /// Controls which env vars are consulted, e.g. `{PREFIX}CONFIG_ROOT`
    /// overrides the autodiscovery root directory.
    #[arg(long, default_value = DEFAULT_ENV_PREFIX)]
    env_prefix: String,

    /// Remap a canonical env key to an environment variable (`KEY=ENV_VAR`), repeatable.
    ///
    /// Supported keys: `GITSNITCH_SOURCE_REF`, `GITSNITCH_TARGET_REF`, `GITSNITCH_COMMIT_SHA`, `GITSNITCH_CONFIG_ROOT`.
    /// For a remapped key, the remapped env var is used instead of the prefix lookup.
    /// This option is mutually exclusive with non-default `--env-prefix` values.
    #[arg(long = "remap-env-var")]
    remap_env_var: Vec<String>,
}

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Exit(#[from] ExitError),
    #[error("config error: {0}")]
    Config(#[from] config::ConfigError),
}

#[derive(Debug, Error)]
#[error("{message}")]
pub(crate) struct ExitError {
    code: i32,
    message: String,
}

#[derive(Debug)]
enum ConfigSource {
    AutoDiscover,
    File(PathBuf),
    Stdin,
}

#[derive(Debug)]
pub(crate) enum LintScope {
    CommitSha(String),
    RefRange {
        source_ref: String,
        target_ref: String,
    },
}

/// Candidate filenames probed during autodiscovery, in precedence order.
///
/// `.gitsnitchrc` has no extension and is parsed as TOML.
const AUTODISCOVER_CANDIDATES: &[&str] = &[
    ".gitsnitch.toml",
    ".gitsnitchrc",
    ".gitsnitch.json",
    ".gitsnitch.json5",
    ".gitsnitch.yaml",
    ".gitsnitch.yml",
];

const REMAP_SUPPORTED_KEYS: &[&str] = &[
    "GITSNITCH_SOURCE_REF",
    "GITSNITCH_TARGET_REF",
    "GITSNITCH_COMMIT_SHA",
    "GITSNITCH_CONFIG_ROOT",
];

fn check_git_installed() -> Result<(), AppError> {
    match Command::new("git").arg("--version").status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(AppError::Exit(ExitError {
            code: EXIT_INTERNAL_DEPENDENCY,
            message: "git is installed but not functioning correctly".to_owned(),
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(AppError::Exit(ExitError {
                code: EXIT_INTERNAL_DEPENDENCY,
                message: "git is not installed or not on PATH".to_owned(),
            }))
        }
        Err(error) => Err(AppError::Exit(ExitError {
            code: EXIT_INTERNAL_IO,
            message: format!("failed to execute git --version: {error}"),
        })),
    }
}

fn check_is_repo() -> Result<(), AppError> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|error| AppError::Message(format!("failed to check git repository: {error}")))?;

    if !output.status.success() {
        return Err(AppError::Message(
            "current directory is not a git repository".to_owned(),
        ));
    }

    let inside_repo = String::from_utf8_lossy(&output.stdout).trim().eq("true");
    if inside_repo {
        Ok(())
    } else {
        Err(AppError::Message(
            "current directory is not a git repository".to_owned(),
        ))
    }
}

fn git_repo_root() -> Result<PathBuf, AppError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| AppError::Message(format!("failed to find git repo root: {error}")))?;

    if !output.status.success() {
        return Err(AppError::Message(
            "failed to determine git repository root".to_owned(),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(PathBuf::from(path))
}

fn autodiscover_config(root: &Path) -> Option<PathBuf> {
    AUTODISCOVER_CANDIDATES
        .iter()
        .map(|name| root.join(name))
        .find(|p| p.is_file())
}

fn validate_custom_meta(entries: &[String]) -> Result<(), AppError> {
    for entry in entries {
        let Some((key, value)) = entry.split_once('=') else {
            return Err(AppError::Message(format!(
                "invalid --custom-meta entry '{entry}': expected key=value"
            )));
        };

        if key.trim().is_empty() {
            return Err(AppError::Message(format!(
                "invalid --custom-meta entry '{entry}': key cannot be empty"
            )));
        }

        if value.trim().is_empty() {
            return Err(AppError::Message(format!(
                "invalid --custom-meta entry '{entry}': value cannot be empty"
            )));
        }
    }

    Ok(())
}

fn resolve_config_source(config: Option<&PathBuf>) -> ConfigSource {
    match config {
        Some(path) if path.as_os_str() == OsStr::new("-") => ConfigSource::Stdin,
        Some(path) => ConfigSource::File(path.clone()),
        None => ConfigSource::AutoDiscover,
    }
}

fn prefixed_env_non_empty_with_lookup<F>(
    prefix: &str,
    key_suffix: &str,
    lookup: F,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let key = format!("{prefix}{key_suffix}");
    normalize_opt_non_empty(lookup(&key))
}

fn normalize_opt_non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_owned())
        .and_then(|v| if v.is_empty() { None } else { Some(v) })
}

fn canonical_prefixed_key(key_suffix: &str) -> String {
    format!("{DEFAULT_ENV_PREFIX}{key_suffix}")
}

fn validate_env_resolution_mode(args: &Args) -> Result<(), AppError> {
    if !args.remap_env_var.is_empty() && args.env_prefix != DEFAULT_ENV_PREFIX {
        return Err(AppError::Message(
            "--remap-env-var is mutually exclusive with non-default --env-prefix; use default GITSNITCH_ prefix when remapping"
                .to_owned(),
        ));
    }

    Ok(())
}

fn parse_remap_env_vars(entries: &[String]) -> Result<BTreeMap<String, String>, AppError> {
    let mut remap_env_vars = BTreeMap::new();

    for entry in entries {
        let Some((key_raw, env_var_raw)) = entry.split_once('=') else {
            return Err(AppError::Message(format!(
                "invalid --remap-env-var entry '{entry}': expected KEY=ENV_VAR"
            )));
        };

        let key = key_raw.trim();
        let env_var = env_var_raw.trim();

        if key.is_empty() {
            return Err(AppError::Message(format!(
                "invalid --remap-env-var entry '{entry}': key cannot be empty"
            )));
        }
        if env_var.is_empty() {
            return Err(AppError::Message(format!(
                "invalid --remap-env-var entry '{entry}': env var cannot be empty"
            )));
        }

        if !REMAP_SUPPORTED_KEYS.contains(&key) {
            return Err(AppError::Message(format!(
                "invalid --remap-env-var key '{key}': supported keys are {}",
                REMAP_SUPPORTED_KEYS.join(", ")
            )));
        }

        if remap_env_vars
            .insert(key.to_owned(), env_var.to_owned())
            .is_some()
        {
            return Err(AppError::Message(format!(
                "duplicate --remap-env-var key '{key}': each key can only be remapped once"
            )));
        }
    }

    Ok(remap_env_vars)
}

fn remapped_or_prefixed_env_non_empty(
    prefix: &str,
    key_suffix: &str,
    remap_env_vars: &BTreeMap<String, String>,
) -> Option<String> {
    remapped_or_prefixed_env_non_empty_with_lookup(prefix, key_suffix, remap_env_vars, |key| {
        env::var(key).ok()
    })
}

fn remapped_or_prefixed_env_non_empty_with_lookup<F>(
    prefix: &str,
    key_suffix: &str,
    remap_env_vars: &BTreeMap<String, String>,
    lookup: F,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let canonical_key = canonical_prefixed_key(key_suffix);
    if let Some(remapped_env_var) = remap_env_vars.get(&canonical_key) {
        return normalize_opt_non_empty(lookup(remapped_env_var));
    }

    prefixed_env_non_empty_with_lookup(prefix, key_suffix, lookup)
}

fn resolve_lint_scope(
    args: &Args,
    remap_env_vars: &BTreeMap<String, String>,
) -> Result<LintScope, AppError> {
    let commit_sha = normalize_opt_non_empty(args.commit_sha.clone()).or_else(|| {
        remapped_or_prefixed_env_non_empty(&args.env_prefix, "COMMIT_SHA", remap_env_vars)
    });

    let source_ref = normalize_opt_non_empty(args.source_ref.clone()).or_else(|| {
        remapped_or_prefixed_env_non_empty(&args.env_prefix, "SOURCE_REF", remap_env_vars)
    });

    let target_ref = normalize_opt_non_empty(args.target_ref.clone()).or_else(|| {
        remapped_or_prefixed_env_non_empty(&args.env_prefix, "TARGET_REF", remap_env_vars)
    });

    if commit_sha.is_some() && (source_ref.is_some() || target_ref.is_some()) {
        return Err(AppError::Message(
            "commit scope and ref range scope are mutually exclusive; use either --commit-sha or both --source-ref and --target-ref"
                .to_owned(),
        ));
    }

    match (commit_sha, source_ref, target_ref) {
        (Some(sha), None, None) => Ok(LintScope::CommitSha(sha)),
        (None, Some(source), Some(target)) => Ok(LintScope::RefRange {
            source_ref: source,
            target_ref: target,
        }),
        (None, Some(_), None) | (None, None, Some(_)) => Err(AppError::Message(
            "ref range scope requires both --source-ref and --target-ref"
                .to_owned(),
        )),
        (None, None, None) => Err(AppError::Message(
            "no lint scope provided; set either --commit-sha or both --source-ref and --target-ref (or equivalent env vars)"
                .to_owned(),
        )),
        _ => Err(AppError::Message("invalid lint scope combination".to_owned())),
    }
}

fn read_config_content(config_source: &ConfigSource) -> Result<Option<String>, AppError> {
    match config_source {
        ConfigSource::AutoDiscover => Ok(None),
        ConfigSource::File(path) => {
            let content = std::fs::read_to_string(path).map_err(|error| {
                AppError::Message(format!(
                    "failed to read config file '{}': {error}",
                    path.display()
                ))
            })?;
            Ok(Some(content))
        }
        ConfigSource::Stdin => {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer).map_err(|error| {
                AppError::Message(format!("failed to read config from stdin: {error}"))
            })?;

            if buffer.trim().is_empty() {
                return Err(AppError::Message(
                    "--config - was provided, but stdin was empty".to_owned(),
                ));
            }

            Ok(Some(buffer))
        }
    }
}

fn run(args: &Args) -> Result<(), AppError> {
    validate_custom_meta(&args.custom_meta)?;
    validate_env_resolution_mode(args)?;

    let remap_env_vars = parse_remap_env_vars(&args.remap_env_var)?;
    if args.verbose >= 3 {
        for (key, env_var) in &remap_env_vars {
            eprintln!("env remap: {key} <- {env_var}");
        }
    }

    let lint_scope = resolve_lint_scope(args, &remap_env_vars)?;
    if args.verbose > 0 {
        match &lint_scope {
            LintScope::CommitSha(sha) => {
                eprintln!("lint scope: commit_sha={sha}");
            }
            LintScope::RefRange {
                source_ref,
                target_ref,
            } => {
                eprintln!("lint scope: source_ref={source_ref} target_ref={target_ref}");
            }
        }
    }

    let config_source = resolve_config_source(args.config.as_ref());
    let resolved_source = match config_source {
        ConfigSource::AutoDiscover => {
            let root = match remapped_or_prefixed_env_non_empty(
                &args.env_prefix,
                "CONFIG_ROOT",
                &remap_env_vars,
            ) {
                Some(val) => PathBuf::from(val),
                _ => git_repo_root()?,
            };
            autodiscover_config(&root).map_or(ConfigSource::AutoDiscover, ConfigSource::File)
        }
        other => other,
    };
    let config_content = read_config_content(&resolved_source)?;
    let mut assertions: Vec<config::Assertion> = Vec::new();
    let mut history = config::History::default();
    let mut severity_bands = config::SeverityBands::default();
    let config_violation_severity_as_exit_code = if let Some(content) = config_content {
        let source_path = match &resolved_source {
            ConfigSource::File(p) => Some(p.as_path()),
            _ => None,
        };
        let cfg = config::parse(&content, source_path)?;
        history = cfg.history.unwrap_or_default();
        assertions = cfg.assertions;
        severity_bands = cfg.severity_bands;
        Some(cfg.violation_severity_as_exit_code)
    } else {
        None
    };

    let effective_violation_severity_as_exit_code = resolve_violation_severity_exit_switch(
        args.violation_severity_as_exit_code,
        config_violation_severity_as_exit_code,
    );
    let collected_violations = violations::collect_violations(&lint_scope, &assertions, &history)?;
    let violation_severities = collected_violations
        .iter()
        .map(|violation| violation.severity)
        .collect::<Vec<_>>();

    if !collected_violations.is_empty() {
        print_violations(&collected_violations, &severity_bands)?;
    }

    let violation_exit_code = resolve_violation_exit_code(
        effective_violation_severity_as_exit_code,
        &violation_severities,
    );

    if violation_exit_code > 0 {
        return Err(AppError::Exit(ExitError {
            code: violation_exit_code,
            message: format!(
                "violations found: {} failing assertion checks",
                violation_severities.len()
            ),
        }));
    }

    Ok(())
}

fn resolve_violation_severity_exit_switch(
    cli_override: Option<bool>,
    config_value: Option<bool>,
) -> bool {
    exit_codes::resolve_violation_severity_exit_switch(cli_override, config_value)
}

fn resolve_violation_exit_code(
    violation_severity_as_exit_code: bool,
    violation_severities: &[u8],
) -> i32 {
    exit_codes::resolve_violation_exit_code(violation_severity_as_exit_code, violation_severities)
}

const fn severity_band_label(severity: u8, bands: &config::SeverityBands) -> &'static str {
    if severity >= bands.fatal {
        "Fatal"
    } else if severity >= bands.error {
        "Error"
    } else if severity >= bands.warning {
        "Warning"
    } else {
        "Information"
    }
}

fn render_template(
    template: &str,
    violation: &violations::Violation,
    severity_band: &str,
) -> Result<Option<String>, AppError> {
    if template.trim().is_empty() {
        return Ok(None);
    }

    let environment = Environment::new();
    let text = format!(
        "[{severity_band}:{}] {}",
        violation.severity, violation.assertion_alias
    );
    let payload = json!({
        "text": text,
        "alias": violation.assertion_alias,
        "description": violation.assertion_description,
        "severity": violation.severity,
        "severity_band": severity_band,
        "commit_sha": violation.commit_sha,
        "commit_title": violation.commit_title,
    });

    let rendered = environment
        .render_str(
            template,
            minijinja::context!(
                violation => &payload,
                violations => std::slice::from_ref(&payload),
                violation_banners => std::slice::from_ref(&payload),
            ),
        )
        .map_err(|error| {
            AppError::Message(format!(
                "failed to render assertion template '{}': {error}",
                violation.assertion_alias
            ))
        })?;

    if rendered.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(rendered))
}

fn print_violations(
    collected_violations: &[violations::Violation],
    severity_bands: &config::SeverityBands,
) -> Result<(), AppError> {
    for violation in collected_violations {
        let severity_band = severity_band_label(violation.severity, severity_bands);
        eprintln!(
            "violation [{severity_band}:{}] {} ({})",
            violation.severity, violation.assertion_alias, violation.commit_sha
        );

        if let Some(rendered_banner) = render_template(&violation.banner, violation, severity_band)?
        {
            eprintln!("{rendered_banner}");
        }

        if let Some(rendered_hint) = render_template(&violation.hint, violation, severity_band)? {
            eprintln!("hint: {rendered_hint}");
        }
    }

    Ok(())
}

fn main() {
    let args = Args::parse();

    let result = (|| -> Result<(), AppError> {
        check_git_installed()?;
        check_is_repo()?;
        run(&args)
    })();

    match result {
        Ok(()) => {}
        Err(AppError::Exit(exit_error)) => {
            eprintln!("{}", exit_error.message);
            let code = if (0..=255).contains(&exit_error.code) {
                exit_error.code
            } else {
                EXIT_INTERNAL_UNEXPECTED
            };
            process::exit(code);
        }
        Err(AppError::Message(message)) => {
            eprintln!("{message}");
            process::exit(EXIT_INTERNAL_GENERIC);
        }
        Err(AppError::Config(error)) => {
            eprintln!("{error}");
            process::exit(EXIT_INTERNAL_CONFIG);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppError, Args, DEFAULT_ENV_PREFIX, parse_remap_env_vars,
        remapped_or_prefixed_env_non_empty_with_lookup, severity_band_label,
        validate_env_resolution_mode,
    };
    use crate::{config, violations};
    use std::collections::BTreeMap;

    fn test_args() -> Args {
        Args {
            config: None,
            verbose: 0,
            violation_severity_as_exit_code: None,
            custom_meta: vec![],
            commit_sha: None,
            source_ref: None,
            target_ref: None,
            default_branch: None,
            env_prefix: DEFAULT_ENV_PREFIX.to_owned(),
            remap_env_var: vec![],
        }
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
        let violation = violations::Violation {
            commit_sha: "abc123".to_owned(),
            commit_title: "feat: add lint".to_owned(),
            assertion_alias: "conventional-title".to_owned(),
            assertion_description: "desc".to_owned(),
            severity: 10,
            banner: "title={{ violation.commit_title }} band={{ violation.severity_band }}"
                .to_owned(),
            hint: String::new(),
        };

        let result = super::render_template(&violation.banner, &violation, "Error");
        let rendered = match result {
            Ok(Some(value)) => value,
            Ok(None) | Err(_) => String::new(),
        };

        assert_eq!(rendered, "title=feat: add lint band=Error");
    }

    #[test]
    fn render_template_supports_loop_over_violations() {
        let violation = violations::Violation {
            commit_sha: "abc123".to_owned(),
            commit_title: "feat: add lint".to_owned(),
            assertion_alias: "conventional-title".to_owned(),
            assertion_description: "desc".to_owned(),
            severity: 10,
            banner: "{% for v in violations %}{{ v.alias }}{% endfor %}".to_owned(),
            hint: String::new(),
        };

        let result = super::render_template(&violation.banner, &violation, "Error");
        let rendered = match result {
            Ok(Some(value)) => value,
            Ok(None) | Err(_) => String::new(),
        };

        assert_eq!(rendered, "conventional-title");
    }
}
