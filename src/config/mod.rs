//! Config loading, parsing, and semantic validation.
//!
//! Supported formats: TOML, JSON, JSON5, YAML.
//! Format is detected by file extension; extensionless files (e.g. `.gitsnitchrc`) are parsed as TOML.

// TODO remove this before first release
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to parse config as TOML: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("failed to parse config as JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("failed to parse config as JSON5: {0}")]
    Json5(#[from] json5::Error),

    #[error("failed to parse config as YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("semantic error: {0}")]
    Semantic(String),
}

// ── Format ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Toml,
    Json,
    Json5,
    Yaml,
}

impl ConfigFormat {
    /// Detect format from file extension. Extensionless → TOML.
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Self::Json,
            Some("json5") => Self::Json5,
            Some("yaml" | "yml") => Self::Yaml,
            _ => Self::Toml, // .toml and extensionless (e.g. .gitsnitchrc) both use TOML
        }
    }
}

// ── Config types ─────────────────────────────────────────────────────────────

pub type CustomMeta = HashMap<String, String>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub api_version: ApiVersion,

    #[serde(default)]
    pub history: Option<History>,

    #[serde(default)]
    pub custom_meta: CustomMeta,

    #[serde(default)]
    pub violation_severity_as_exit_code: bool,

    #[serde(default = "defaults::severity_bands")]
    pub severity_bands: SeverityBands,

    #[serde(default)]
    pub assertions: Vec<Assertion>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum ApiVersion {
    #[serde(rename = "pre")]
    Pre,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub struct History {
    #[serde(default = "defaults::autoheal_shallow")]
    pub autoheal_shallow: AutohealShallow,

    #[serde(default = "defaults::autoheal_shallow_shift")]
    pub autoheal_shallow_shift: u32,

    #[serde(default = "defaults::autoheal_shallow_tries")]
    pub autoheal_shallow_tries: u32,
}

impl Default for History {
    fn default() -> Self {
        Self {
            autoheal_shallow: defaults::autoheal_shallow(),
            autoheal_shallow_shift: defaults::autoheal_shallow_shift(),
            autoheal_shallow_tries: defaults::autoheal_shallow_tries(),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AutohealShallow {
    Never,
    Incremental,
    Full,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeverityBands {
    #[serde(rename = "Fatal")]
    pub fatal: u8,
    #[serde(rename = "Error")]
    pub error: u8,
    #[serde(rename = "Warning")]
    pub warning: u8,
    #[serde(rename = "Information")]
    pub information: u8,
}

impl Default for SeverityBands {
    fn default() -> Self {
        defaults::severity_bands()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub alias: String,

    #[serde(default)]
    pub skip: bool,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub banner: String,

    #[serde(default)]
    pub hint: String,

    #[serde(default = "defaults::severity")]
    pub severity: u8,

    pub must_satisfy: ConditionContainer,

    #[serde(default)]
    pub skip_if: Option<ConditionContainer>,

    #[serde(default)]
    pub custom_meta: CustomMeta,
}

#[derive(Debug, Deserialize)]
pub struct ConditionContainer {
    pub condition: Condition,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    MsgMatchAny(MsgMatchCondition),
    MsgMatchNone(MsgMatchCondition),
    DiffMatchAny(DiffMatchCondition),
    DiffMatchNone(DiffMatchCondition),
    BranchMatch(BranchMatchCondition),
    ThresholdCompare(ThresholdCondition),
}

#[derive(Debug, Deserialize)]
pub struct MsgMatchCondition {
    #[serde(default)]
    pub name: String,
    pub mode: MsgMode,
    pub patterns: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MsgMode {
    Raw,
    Title,
    Body,
}

#[derive(Debug, Deserialize)]
pub struct DiffMatchCondition {
    #[serde(default)]
    pub name: String,
    pub mode: DiffMode,
    pub patterns: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    Raw,
    File,
    Line,
}

#[derive(Debug, Deserialize)]
pub struct BranchMatchCondition {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ThresholdCondition {
    #[serde(default)]
    pub name: String,
    pub metric: ThresholdMetric,
    pub operator: ThresholdOperator,
    pub value: u32,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdMetric {
    LineCount,
    FileCount,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdOperator {
    Lte,
    Gte,
}

// ── Defaults ─────────────────────────────────────────────────────────────────

mod defaults {
    use super::{AutohealShallow, SeverityBands};

    pub const fn severity() -> u8 {
        10
    }
    pub const fn autoheal_shallow_shift() -> u32 {
        10
    }
    pub const fn autoheal_shallow_tries() -> u32 {
        6
    }
    pub const fn autoheal_shallow() -> AutohealShallow {
        AutohealShallow::Incremental
    }
    pub const fn severity_bands() -> SeverityBands {
        SeverityBands {
            fatal: 250,
            error: 10,
            warning: 1,
            information: 0,
        }
    }
}

// ── Parse ─────────────────────────────────────────────────────────────────────

/// Parse config content using the format detected from `source_path`.
/// If `source_path` is `None` (e.g. stdin), defaults to TOML.
pub fn parse(content: &str, source_path: Option<&Path>) -> Result<Config, ConfigError> {
    let format = source_path.map_or(ConfigFormat::Toml, ConfigFormat::from_path);
    let config = match format {
        ConfigFormat::Toml => toml::from_str(content)?,
        ConfigFormat::Json => serde_json::from_str(content)?,
        ConfigFormat::Json5 => json5::from_str(content)?,
        ConfigFormat::Yaml => serde_yaml::from_str(content)?,
    };
    validate(config)
}

// ── Semantic validation ───────────────────────────────────────────────────────

fn validate(config: Config) -> Result<Config, ConfigError> {
    validate_assertions(&config.assertions)?;
    validate_severity_bands(&config.severity_bands)?;
    Ok(config)
}

pub fn validate_assertions(assertions: &[Assertion]) -> Result<(), ConfigError> {
    validate_assertion_aliases(assertions)?;
    validate_assertion_severities(assertions)?;
    validate_assertion_patterns(assertions)?;
    Ok(())
}

fn validate_assertion_aliases(assertions: &[Assertion]) -> Result<(), ConfigError> {
    let mut seen = std::collections::HashSet::new();
    for assertion in assertions {
        if !seen.insert(assertion.alias.as_str()) {
            return Err(ConfigError::Semantic(format!(
                "duplicate assertion alias: '{}'",
                assertion.alias
            )));
        }
    }
    Ok(())
}

fn validate_assertion_severities(assertions: &[Assertion]) -> Result<(), ConfigError> {
    for assertion in assertions {
        if assertion.severity > 250 {
            return Err(ConfigError::Semantic(format!(
                "assertion '{}' has severity {} which must be <= 250",
                assertion.alias, assertion.severity
            )));
        }
    }

    Ok(())
}

fn validate_assertion_patterns(assertions: &[Assertion]) -> Result<(), ConfigError> {
    for assertion in assertions {
        validate_condition_patterns(
            &assertion.must_satisfy.condition,
            &assertion.alias,
            "must_satisfy",
        )?;

        if let Some(skip_if) = &assertion.skip_if {
            validate_condition_patterns(&skip_if.condition, &assertion.alias, "skip_if")?;
        }
    }

    Ok(())
}

fn validate_condition_patterns(
    condition: &Condition,
    assertion_alias: &str,
    condition_field: &str,
) -> Result<(), ConfigError> {
    let patterns = match condition {
        Condition::MsgMatchAny(cond) | Condition::MsgMatchNone(cond) => Some(&cond.patterns),
        Condition::DiffMatchAny(cond) | Condition::DiffMatchNone(cond) => Some(&cond.patterns),
        Condition::BranchMatch(cond) => Some(&cond.patterns),
        Condition::ThresholdCompare(_) => None,
    };

    if let Some(patterns) = patterns {
        for (index, pattern) in patterns.iter().enumerate() {
            Regex::new(pattern).map_err(|error| {
                ConfigError::Semantic(format!(
                    "assertion '{assertion_alias}' has invalid regex in {condition_field}.patterns[{index}]: '{pattern}' ({error})"
                ))
            })?;
        }
    }

    Ok(())
}

fn validate_severity_bands(bands: &SeverityBands) -> Result<(), ConfigError> {
    if bands.fatal > 250 {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Fatal ({}) must be <= 250",
            bands.fatal
        )));
    }
    if bands.error > 250 {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Error ({}) must be <= 250",
            bands.error
        )));
    }
    if bands.warning > 250 {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Warning ({}) must be <= 250",
            bands.warning
        )));
    }
    if bands.information > 250 {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Information ({}) must be <= 250",
            bands.information
        )));
    }

    // Bands must be strictly monotonic: Fatal > Error > Warning >= Information.
    // Fatal must be strictly greater than Error, Error strictly greater than Warning.
    if bands.fatal <= bands.error {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Fatal ({}) must be greater than Error ({})",
            bands.fatal, bands.error
        )));
    }
    if bands.error <= bands.warning {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Error ({}) must be greater than Warning ({})",
            bands.error, bands.warning
        )));
    }
    // Information is allowed to equal Warning (both can be 0).
    if bands.warning < bands.information {
        return Err(ConfigError::Semantic(format!(
            "severity_bands: Warning ({}) must be >= Information ({})",
            bands.warning, bands.information
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
