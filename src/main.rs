use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use chrono::Utc;
use clap::{ArgAction, Parser, ValueEnum};
use minijinja::Environment;
use serde::Serialize;
use thiserror::Error;

mod config;
mod exit_codes;
mod presets;
mod violations;

const EXIT_INTERNAL_GENERIC: i32 = 251;
const EXIT_INTERNAL_CONFIG: i32 = 252;
const EXIT_INTERNAL_DEPENDENCY: i32 = 253;
const EXIT_INTERNAL_IO: i32 = 254;
const EXIT_INTERNAL_UNEXPECTED: i32 = 255;
const DEFAULT_ENV_PREFIX: &str = "GITSNITCH_";
const PLAIN_TEXT_REPORT_TEMPLATE: &str = include_str!("presets_data/report_plain_text.jinja2");

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum RenderOutput {
    Json,
    JsonCompact,
    TextPlain,
}

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

    /// Select report output renderer: json, json-compact, text-plain.
    #[arg(long, value_enum, default_value_t = RenderOutput::Json)]
    render_output: RenderOutput,

    /// Override config and force violation severity to be used as process exit code.
    #[arg(
        long,
        action = ArgAction::Count,
        conflicts_with = "no_violation_severity_as_exit_code"
    )]
    violation_severity_as_exit_code: u8,

    /// Override config and force violations to be exit-code silent.
    #[arg(long, action = ArgAction::Count)]
    no_violation_severity_as_exit_code: u8,

    /// Add root-level custom metadata entry (key=value), repeatable.
    #[arg(long = "custom-meta")]
    custom_meta: Vec<String>,

    /// Select one or more embedded presets by name (dash-case), repeatable.
    #[arg(long = "preset")]
    preset: Vec<String>,

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
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
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
    let current_dir = env::current_dir()
        .map_err(|error| AppError::Message(format!("failed to get current directory: {error}")))?;
    check_is_repo_at(&current_dir)
}

fn check_is_repo_at(path: &Path) -> Result<(), AppError> {
    let output = Command::new("git")
        .current_dir(path)
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
    let current_dir = env::current_dir()
        .map_err(|error| AppError::Message(format!("failed to get current directory: {error}")))?;
    git_repo_root_at(&current_dir)
}

fn git_repo_root_at(path: &Path) -> Result<PathBuf, AppError> {
    let output = Command::new("git")
        .current_dir(path)
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
        ConfigSource::Stdin => read_config_content_from_reader(io::stdin()),
    }
}

fn read_config_content_from_reader<R: Read>(mut reader: R) -> Result<Option<String>, AppError> {
    let mut buffer = String::new();
    reader
        .read_to_string(&mut buffer)
        .map_err(|error| AppError::Message(format!("failed to read config from stdin: {error}")))?;

    if buffer.trim().is_empty() {
        return Err(AppError::Message(
            "--config - was provided, but stdin was empty".to_owned(),
        ));
    }

    Ok(Some(buffer))
}

struct LoadedRuntimeConfig {
    assertions: Vec<config::Assertion>,
    history: config::History,
    severity_bands: config::SeverityBands,
    custom_meta: config::CustomMeta,
    violation_severity_as_exit_code: Option<bool>,
}

fn log_lint_scope(lint_scope: &LintScope, verbose: u8) {
    if verbose == 0 {
        return;
    }

    match lint_scope {
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

fn load_runtime_config(
    args: &Args,
    remap_env_vars: &BTreeMap<String, String>,
) -> Result<LoadedRuntimeConfig, AppError> {
    let config_source = resolve_config_source(args.config.as_ref());
    let resolved_source = match config_source {
        ConfigSource::AutoDiscover => {
            let root = match remapped_or_prefixed_env_non_empty(
                &args.env_prefix,
                "CONFIG_ROOT",
                remap_env_vars,
            ) {
                Some(val) => PathBuf::from(val),
                _ => git_repo_root()?,
            };
            autodiscover_config(&root).map_or(ConfigSource::AutoDiscover, ConfigSource::File)
        }
        other => other,
    };

    let config_content = read_config_content(&resolved_source)?;
    if let Some(content) = config_content {
        let source_path = match &resolved_source {
            ConfigSource::File(path) => Some(path.as_path()),
            _ => None,
        };
        let cfg = config::parse(&content, source_path)?;

        return Ok(LoadedRuntimeConfig {
            assertions: cfg.assertions,
            history: cfg.history.unwrap_or_default(),
            severity_bands: cfg.severity_bands,
            custom_meta: cfg.custom_meta,
            violation_severity_as_exit_code: Some(cfg.violation_severity_as_exit_code),
        });
    }

    Ok(LoadedRuntimeConfig {
        assertions: Vec::new(),
        history: config::History::default(),
        severity_bands: config::SeverityBands::default(),
        custom_meta: config::CustomMeta::new(),
        violation_severity_as_exit_code: None,
    })
}

fn run(args: &Args) -> Result<(), AppError> {
    validate_custom_meta(&args.custom_meta)?;
    validate_env_resolution_mode(args)?;
    presets::validate_cli_preset_names(&args.preset)?;

    let remap_env_vars = parse_remap_env_vars(&args.remap_env_var)?;
    if args.verbose >= 3 {
        for (key, env_var) in &remap_env_vars {
            eprintln!("env remap: {key} <- {env_var}");
        }
    }

    let lint_scope = resolve_lint_scope(args, &remap_env_vars)?;
    log_lint_scope(&lint_scope, args.verbose);

    let loaded = load_runtime_config(args, &remap_env_vars)?;
    let mut assertions = loaded.assertions;
    let history = loaded.history;
    let severity_bands = loaded.severity_bands;
    let config_custom_meta = loaded.custom_meta;
    let config_violation_severity_as_exit_code = loaded.violation_severity_as_exit_code;

    let preset_assertions = presets::select_assertions_from_presets(&args.preset)?;
    assertions.extend(preset_assertions);
    config::validate_assertions(&assertions)?;

    let cli_violation_exit_override = resolve_toggle_override(
        args.violation_severity_as_exit_code > 0,
        args.no_violation_severity_as_exit_code > 0,
    );
    let effective_violation_severity_as_exit_code = resolve_violation_severity_exit_switch(
        cli_violation_exit_override,
        config_violation_severity_as_exit_code,
    );
    let collected =
        violations::collect_violations(&lint_scope, &assertions, &history, args.verbose)?;
    let violation_severities = collected
        .violations
        .iter()
        .map(|violation| violation.severity)
        .collect::<Vec<_>>();

    let api_version_str = "pre";
    emit_report(
        &collected.violations,
        &severity_bands,
        effective_violation_severity_as_exit_code,
        args.render_output,
        &config_custom_meta,
        api_version_str,
    )?;

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

const fn resolve_toggle_override(enable_flag: bool, disable_flag: bool) -> Option<bool> {
    if enable_flag {
        Some(true)
    } else if disable_flag {
        Some(false)
    } else {
        None
    }
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

fn render_banner_template(
    template: &str,
    violation_payload: &serde_json::Value,
    all_violations_payloads: &[serde_json::Value],
) -> Result<Option<String>, AppError> {
    if template.trim().is_empty() {
        return Ok(None);
    }

    let environment = Environment::new();
    let rendered = environment
        .render_str(
            template,
            minijinja::context!(
                violation => violation_payload,
                violations => all_violations_payloads,
                violation_banners => all_violations_payloads,
            ),
        )
        .map_err(|error| AppError::Message(format!("failed to render banner template: {error}")))?;

    if rendered.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(rendered))
}

#[derive(Serialize)]
struct ViolationContextItem<'a> {
    assertion_alias: &'a str,
    commit_sha: &'a str,
    commit_sha_short: &'a str,
    commit_title: &'a str,
    description: &'a str,
    severity: u8,
    severity_band: &'a str,
    text: String,
    banner: &'a str,
    hint: &'a str,
}

#[derive(Serialize)]
struct ViolationBandItem {
    assertion_alias: String,
    commit_sha: String,
    commit_sha_short: String,
    commit_title: String,
}

#[derive(Serialize)]
struct ViolationsByBand {
    #[serde(rename = "Fatal")]
    fatal: Vec<ViolationBandItem>,
    #[serde(rename = "Error")]
    error: Vec<ViolationBandItem>,
    #[serde(rename = "Warning")]
    warning: Vec<ViolationBandItem>,
    #[serde(rename = "Information")]
    information: Vec<ViolationBandItem>,
}

#[derive(Serialize)]
struct ViolationBanner {
    assertion_alias: String,
    text: String,
    hint: String,
    severity: u8,
    severity_band: String,
    code: String,
    description: String,
    commit_sha_shorts: Vec<String>,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    schema_version: &'a str,
    generated_at: String,
    gitsnitch_version: &'a str,
    violation_severity_as_exit_code: bool,
    custom_meta: &'a config::CustomMeta,
    violation_banners: Vec<ViolationBanner>,
    violations: ViolationsByBand,
}

const BAND_ORDER: &[&str] = &["Fatal", "Error", "Warning", "Information"];

fn format_violation_code(severity_band: &str, severity: u8) -> String {
    format!("[{severity_band}:{severity}]")
}

fn build_violation_context_entries<'a>(
    collected_violations: &'a [violations::Violation],
    severity_bands: &config::SeverityBands,
) -> Vec<ViolationContextItem<'a>> {
    collected_violations
        .iter()
        .map(|v| {
            let severity_band = severity_band_label(v.severity, severity_bands);
            let sha_short = v.commit_sha.get(..7).unwrap_or(v.commit_sha.as_str());
            ViolationContextItem {
                assertion_alias: v.assertion_alias.as_str(),
                commit_sha: v.commit_sha.as_str(),
                commit_sha_short: sha_short,
                commit_title: v.commit_title.as_str(),
                description: v.assertion_description.as_str(),
                severity: v.severity,
                severity_band,
                text: format!("[{severity_band}:{}] {}", v.severity, v.assertion_alias),
                banner: v.banner.as_str(),
                hint: v.hint.as_str(),
            }
        })
        .collect()
}

fn group_entries_by_band<'a>(
    entries: &'a [ViolationContextItem<'a>],
) -> BTreeMap<&'a str, Vec<&'a ViolationContextItem<'a>>> {
    let mut by_band: BTreeMap<&str, Vec<&ViolationContextItem<'_>>> = BTreeMap::new();
    for band in BAND_ORDER {
        by_band.insert(*band, Vec::new());
    }
    for entry in entries {
        by_band.entry(entry.severity_band).or_default().push(entry);
    }

    // Strict descending numeric severity within each band.
    // Tie-breakers keep ordering deterministic across runs.
    for entries_in_band in by_band.values_mut() {
        entries_in_band.sort_by(|left, right| {
            right
                .severity
                .cmp(&left.severity)
                .then_with(|| left.assertion_alias.cmp(right.assertion_alias))
                .then_with(|| left.commit_sha.cmp(right.commit_sha))
        });
    }

    by_band
}

fn serialize_violation_payloads(
    entries: &[ViolationContextItem<'_>],
) -> Result<Vec<serde_json::Value>, AppError> {
    entries
        .iter()
        .map(|entry| {
            serde_json::to_value(entry).map_err(|error| {
                AppError::Message(format!(
                    "failed to serialize violation context for banner template: {error}"
                ))
            })
        })
        .collect::<Result<Vec<_>, AppError>>()
}

fn build_violation_banners(
    by_band: &BTreeMap<&str, Vec<&ViolationContextItem<'_>>>,
    all_violations_payloads: &[serde_json::Value],
) -> Result<Vec<ViolationBanner>, AppError> {
    let mut seen_assertion_aliases: BTreeSet<&str> = BTreeSet::new();
    let mut violation_banners: Vec<ViolationBanner> = Vec::new();

    for band in BAND_ORDER {
        for entry in by_band
            .get(*band)
            .map_or(&[] as &[&ViolationContextItem<'_>], Vec::as_slice)
        {
            if !seen_assertion_aliases.insert(entry.assertion_alias) {
                continue;
            }

            let violation_payload = serde_json::to_value(entry).map_err(|error| {
                AppError::Message(format!(
                    "failed to serialize current violation for banner template: {error}"
                ))
            })?;

            let rendered_text =
                render_banner_template(entry.banner, &violation_payload, all_violations_payloads)?;

            let commit_sha_shorts = collect_short_shas_for_alias(by_band, entry.assertion_alias);
            let code = format_violation_code(band, entry.severity);

            violation_banners.push(ViolationBanner {
                assertion_alias: entry.assertion_alias.to_owned(),
                text: rendered_text.unwrap_or_default(),
                hint: entry.hint.to_owned(),
                severity: entry.severity,
                severity_band: (*band).to_owned(),
                code,
                description: entry.description.to_owned(),
                commit_sha_shorts,
            });
        }
    }

    Ok(violation_banners)
}

fn collect_short_shas_for_alias(
    by_band: &BTreeMap<&str, Vec<&ViolationContextItem<'_>>>,
    assertion_alias: &str,
) -> Vec<String> {
    let mut unique = BTreeMap::new();

    for band in BAND_ORDER {
        for entry in by_band
            .get(*band)
            .map_or(&[] as &[&ViolationContextItem<'_>], Vec::as_slice)
        {
            if entry.assertion_alias == assertion_alias {
                unique
                    .entry(entry.commit_sha_short)
                    .or_insert_with(|| entry.commit_sha_short.to_owned());
            }
        }
    }

    unique.into_values().collect()
}

fn make_band_items(
    by_band: &BTreeMap<&str, Vec<&ViolationContextItem<'_>>>,
    band: &str,
) -> Vec<ViolationBandItem> {
    by_band
        .get(band)
        .map_or(&[] as &[&ViolationContextItem<'_>], Vec::as_slice)
        .iter()
        .map(|entry| ViolationBandItem {
            assertion_alias: entry.assertion_alias.to_owned(),
            commit_sha: entry.commit_sha.to_owned(),
            commit_sha_short: entry.commit_sha_short.to_owned(),
            commit_title: entry.commit_title.to_owned(),
        })
        .collect()
}

fn build_violations_by_band(
    by_band: &BTreeMap<&str, Vec<&ViolationContextItem<'_>>>,
) -> ViolationsByBand {
    ViolationsByBand {
        fatal: make_band_items(by_band, "Fatal"),
        error: make_band_items(by_band, "Error"),
        warning: make_band_items(by_band, "Warning"),
        information: make_band_items(by_band, "Information"),
    }
}

fn build_report<'a>(
    collected_violations: &[violations::Violation],
    severity_bands: &config::SeverityBands,
    effective_violation_severity_as_exit_code: bool,
    custom_meta: &'a config::CustomMeta,
    api_version_str: &'a str,
) -> Result<JsonReport<'a>, AppError> {
    let entries = build_violation_context_entries(collected_violations, severity_bands);
    let by_band = group_entries_by_band(&entries);
    let all_violations_payloads = serialize_violation_payloads(&entries)?;
    let violation_banners = build_violation_banners(&by_band, &all_violations_payloads)?;
    let violations = build_violations_by_band(&by_band);

    Ok(JsonReport {
        schema_version: api_version_str,
        generated_at: Utc::now().to_rfc3339(),
        gitsnitch_version: env!("CARGO_PKG_VERSION"),
        violation_severity_as_exit_code: effective_violation_severity_as_exit_code,
        custom_meta,
        violation_banners,
        violations,
    })
}

fn emit_json_report(
    collected_violations: &[violations::Violation],
    severity_bands: &config::SeverityBands,
    effective_violation_severity_as_exit_code: bool,
    compact_output: bool,
    custom_meta: &config::CustomMeta,
    api_version_str: &str,
) -> Result<(), AppError> {
    let report = build_report(
        collected_violations,
        severity_bands,
        effective_violation_severity_as_exit_code,
        custom_meta,
        api_version_str,
    )?;

    let serialized = if compact_output {
        serde_json::to_string(&report)
    } else {
        serde_json::to_string_pretty(&report)
    }
    .map_err(|error| AppError::Message(format!("failed to serialize report as JSON: {error}")))?;
    println!("{serialized}");

    Ok(())
}

fn emit_plain_text_report(
    collected_violations: &[violations::Violation],
    severity_bands: &config::SeverityBands,
    effective_violation_severity_as_exit_code: bool,
    custom_meta: &config::CustomMeta,
    api_version_str: &str,
) -> Result<(), AppError> {
    let report = build_report(
        collected_violations,
        severity_bands,
        effective_violation_severity_as_exit_code,
        custom_meta,
        api_version_str,
    )?;

    let report_payload = serde_json::to_value(&report).map_err(|error| {
        AppError::Message(format!("failed to prepare plain-text report: {error}"))
    })?;

    let environment = Environment::new();
    let rendered = environment
        .render_str(
            PLAIN_TEXT_REPORT_TEMPLATE,
            minijinja::context!(report => report_payload),
        )
        .map_err(|error| {
            AppError::Message(format!("failed to render plain-text report: {error}"))
        })?;

    println!("{rendered}");

    Ok(())
}

fn emit_report(
    collected_violations: &[violations::Violation],
    severity_bands: &config::SeverityBands,
    effective_violation_severity_as_exit_code: bool,
    render_output: RenderOutput,
    custom_meta: &config::CustomMeta,
    api_version_str: &str,
) -> Result<(), AppError> {
    match render_output {
        RenderOutput::Json => emit_json_report(
            collected_violations,
            severity_bands,
            effective_violation_severity_as_exit_code,
            false,
            custom_meta,
            api_version_str,
        ),
        RenderOutput::JsonCompact => emit_json_report(
            collected_violations,
            severity_bands,
            effective_violation_severity_as_exit_code,
            true,
            custom_meta,
            api_version_str,
        ),
        RenderOutput::TextPlain => emit_plain_text_report(
            collected_violations,
            severity_bands,
            effective_violation_severity_as_exit_code,
            custom_meta,
            api_version_str,
        ),
    }
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
mod main_tests;
