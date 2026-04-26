[![codecov](https://codecov.io/gh/iilei/gitsnitch/branch/master/graph/badge.svg?token=TZ71OWC0AZ)](https://codecov.io/gh/iilei/gitsnitch)

# gitsnitch 🗡️🦆

![duck with a knife](gitsnitch_banner.png)

**Lints your Git commit history against a declarative ruleset** — locally as a pre-commit/pre-push hook, or in any CI/CD pipeline.

Think of it as a linter, but for commit hygiene — enforced consistently across every author and every environment.

---

## Installation

### Homebrew (macOS / Linux)

```sh
brew tap iilei/tap
brew install --formula iilei/tap/gitsnitch
```

### cargo-binstall

```sh
cargo binstall gitsnitch
```

### NuGet / Chocolatey (Windows)

```powershell
choco install iilei.gitsnitch
```

Or via `dotnet`:

```sh
dotnet tool install --global iilei.gitsnitch
```

### Verifying release signatures

All release binaries are GPG-signed. The `.sig` file covers the extracted binary, not the archive.
Archive names use a friendly `OS-arch` scheme; signature names use the Rust target triple.

**macOS / Linux**

```sh
curl -sSL https://iilei.github.io/pubkey-0F50EA12D4E2AB1D.asc | gpg --import

# example: Linux x86_64
tar xzf gitsnitch-Linux-musl-x86_64.tar.gz
gpg --verify gitsnitch-x86_64-unknown-linux-musl.sig gitsnitch
```

**Windows** (PowerShell, requires [Gpg4win](https://www.gpg4win.org/) or Git for Windows)

```powershell
Invoke-WebRequest https://iilei.github.io/pubkey-0F50EA12D4E2AB1D.asc -OutFile pubkey.asc
gpg --import pubkey.asc

# example: Windows x86_64
Expand-Archive gitsnitch-Windows-msvc-x86_64.zip -DestinationPath .
gpg --verify gitsnitch-x86_64-pc-windows-msvc.sig gitsnitch.exe
```

Public key fingerprint: `E298 44DB 66D4 7846 A802  81D6 0F50 EA12 D4E2 AB1D`

([download public key](https://iilei.github.io/pubkey-0F50EA12D4E2AB1D.asc))

---

## Quick Start

### For developers (local linting)

Lint a single commit while iterating:

```sh
gitsnitch --commit-sha <sha>
```

With a preset bundle (e.g., enforce conventional commits):

```sh
gitsnitch --preset conventional-commits --commit-sha <sha>
```

### For CI/CD pipelines

Lint a range of commits in a pull request:

```sh
gitsnitch \
	--source-ref "$GITHUB_HEAD_REF" \
	--target-ref "origin/${GITHUB_BASE_REF}" \
	--config .gitsnitch.toml \
	--violation-severity-as-exit-code
```

Or lint via commit SHA with JSON output:

```sh
gitsnitch \
	--commit-sha "$CI_COMMIT_SHA" \
	--config .gitsnitch.toml \
	--output-format json-compact
```

---

## Built-in Presets

Apply assertion bundles with `--preset` flags (repeatable):

* `conventional-commits` — enforce [Conventional Commits](https://www.conventionalcommits.org/)
* `title-body-separator` — require blank line between title and body
* `forbid-wip` — block WIP/DO NOT MERGE patterns
* `security-related-edits-mention` — require explicit mention of security in certain commit types

**Examples:**

```sh
gitsnitch --preset conventional-commits --preset forbid-wip --commit-sha <sha>
```

---

## Core Features

* **Message rules** — regex patterns on commit title, body, or full message
* **Diff rules** — restrict file paths, detect forbidden line patterns, enforce line-count thresholds
* **Context-aware skipping** — skip rules conditionally (e.g., on maintenance branches)
* **Severity bands** — map severity 0–250 to `Information`, `Warning`, `Error`, `Fatal`
* **Exit code mapping** — optionally map violation severity to exit code for CI automation
* **Shallow clone healing** — auto-deepen shallow CI checkouts
* **Remediation hints** — customizable Jinja2 banner templates per rule
* **Config autodiscovery** — find `.gitsnitch.toml`, `.gitsnitchrc`, `.gitsnitch.json`, etc., automatically

---

<details>
<summary><strong>Creating Custom Presets</strong></summary>

Presets provide assertion bundles (with optional assertion-level `banner` and `hint` templates) selected at runtime via CLI only.

**Rules:**

* Presets contain assertions only — no root-level `history`, `severity_bands`, or global switches
* Embedded at build-time from snake_case files
* Runtime names use dash-case (e.g., `conventional-commits`)
* Selected presets append to config assertions
* Assertion aliases must be globally unique; duplicates fail as a config error

**Authoring custom presets:**

Use the embedded preset files as templates:

* [src/presets_data/conventional_commits.toml](src/presets_data/conventional_commits.toml)
* [src/presets_data/title_body_separator.toml](src/presets_data/title_body_separator.toml)
* [src/presets_data/forbid_wip.toml](src/presets_data/forbid_wip.toml)
* [src/presets_data/security_related_edits_mention.toml](src/presets_data/security_related_edits_mention.toml)

Copy and adapt assertion blocks into your shared config file for project-local customization.

</details>

---

<details>
<summary><strong>Configuration & Input Modes</strong></summary>

### Choosing a lint scope

gitsnitch requires exactly one mode:

1. `--commit-sha <sha>` — lint a single commit
2. `--source-ref <ref> --target-ref <ref>` — lint a range between two refs

These are mutually exclusive. If neither is provided, gitsnitch fails with an explicit error.

### Config file autodiscovery

If no `--config` flag is given, gitsnitch searches the git repository root for config files in this order:

1. `.gitsnitch.toml`
2. `.gitsnitchrc` (TOML format, no extension)
3. `.gitsnitch.json`
4. `.gitsnitch.json5`
5. `.gitsnitch.yaml`
6. `.gitsnitch.yml`

The first match wins. If none is found, gitsnitch runs with no config (no assertions).

**Explicit config path:**

```sh
gitsnitch --config path/to/config.toml
```

**Read from stdin:**

```sh
cat my-config.toml | gitsnitch --config -
```

### Config discovery root

By default, discovery searches the git repository root. Override it with:

```sh
GITSNITCH_CONFIG_ROOT=/path/to/config/dir gitsnitch
```

Or via flag:

```sh
gitsnitch --env-prefix CI_
# now reads CI_CONFIG_ROOT instead of GITSNITCH_CONFIG_ROOT
```

Custom namespace:

```sh
gitsnitch --env-prefix GITSNITCH_CUSTOM_NAMESPACE_
# reads GITSNITCH_CUSTOM_NAMESPACE_CONFIG_ROOT, etc.
```

</details>

---

<details>
<summary><strong>Runtime Inputs, Precedence & Environment Variables</strong></summary>

### precedence (CLI → env vars → config → defaults)

1. CLI flags (highest priority)
2. Environment variables (supported runtime keys only)
3. Config file values
4. Built-in defaults

### Supported environment variables

Canonical keys (default prefix `GITSNITCH_`):

* `GITSNITCH_CONFIG_ROOT` — where to search for config file
* `GITSNITCH_COMMIT_SHA` — commit to lint
* `GITSNITCH_SOURCE_REF` — source branch (for range linting)
* `GITSNITCH_TARGET_REF` — target branch (for range linting)

**Change the prefix:**

```sh
gitsnitch --env-prefix CI_
# reads CI_CONFIG_ROOT, CI_COMMIT_SHA, CI_SOURCE_REF, CI_TARGET_REF
```

**Remap to arbitrary env var names:**

```sh
gitsnitch \
	--remap-env-var GITSNITCH_SOURCE_REF=PRE_COMMIT_TO_REF \
	--remap-env-var GITSNITCH_TARGET_REF=PRE_COMMIT_FROM_REF
```

**Remap rules:**

* Format: `KEY=ENV_VAR`
* `ENV_VAR` must be non-empty
* A key can only be remapped once
* For a remapped key, only the remapped env var is read (no fallback)
* `--remap-env-var` is mutually exclusive with non-default `--env-prefix`

</details>

---

<details>
<summary><strong>Exit Codes & Output Formats</strong></summary>

### Exit code behavior

gitsnitch reserves exit codes `251..255` for internal/runtime failures.

Violation exit behavior is controlled by `violation_severity_as_exit_code`:

* `false` (default): violations are reported, exit is `0`
* `true`: exit code is the maximum violating assertion severity (0–250)

**Examples:**

* violations `{100, 200}` with mode `true` → exit `200`
* violations `{0, 0}` with mode `true` → exit `0`
* any violations with mode `false` → exit `0`

**CLI override:**

```sh
gitsnitch --violation-severity-as-exit-code ...       # enable
gitsnitch --no-violation-severity-as-exit-code ...    # disable (overrides config)
```

**Precedence:**

1. CLI flag (if provided)
2. config `violation_severity_as_exit_code`
3. default `false`

### Output formats

By default, `gitsnitch` renders pretty JSON:

```sh
gitsnitch --output-format json ...
```

Compact single-line JSON:

```sh
gitsnitch --output-format json-compact ...
```

Human-friendly text:

```sh
gitsnitch --output-format text-plain ...
```

Decorative terminal text:

```sh
gitsnitch --output-format text-decorative ...
```

Additionally write a JSON artifact to a file (without changing terminal output style):

```sh
gitsnitch --output-format text-decorative --gitsnitch-json report.json ...
```

`--gitsnitch-json` requires a real file path and does not accept `-`.

**Hint:** `text-decorative` prints ANSI colors only on compatible TTYs.
On Windows terminals, `ConEmuANSI=ON` can mark compatibility.
Safety overrides still win: `NO_COLOR`, `TERM=dumb`, non-TTY stdout, and `CLICOLOR=0` disable color.
For machine-readable output, prefer `json` or `json-compact`.
</details>

---

<details>
<summary><strong>CI Authentication & Shallow Clone Autoheal</strong></summary>

When linting a ref range in a shallow checkout, `gitsnitch` may run `git fetch` to deepen history.

**Requirement:**

CI credentials must allow `git fetch` from `origin`.

**Common setups:**

* CI-native checkout token persisted for later fetches
* Git credential helper configured in the runner
* Optional `.netrc` file

**Example `.netrc`:**

```text
machine github.com
	login x-access-token
	password ${GITHUB_TOKEN}
```

Without credentials, shallow autoheal fetches fail and gitsnitch returns an internal/runtime error (`251..255`).

</details>


## Contributing

```sh
make install-tools
```

### Code-Quality

One-off run prek (1)

```sh
prek --stage manual --all-files
```

Install pre-commit, pre-push, and post-commit hooks

```sh
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
