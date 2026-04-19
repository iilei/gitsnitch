# gitsnitch :dagger::goose:

![goose with a knife](gitsnitch_banner.png)

**gitsnitch** lints your Git commit history against a declarative ruleset — locally as a pre-commit/pre-push hook, or in any CI/CD pipeline.

You define *assertions*: each assertion has a condition that each commit must satisfy (or a condition under which to skip it entirely). Conditions can match against the commit message title, body, or raw text; inspect changed file paths or diff lines; or compare numeric metrics such as diff line count against a threshold. Violations are assigned a numeric severity, mapped to named bands (`Information`, `Warning`, `Error`, `Fatal`), and surfaced with configurable banners and remediation hints.

Think of it as a linter, but for commit hygiene — enforced consistently across every author and every environment.

### Features

* **Message rules** — require conventional-commit titles, ticket references in the body, or any regex pattern against title/body/raw message
* **Diff rules** — restrict changed file paths, detect forbidden line patterns, or gate on diff size (line count thresholds)
* **Context-aware skipping** — skip assertions conditionally, e.g. on maintenance branches
* **Severity bands** — map numeric severity (0–250) to `Information / Warning / Error / Fatal` with configurable thresholds
* **Severity-as-exit optional mode** — when enabled, exit with the maximum violation severity; internal/runtime errors are reserved to 251–255
* **Shallow clone healing** — automatically deepens shallow CI checkouts before linting
* **Remediation hints** — show actionable guidance on violation via Jinja2 banner templates
* **Named assertion presets** — select embedded preset assertion bundles with repeatable `--preset` flags

## Named Presets

Presets provide assertion bundles (including assertion-level `banner` and `hint` templates) and are selected at runtime via CLI only.

Rules:

* Presets do not carry root config (no `history`, no `severity_bands`, no global switches).
* Presets are embedded at build-time from snake_case preset files.
* Runtime selection uses dash-case names via repeatable `--preset` flags.
* Selected preset assertions are appended to config assertions.
* Assertion aliases must be globally unique across config + all selected presets; duplicates fail as a config error.

Available presets (CLI names):

* `conventional-commits`
* `title-body-seperator`
* `forbid-wip`
* `security-related-edits-mention`

Examples:

```sh
gitsnitch --preset conventional-commits --commit-sha <sha>
```

```sh
gitsnitch \
	--preset conventional-commits \
	--preset forbid-wip \
	--source-ref <source-ref> \
	--target-ref <target-ref>
```

### Self-authoring presets

Use the embedded preset files as inspiration:

* [src/presets_data/conventional_commits.toml](src/presets_data/conventional_commits.toml)
* [src/presets_data/title_body_seperator.toml](src/presets_data/title_body_seperator.toml)
* [src/presets_data/forbid_wip.toml](src/presets_data/forbid_wip.toml)
* [src/presets_data/security_related_edits_mention.toml](src/presets_data/security_related_edits_mention.toml)

Authoring guidance:

* Preset content is assertion-only (no root-level history, severity_bands, or global switches).
* For project-local customization, copy/adapt assertion blocks into your shared config file.

## Workflows by Role

### Developers (local)

1. Keep a shared repo config (`.gitsnitch.toml` or equivalent).
2. Lint a single commit while iterating:

```sh
gitsnitch --commit-sha <sha>
```

3. Reuse named presets when needed:

```sh
gitsnitch --preset conventional-commits --commit-sha <sha>
```

### CI/CD orchestrators

Use flags for policy and behavior; use env only to wire runtime context.

```sh
gitsnitch \
	--source-ref "$GITHUB_SHA" \
	--target-ref "origin/${GITHUB_BASE_REF}" \
	--config .gitsnitch.toml \
	--violation-severity-as-exit-code
```

```sh
gitsnitch \
	--source-ref "$CI_COMMIT_SHA" \
	--target-ref "origin/main" \
	--config .gitsnitch.toml \
	--render-output json-compact
```

### Policy designers

1. Define assertions and severity bands in config.
2. Optionally bundle reusable assertion sets as presets.
3. Keep policy stable in config and avoid environment-specific policy overrides.

## Runtime Inputs and Precedence

gitsnitch needs an explicit lint scope. Choose exactly one mode:

1. `--commit-sha <sha>`
2. `--source-ref <source-ref> --target-ref <target-ref>`

Rules:

* `--commit-sha` is mutually exclusive with `--source-ref` and `--target-ref`.
* `--source-ref` and `--target-ref` must be provided together.
* If none are provided, gitsnitch fails with an explicit error.

Global precedence model:

1. CLI flags
2. env vars (supported runtime context keys only)
3. config file
4. built-in defaults

### Environment variable scope

Supported canonical runtime keys:

* `GITSNITCH_CONFIG_ROOT`
* `GITSNITCH_COMMIT_SHA`
* `GITSNITCH_SOURCE_REF`
* `GITSNITCH_TARGET_REF`

Scope note:

* Env vars are for runtime context wiring only.
* Policy/config settings are expected to come from CLI flags or config file.

You can change the prefix with `--env-prefix`:

```sh
gitsnitch --env-prefix CI_
# reads CI_CONFIG_ROOT, CI_COMMIT_SHA, CI_SOURCE_REF, CI_TARGET_REF
```

You can also remap canonical keys to arbitrary env var names:

```sh
gitsnitch \
	--remap-env-var GITSNITCH_SOURCE_REF=PRE_COMMIT_TO_REF \
	--remap-env-var GITSNITCH_TARGET_REF=PRE_COMMIT_FROM_REF
```

Remap rules:

* Format must be `KEY=ENV_VAR`.
* `ENV_VAR` must be non-empty.
* A key can only be remapped once.
* For a remapped key, gitsnitch reads only the remapped env var (no fallback).
* `--remap-env-var` is mutually exclusive with non-default `--env-prefix`.

## Configuration

### Config file autodiscovery

When no `--config` flag is given, gitsnitch searches the git repository root for config files in this precedence order:

1. `.gitsnitch.toml`
2. `.gitsnitchrc` (no extension, parsed as TOML)
3. `.gitsnitch.json`
4. `.gitsnitch.json5`
5. `.gitsnitch.yaml`
6. `.gitsnitch.yml`

The first match wins. If none is found, gitsnitch runs with no config (no assertions).

### Overriding the discovery root

The discovery root defaults to the git repository root (`git rev-parse --show-toplevel`). Override it with an environment variable:

```sh
GITSNITCH_CONFIG_ROOT=/path/to/config/dir gitsnitch
```

The env var prefix defaults to `GITSNITCH_` and can be changed with `--env-prefix`:

```sh
gitsnitch --env-prefix CI_
# now reads CI_CONFIG_ROOT instead of GITSNITCH_CONFIG_ROOT
```

You can also use a project-specific namespace when preferred:

```sh
gitsnitch --env-prefix GITSNITCH_CUSTOM_NAMESPACE_
# reads GITSNITCH_CUSTOM_NAMESPACE_CONFIG_ROOT, GITSNITCH_CUSTOM_NAMESPACE_SOURCE_REF, ...
```

### Explicit config path

Pass an explicit file path to skip autodiscovery entirely:

```sh
gitsnitch --config path/to/config.toml
```

Pass `-` to read the config from stdin:

```sh
cat my-config.toml | gitsnitch --config -
```

### Exit code behavior

`gitsnitch` reserves process exit codes `251..255` for internal/runtime failures.

Violation exit behavior is controlled by `violation_severity_as_exit_code`:

* `false` (default): violations are reported but process exit remains `0`.
* `true`: process exit code is the maximum violating assertion severity (`0..250`).

Examples:

* violations with severities `{100, 200}` and mode `true` => exit `200`
* violations with severities `{0, 0}` and mode `true` => exit `0`
* any violations and mode `false` => exit `0`

CLI override:

```sh
gitsnitch --violation-severity-as-exit-code ...
```

Disable from CLI (even if config enables it):

```sh
gitsnitch --no-violation-severity-as-exit-code ...
```

Precedence:

1. CLI `--violation-severity-as-exit-code` or `--no-violation-severity-as-exit-code`
2. config `violation_severity_as_exit_code`
3. default `false`

### Output format

By default, `gitsnitch` renders pretty JSON:

```sh
gitsnitch --render-output json ...
```

Use compact single-line JSON when needed:

```sh
gitsnitch --render-output json-compact ...
```

Use the internal human-friendly text renderer:

```sh
gitsnitch --render-output text-plain ...
```

## CI Authentication for Shallow Autoheal

When linting a ref range in a shallow checkout, gitsnitch may run `git fetch` to deepen history.

Requirement:

* CI credentials must allow `git fetch` from `origin`.

Common setups:

* CI-native checkout token persisted for later fetches.
* Git credential helper configured in the runner.
* Optional `.netrc` in environments where that is preferred.

Example `.netrc`:

```text
machine github.com
	login x-access-token
	password ${GITHUB_TOKEN}
```

Without credentials, shallow autoheal fetches fail and gitsnitch returns an internal/runtime error (`251..255`).


## Contributing

```bash
make install-tools
```

### Code-Quality

One-off run prek (1)

```
prek --stage manual --all-files
```

Install pre-commit, pre-push, and post-commit hooks

```
prek install
prek install --hook-type pre-push
prek install --hook-type post-commit
```

Configured automation highlights:

* `pre-push`: runs quality/security hooks and `make maintenance`
* `post-commit`: runs `make generate-coverage`


---

1) [prek quote](https://prek.j178.dev/):
<blockquote>
prek is a reimagined version of pre-commit, built in Rust. It is designed to be a faster, dependency-free and drop-in alternative for it, while also providing some additional long-requested features.
</blockquote>
