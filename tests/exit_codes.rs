use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, String> {
        let mut path = std::env::temp_dir();
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("failed to compute unix timestamp: {error}"))?;
        let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "{prefix}-{}-{}-{sequence}",
            std::process::id(),
            since_epoch.as_nanos()
        ));

        fs::create_dir_all(&path).map_err(|error| {
            format!(
                "failed to create temp directory '{}': {error}",
                path.display()
            )
        })?;

        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run git {args:?}: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_git_commit(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = vec!["-c", "commit.gpgsign=false", "commit"];
    command.extend_from_slice(args);
    run_git(repo, &command)
}

fn run_gitsnitch(cwd: &Path, args: &[&str]) -> Result<i32, String> {
    let bin = std::env::var("CARGO_BIN_EXE_gitsnitch")
        .map_err(|error| format!("missing CARGO_BIN_EXE_gitsnitch env var: {error}"))?;

    let status = Command::new(bin)
        .current_dir(cwd)
        .args(args)
        .status()
        .map_err(|error| format!("failed to run gitsnitch: {error}"))?;

    status
        .code()
        .ok_or_else(|| "gitsnitch terminated without an exit code".to_owned())
}

fn run_gitsnitch_with_env_and_output(
    cwd: &Path,
    args: &[&str],
    env_pairs: &[(&str, &str)],
) -> Result<(i32, String, String), String> {
    let bin = std::env::var("CARGO_BIN_EXE_gitsnitch")
        .map_err(|error| format!("missing CARGO_BIN_EXE_gitsnitch env var: {error}"))?;

    let mut command = Command::new(bin);
    command.current_dir(cwd).args(args);
    for (key, value) in env_pairs {
        command.env(key, value);
    }

    let output = command
        .output()
        .map_err(|error| format!("failed to run gitsnitch: {error}"))?;

    let code = output
        .status
        .code()
        .ok_or_else(|| "gitsnitch terminated without an exit code".to_owned())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((code, stdout, stderr))
}

fn init_repo_with_single_commit() -> Result<(TempDir, String), String> {
    let temp = TempDir::new("gitsnitch-it")?;

    run_git(&temp.path, &["init"])?;
    run_git(&temp.path, &["config", "user.name", "Test User"])?;
    run_git(&temp.path, &["config", "user.email", "test@example.com"])?;

    let file_path = temp.path.join("README.md");
    fs::write(&file_path, "seed\n")
        .map_err(|error| format!("failed to write '{}': {error}", file_path.display()))?;

    run_git(&temp.path, &["add", "README.md"])?;
    run_git_commit(&temp.path, &["-m", "feat: seed", "-m", "body text"])?;

    let sha = run_git(&temp.path, &["rev-parse", "HEAD"])?;
    Ok((temp, sha))
}

fn write_config(repo: &Path, violation_mode: bool, severities: &[u8]) -> Result<PathBuf, String> {
    let mut content = format!(
        "api_version = \"pre\"\nviolation_severity_as_exit_code = {}\n\n",
        if violation_mode { "true" } else { "false" }
    );

    for (idx, severity) in severities.iter().enumerate() {
        content.push_str("[[assertions]]\n");
        if writeln!(content, "alias = \"fail_{idx}\"").is_err() {
            return Err("failed to write alias into test config".to_owned());
        }
        if writeln!(content, "severity = {severity}").is_err() {
            return Err("failed to write severity into test config".to_owned());
        }
        content.push_str("[assertions.must_satisfy]\n");
        content.push_str("[assertions.must_satisfy.condition]\n");
        content.push_str("type = \"msg_match_any\"\n");
        content.push_str("mode = \"raw\"\n");
        content.push_str("patterns = [\"^THIS_PATTERN_NEVER_MATCHES$\"]\n\n");
    }

    let config_path = repo.join("test-config.toml");
    fs::write(&config_path, content)
        .map_err(|error| format!("failed to write '{}': {error}", config_path.display()))?;

    Ok(config_path)
}

fn write_config_without_assertions(repo: &Path, violation_mode: bool) -> Result<PathBuf, String> {
    let content = format!(
        "api_version = \"pre\"\nviolation_severity_as_exit_code = {}\n",
        if violation_mode { "true" } else { "false" }
    );

    let config_path = repo.join("test-config-no-assertions.toml");
    fs::write(&config_path, content)
        .map_err(|error| format!("failed to write '{}': {error}", config_path.display()))?;

    Ok(config_path)
}

fn write_config_with_alias(repo: &Path, alias: &str) -> Result<PathBuf, String> {
    let content = format!(
        "api_version = \"pre\"\n\n[[assertions]]\nalias = \"{alias}\"\nseverity = 10\n[assertions.must_satisfy]\n[assertions.must_satisfy.condition]\ntype = \"msg_match_any\"\nmode = \"raw\"\npatterns = [\"^THIS_PATTERN_NEVER_MATCHES$\"]\n"
    );

    let config_path = repo.join("test-config-with-alias.toml");
    fs::write(&config_path, content)
        .map_err(|error| format!("failed to write '{}': {error}", config_path.display()))?;

    Ok(config_path)
}

fn write_history_config(
    repo: &Path,
    autoheal_shallow: &str,
    autoheal_shallow_shift: u32,
    autoheal_shallow_tries: u32,
) -> Result<PathBuf, String> {
    let content = format!(
        "api_version = \"pre\"\n\n[history]\nautoheal_shallow = \"{autoheal_shallow}\"\nautoheal_shallow_shift = {autoheal_shallow_shift}\nautoheal_shallow_tries = {autoheal_shallow_tries}\n\n[[assertions]]\nalias = \"history-test\"\nseverity = 10\n[assertions.must_satisfy]\n[assertions.must_satisfy.condition]\ntype = \"msg_match_any\"\nmode = \"raw\"\npatterns = [\"^THIS_PATTERN_NEVER_MATCHES$\"]\n"
    );

    let config_path = repo.join("history-config.toml");
    fs::write(&config_path, content)
        .map_err(|error| format!("failed to write '{}': {error}", config_path.display()))?;

    Ok(config_path)
}

fn init_repo_with_linear_history(commit_count: usize) -> Result<TempDir, String> {
    let temp = TempDir::new("gitsnitch-history")?;

    run_git(&temp.path, &["init"])?;
    run_git(&temp.path, &["config", "user.name", "Test User"])?;
    run_git(&temp.path, &["config", "user.email", "test@example.com"])?;

    if commit_count < 2 {
        return Err("commit_count must be at least 2".to_owned());
    }

    for index in 1..=commit_count {
        let file_name = format!("file-{index}.txt");
        let file_path = temp.path.join(&file_name);
        fs::write(&file_path, format!("line {index}\n"))
            .map_err(|error| format!("failed to write '{}': {error}", file_path.display()))?;
        run_git(&temp.path, &["add", &file_name])?;
        run_git_commit(&temp.path, &["-m", &format!("feat: commit {index}")])?;

        if index == 1 {
            run_git(&temp.path, &["branch", "base"])?;
        }
    }

    run_git(&temp.path, &["branch", "feature"])?;

    Ok(temp)
}

fn clone_shallow_repo(source: &Path) -> Result<TempDir, String> {
    let clone_temp = TempDir::new("gitsnitch-shallow")?;
    let source_str = source.to_str().ok_or_else(|| {
        format!(
            "invalid source path '{}': not valid UTF-8",
            source.display()
        )
    })?;
    run_git(&clone_temp.path, &["init"])?;
    run_git(&clone_temp.path, &["remote", "add", "origin", source_str])?;
    run_git(
        &clone_temp.path,
        &["fetch", "--depth", "1", "origin", "master"],
    )?;
    run_git(
        &clone_temp.path,
        &["fetch", "--depth", "1", "origin", "feature"],
    )?;
    run_git(
        &clone_temp.path,
        &["checkout", "-b", "master", "FETCH_HEAD"],
    )?;

    let is_shallow = run_git(&clone_temp.path, &["rev-parse", "--is-shallow-repository"])?;
    if is_shallow != "true" {
        return Err("clone is not shallow as expected".to_owned());
    }

    Ok(clone_temp)
}

fn break_origin_remote(repo: &Path) -> Result<(), String> {
    run_git(
        repo,
        &[
            "remote",
            "set-url",
            "origin",
            "/tmp/gitsnitch-missing-remote",
        ],
    )?;
    Ok(())
}

#[test]
fn violations_are_exit_silent_when_mode_is_disabled() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config(&repo.path, false, &[200]);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &["--config", &cfg_path_str, "--commit-sha", &sha],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 0);
}

#[test]
fn violations_return_max_severity_when_mode_is_enabled() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config(&repo.path, true, &[100, 200]);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &["--config", &cfg_path_str, "--commit-sha", &sha],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 200);
}

#[test]
fn mode_enabled_with_only_zero_severities_returns_zero() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config(&repo.path, true, &[0]);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &["--config", &cfg_path_str, "--commit-sha", &sha],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 0);
}

#[test]
fn non_repo_failures_use_reserved_internal_exit_range() {
    let temp = TempDir::new("gitsnitch-nonrepo");
    assert!(temp.is_ok());
    let Ok(temp_dir) = temp else {
        return;
    };

    let exit = run_gitsnitch(&temp_dir.path, &["--commit-sha", "deadbeef"]);
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert!((251..=255).contains(&code));
}

#[test]
fn shallow_ref_range_runs_with_autoheal_never() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let cfg = write_history_config(&clone_repo.path, "never", 1, 3);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/feature",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 0);
}

#[test]
fn non_shallow_ref_range_full_autoheal_returns_internal_error_without_fetching() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(repo) = setup else {
        return;
    };

    let cfg = write_history_config(&repo.path, "full", 1, 1);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/does-not-exist",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn non_shallow_ref_range_incremental_autoheal_returns_internal_error_without_fetching() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(repo) = setup else {
        return;
    };

    let cfg = write_history_config(&repo.path, "incremental", 1, 2);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/does-not-exist",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn shallow_ref_range_succeeds_with_incremental_autoheal() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let cfg = write_history_config(&clone_repo.path, "incremental", 1, 3);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/feature",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 0);
}

#[test]
fn shallow_ref_range_incremental_autoheal_reports_fetch_failure_with_internal_code() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let break_remote = break_origin_remote(&clone_repo.path);
    assert!(break_remote.is_ok());

    let cfg = write_history_config(&clone_repo.path, "incremental", 1, 2);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/base",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn shallow_ref_range_full_autoheal_reports_fetch_failure_with_internal_code() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let break_remote = break_origin_remote(&clone_repo.path);
    assert!(break_remote.is_ok());

    let cfg = write_history_config(&clone_repo.path, "full", 1, 1);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/base",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn shallow_ref_range_incremental_autoheal_reports_post_fetch_range_failure() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let cfg = write_history_config(&clone_repo.path, "incremental", 1, 2);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/does-not-exist",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn shallow_ref_range_incremental_autoheal_with_zero_tries_reports_internal_error() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let cfg = write_history_config(&clone_repo.path, "incremental", 1, 0);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/does-not-exist",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn shallow_ref_range_full_autoheal_reports_post_fetch_range_failure() {
    let setup = init_repo_with_linear_history(3);
    assert!(setup.is_ok());
    let Ok(source_repo) = setup else {
        return;
    };

    let shallow_clone = clone_shallow_repo(&source_repo.path);
    assert!(shallow_clone.is_ok());
    let Ok(clone_repo) = shallow_clone else {
        return;
    };

    let cfg = write_history_config(&clone_repo.path, "full", 1, 1);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &clone_repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--source-ref",
            "origin/does-not-exist",
            "--target-ref",
            "HEAD",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 251);
}

#[test]
fn missing_git_on_path_returns_dependency_exit_code() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, _sha)) = setup else {
        return;
    };

    let run = run_gitsnitch_with_env_and_output(
        &repo.path,
        &["--commit-sha", "deadbeef"],
        &[("PATH", "")],
    );
    assert!(run.is_ok());
    let Ok((code, _stdout, stderr)) = run else {
        return;
    };

    assert_eq!(code, 253);
    assert!(stderr.contains("git is not installed or not on PATH"));
}

#[test]
fn decorative_output_omits_ansi_when_no_color_is_set() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config(&repo.path, false, &[10]);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let run = run_gitsnitch_with_env_and_output(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--commit-sha",
            &sha,
            "--output-format",
            "text-decorative",
        ],
        &[("NO_COLOR", "1")],
    );
    assert!(run.is_ok());
    let Ok((code, stdout, _stderr)) = run else {
        return;
    };

    assert_eq!(code, 0);
    assert!(!stdout.contains("\u{1b}[38;5;208m"));
}

#[test]
fn decorative_output_emits_ansi_when_clicolor_force_is_enabled() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config(&repo.path, false, &[10]);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let run = run_gitsnitch_with_env_and_output(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--commit-sha",
            &sha,
            "--output-format",
            "text-decorative",
        ],
        &[("CLICOLOR_FORCE", "1")],
    );
    assert!(run.is_ok());
    let Ok((code, stdout, _stderr)) = run else {
        return;
    };

    assert_eq!(code, 0);
    assert!(stdout.contains("\u{1b}[38;5;208m"));
}

#[test]
fn selecting_preset_extends_assertions_and_can_fail_with_severity_exit() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config_without_assertions(&repo.path, true);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--commit-sha",
            &sha,
            "--preset",
            "conventional-commits",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 0);
}

#[test]
fn unknown_preset_exits_with_internal_config_code() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config_without_assertions(&repo.path, false);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--commit-sha",
            &sha,
            "--preset",
            "unknown-preset",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 252);
}

#[test]
fn duplicate_alias_between_config_and_preset_exits_with_internal_config_code() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config_with_alias(&repo.path, "preset_conventional_title");
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--commit-sha",
            &sha,
            "--preset",
            "conventional-commits",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 252);
}

#[test]
fn duplicate_alias_between_selected_presets_exits_with_internal_config_code() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let cfg = write_config_without_assertions(&repo.path, false);
    assert!(cfg.is_ok());
    let Ok(cfg_path) = cfg else {
        return;
    };

    let cfg_path_str = cfg_path.to_string_lossy().to_string();
    let exit = run_gitsnitch(
        &repo.path,
        &[
            "--config",
            &cfg_path_str,
            "--commit-sha",
            &sha,
            "--preset",
            "conventional-commits",
            "--preset",
            "conventional-commits",
        ],
    );
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 252);
}

#[test]
fn no_config_and_no_presets_exits_with_internal_config_code() {
    let setup = init_repo_with_single_commit();
    assert!(setup.is_ok());
    let Ok((repo, sha)) = setup else {
        return;
    };

    let exit = run_gitsnitch(&repo.path, &["--commit-sha", &sha]);
    assert!(exit.is_ok());
    let Ok(code) = exit else {
        return;
    };

    assert_eq!(code, 252);
}
