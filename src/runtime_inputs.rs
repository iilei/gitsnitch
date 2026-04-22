use std::collections::BTreeMap;
use std::env;

use super::{AppError, Args, DEFAULT_ENV_PREFIX, LintScope, REMAP_SUPPORTED_KEYS};

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

pub(crate) fn parse_remap_env_vars(
    entries: &[String],
) -> Result<BTreeMap<String, String>, AppError> {
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

pub(crate) fn remapped_or_prefixed_env_non_empty_with_lookup<F>(
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

pub(crate) fn resolve_lint_scope(
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
            "ref range scope requires both --source-ref and --target-ref".to_owned(),
        )),
        (None, None, None) => Err(AppError::Message(
            "no lint scope provided; set either --commit-sha or both --source-ref and --target-ref (or equivalent env vars)"
                .to_owned(),
        )),
        _ => Err(AppError::Message("invalid lint scope combination".to_owned())),
    }
}

pub(crate) fn remapped_or_prefixed_env_non_empty_for_runtime(
    prefix: &str,
    key_suffix: &str,
    remap_env_vars: &BTreeMap<String, String>,
) -> Option<String> {
    remapped_or_prefixed_env_non_empty(prefix, key_suffix, remap_env_vars)
}
