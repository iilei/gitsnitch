use std::process::Command;

use regex::Regex;
use serde::Serialize;

use crate::{AppError, LintScope, config};

#[derive(Debug, Serialize)]
pub struct Violation {
    pub commit_sha: String,
    pub commit_title: String,
    pub assertion_alias: String,
    pub assertion_description: String,
    pub severity: u8,
    pub banner: String,
    pub hint: String,
}

#[derive(Debug)]
pub struct LintResult {
    pub commits_checked: Vec<String>,
    pub violations: Vec<Violation>,
}

#[derive(Debug)]
struct CommitContext {
    raw_message: String,
    title: String,
    body: String,
    diff_raw: String,
    diff_files_joined: String,
    diff_lines_joined: String,
    line_count: u32,
    file_count: u32,
    branches_joined: String,
}

fn run_git_capture(args: &[&str]) -> Result<String, AppError> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| AppError::Message(format!("failed to execute git {args:?}: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(AppError::Message(format!(
            "git command {:?} failed: {}",
            args,
            if stderr.is_empty() {
                "unknown git error".to_owned()
            } else {
                stderr
            }
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn add_context(error: AppError, context: &str) -> AppError {
    match error {
        AppError::Message(message) => AppError::Message(format!("{context}: {message}")),
        other => other,
    }
}

fn log_autoheal(verbose: u8, level: u8, message: &str) {
    if verbose >= level {
        eprintln!("autoheal: {message}");
    }
}

fn resolve_ref_range_commit_shas(
    source_ref: &str,
    target_ref: &str,
    history: &config::History,
    verbose: u8,
) -> Result<Vec<String>, AppError> {
    let range = format!("{target_ref}..{source_ref}");
    log_autoheal(
        verbose,
        2,
        &format!(
            "resolving commit range '{range}' with strategy={:?}",
            history.autoheal_shallow
        ),
    );

    match collect_rev_list(&range) {
        Ok(commits) => Ok(commits),
        Err(original_error) => {
            log_autoheal(
                verbose,
                1,
                "initial range resolution failed; attempting shallow-heal strategy",
            );

            match history.autoheal_shallow {
                config::AutohealShallow::Never => Err(original_error),
                config::AutohealShallow::Full => {
                    maybe_autoheal_shallow_full(&range, original_error, verbose)
                }
                config::AutohealShallow::Incremental => {
                    maybe_autoheal_shallow_incremental(&range, history, original_error, verbose)
                }
            }
        }
    }
}

fn is_shallow_for_heal(verbose: u8) -> Result<bool, AppError> {
    let shallow = is_shallow_repository()?;
    if !shallow {
        log_autoheal(
            verbose,
            2,
            "repository is not shallow; returning original ref resolution error",
        );
    }
    Ok(shallow)
}

fn collect_rev_list(range: &str) -> Result<Vec<String>, AppError> {
    let output = run_git_capture(&["rev-list", "--reverse", range])?;
    let commits = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Ok(commits)
}

fn maybe_autoheal_shallow_full(
    range: &str,
    original_error: AppError,
    verbose: u8,
) -> Result<Vec<String>, AppError> {
    if !is_shallow_for_heal(verbose)? {
        return Err(original_error);
    }

    log_autoheal(
        verbose,
        1,
        "strategy=full; running git fetch --all --unshallow --tags",
    );
    run_git_capture(&["fetch", "--all", "--unshallow", "--tags"]).map_err(|error| {
        add_context(
            error,
            "shallow autoheal (full) failed while running git fetch --all --unshallow --tags",
        )
    })?;
    collect_rev_list(range).map_err(|error| {
        add_context(
            error,
            "shallow autoheal (full) fetch succeeded but range resolution still failed",
        )
    })
}

fn maybe_autoheal_shallow_incremental(
    range: &str,
    history: &config::History,
    original_error: AppError,
    verbose: u8,
) -> Result<Vec<String>, AppError> {
    if !is_shallow_for_heal(verbose)? {
        return Err(original_error);
    }

    let mut last_range_error: Option<AppError> = None;
    for try_index in 0..history.autoheal_shallow_tries {
        let try_number = try_index.checked_add(1).ok_or_else(|| {
            AppError::Message("incremental shallow-heal try counter overflow".to_owned())
        })?;
        let shift = incremental_deepen_step(history.autoheal_shallow_shift, try_index)?;
        let shift_str = shift.to_string();
        log_autoheal(
            verbose,
            1,
            &format!(
                "strategy=incremental; try={}/{}; running git fetch --all --deepen {} --tags",
                try_number, history.autoheal_shallow_tries, shift
            ),
        );
        run_git_capture(&["fetch", "--all", "--deepen", shift_str.as_str(), "--tags"])
            .map_err(|error| {
                add_context(
                    error,
                    &format!(
                        "shallow autoheal (incremental) failed on try {}/{} while running git fetch --all --deepen {} --tags",
                        try_number,
                        history.autoheal_shallow_tries,
                        shift
                    ),
                )
            })?;

        match collect_rev_list(range) {
            Ok(commits) => {
                log_autoheal(
                    verbose,
                    1,
                    &format!(
                        "range resolution succeeded after try {}/{}",
                        try_number, history.autoheal_shallow_tries
                    ),
                );
                return Ok(commits);
            }
            Err(error) => {
                last_range_error = Some(add_context(
                    error,
                    &format!(
                        "range resolution still failed after incremental try {}/{}",
                        try_number, history.autoheal_shallow_tries
                    ),
                ));
            }
        }
    }

    if let Some(error) = last_range_error {
        return Err(error);
    }

    Err(add_context(
        original_error,
        "range resolution failed and no incremental shallow-heal attempts were performed",
    ))
}

fn incremental_deepen_step(base_shift: u32, try_index: u32) -> Result<u32, AppError> {
    let factor = 1_u32
        .checked_shl(try_index)
        .ok_or_else(|| AppError::Message("incremental shallow-heal factor overflow".to_owned()))?;
    base_shift.checked_mul(factor).ok_or_else(|| {
        AppError::Message("incremental shallow-heal deepen value overflow".to_owned())
    })
}

fn is_shallow_repository() -> Result<bool, AppError> {
    let output = run_git_capture(&["rev-parse", "--is-shallow-repository"])?;
    Ok(output.trim() == "true")
}

fn load_commit_message_fields(sha: &str) -> Result<(String, String), AppError> {
    // Use NUL-delimited `%s` + `%B` to preserve the raw message shape reliably.
    // `--no-show-signature` avoids PGP signature lines polluting `%B`.
    let output = run_git_capture(&[
        "log",
        "--no-show-signature",
        "-n",
        "1",
        "--format=%s%x00%B%x00",
        sha,
    ])?;

    let mut fields = output.split('\0');
    let title = fields.next().ok_or_else(|| {
        AppError::Message(format!(
            "failed to parse git log title field for commit '{sha}'"
        ))
    })?;
    let raw_message = fields.next().ok_or_else(|| {
        AppError::Message(format!(
            "failed to parse git log raw message field for commit '{sha}'"
        ))
    })?;

    Ok((title.to_owned(), raw_message.to_owned()))
}

fn body_from_raw_message(raw_message: &str) -> String {
    // Keep the exact delimiter/newline structure between title and body.
    // We only remove the first title line plus its terminating newline.
    raw_message
        .split_once('\n')
        .map_or_else(String::new, |(_, rest)| rest.to_owned())
}

fn collect_diff_lines(diff_raw: &str) -> String {
    diff_raw
        .lines()
        .filter(|line| {
            (line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_numstat_totals(numstat: &str) -> Result<(u32, u32), AppError> {
    let mut total_lines: u32 = 0;
    let mut total_files: u32 = 0;

    for line in numstat.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split('\t');
        let added_raw = fields.next().unwrap_or_default();
        let removed_raw = fields.next().unwrap_or_default();
        let path_raw = fields.next().unwrap_or_default();

        if path_raw.is_empty() {
            continue;
        }

        total_files = total_files.checked_add(1).ok_or_else(|| {
            AppError::Message("file count overflow while parsing git numstat".to_owned())
        })?;

        let added = if added_raw == "-" {
            0
        } else {
            added_raw.parse::<u32>().map_err(|error| {
                AppError::Message(format!(
                    "failed to parse numstat added value '{added_raw}': {error}"
                ))
            })?
        };

        let removed = if removed_raw == "-" {
            0
        } else {
            removed_raw.parse::<u32>().map_err(|error| {
                AppError::Message(format!(
                    "failed to parse numstat removed value '{removed_raw}': {error}"
                ))
            })?
        };

        total_lines = total_lines
            .checked_add(added)
            .and_then(|value| value.checked_add(removed))
            .ok_or_else(|| {
                AppError::Message("line count overflow while parsing git numstat".to_owned())
            })?;
    }

    Ok((total_lines, total_files))
}

fn load_commit_context(sha: &str) -> Result<CommitContext, AppError> {
    let (title, raw_message) = load_commit_message_fields(sha)?;
    let body = body_from_raw_message(&raw_message);

    let diff_raw = run_git_capture(&["show", "--format=", "--no-color", sha])?;
    let diff_files_joined =
        run_git_capture(&["diff-tree", "--no-commit-id", "--name-only", "-r", sha])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    let diff_lines_joined = collect_diff_lines(&diff_raw);

    let numstat = run_git_capture(&["show", "--numstat", "--format=", sha])?;
    let (line_count, file_count) = parse_numstat_totals(&numstat)?;

    let branches_joined =
        run_git_capture(&["branch", "--contains", sha, "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

    Ok(CommitContext {
        raw_message,
        title,
        body,
        diff_raw,
        diff_files_joined,
        diff_lines_joined,
        line_count,
        file_count,
        branches_joined,
    })
}

fn matches_any_regex(patterns: &[String], haystack: &str) -> Result<bool, AppError> {
    for pattern in patterns {
        let regex = Regex::new(pattern).map_err(|error| {
            AppError::Message(format!("invalid regex pattern '{pattern}': {error}"))
        })?;
        if regex.is_match(haystack) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn evaluate_condition(
    condition: &config::Condition,
    commit: &CommitContext,
) -> Result<bool, AppError> {
    match condition {
        config::Condition::MsgMatchAny(cond) => {
            let haystack = match cond.mode {
                config::MsgMode::Raw => commit.raw_message.as_str(),
                config::MsgMode::Title => commit.title.as_str(),
                config::MsgMode::Body => commit.body.as_str(),
            };
            matches_any_regex(&cond.patterns, haystack)
        }
        config::Condition::MsgMatchNone(cond) => {
            let haystack = match cond.mode {
                config::MsgMode::Raw => commit.raw_message.as_str(),
                config::MsgMode::Title => commit.title.as_str(),
                config::MsgMode::Body => commit.body.as_str(),
            };
            matches_any_regex(&cond.patterns, haystack).map(|matched| !matched)
        }
        config::Condition::DiffMatchAny(cond) => {
            let haystack = match cond.mode {
                config::DiffMode::Raw => commit.diff_raw.as_str(),
                config::DiffMode::File => commit.diff_files_joined.as_str(),
                config::DiffMode::Line => commit.diff_lines_joined.as_str(),
            };
            matches_any_regex(&cond.patterns, haystack)
        }
        config::Condition::DiffMatchNone(cond) => {
            let haystack = match cond.mode {
                config::DiffMode::Raw => commit.diff_raw.as_str(),
                config::DiffMode::File => commit.diff_files_joined.as_str(),
                config::DiffMode::Line => commit.diff_lines_joined.as_str(),
            };
            matches_any_regex(&cond.patterns, haystack).map(|matched| !matched)
        }
        config::Condition::BranchMatch(cond) => {
            matches_any_regex(&cond.patterns, commit.branches_joined.as_str())
        }
        config::Condition::ThresholdCompare(cond) => {
            let actual = match cond.metric {
                config::ThresholdMetric::LineCount => commit.line_count,
                config::ThresholdMetric::FileCount => commit.file_count,
            };

            Ok(match cond.operator {
                config::ThresholdOperator::Lte => actual <= cond.value,
                config::ThresholdOperator::Gte => actual >= cond.value,
            })
        }
    }
}

fn assertion_violated(
    assertion: &config::Assertion,
    commit: &CommitContext,
) -> Result<bool, AppError> {
    if assertion.skip {
        return Ok(false);
    }

    if let Some(skip_if) = &assertion.skip_if
        && evaluate_condition(&skip_if.condition, commit)?
    {
        return Ok(false);
    }

    evaluate_condition(&assertion.must_satisfy.condition, commit).map(|passed| !passed)
}

pub fn collect_violations(
    scope: &LintScope,
    assertions: &[config::Assertion],
    history: &config::History,
    verbose: u8,
) -> Result<LintResult, AppError> {
    if assertions.is_empty() {
        return Ok(LintResult {
            commits_checked: Vec::new(),
            violations: Vec::new(),
        });
    }

    let commits_checked = match scope {
        LintScope::CommitSha(sha) => vec![sha.clone()],
        LintScope::RefRange {
            source_ref,
            target_ref,
        } => resolve_ref_range_commit_shas(source_ref, target_ref, history, verbose)?,
    };

    if commits_checked.is_empty() {
        return Ok(LintResult {
            commits_checked,
            violations: Vec::new(),
        });
    }

    let mut violations = Vec::new();
    for sha in &commits_checked {
        let context = load_commit_context(sha)?;
        for assertion in assertions {
            if assertion_violated(assertion, &context)? {
                violations.push(Violation {
                    commit_sha: sha.clone(),
                    commit_title: context.title.clone(),
                    assertion_alias: assertion.alias.clone(),
                    assertion_description: assertion.description.clone(),
                    severity: assertion.severity,
                    banner: assertion.banner.clone(),
                    hint: assertion.hint.clone(),
                });
            }
        }
    }

    Ok(LintResult {
        commits_checked,
        violations,
    })
}

#[cfg(test)]
mod tests {
    use super::incremental_deepen_step;

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
}
