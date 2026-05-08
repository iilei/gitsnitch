---
layout: default
title: GitSnitch
---

[![codecov](https://codecov.io/gh/iilei/gitsnitch/branch/master/graph/badge.svg?token=TZ71OWC0AZ)](https://codecov.io/gh/iilei/gitsnitch)
[![GitHub](https://img.shields.io/badge/GitHub-iilei%2Fgitsnitch-blue?logo=github)](https://github.com/iilei/gitsnitch)

# gitsnitch 🗡️🦆

![duck with a knife](https://cdn.jsdelivr.net/gh/iilei/gitsnitch@master/gitsnitch_banner.png)

**Lints your Git commit history against a declarative ruleset** - locally as a pre-commit/pre-push hook, or in any CI/CD pipeline.

Think of it as a linter, but for commit hygiene - enforced consistently across every author and every environment.

Source and issue tracker: [github.com/iilei/gitsnitch](https://github.com/iilei/gitsnitch)

---

## Quick start with pre-commit

gitsnitch ships with [pre-commit](https://pre-commit.com) hooks out of the box. Add this to your `.pre-commit-config.yaml`:

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/iilei/gitsnitch
    rev: v0.3.3  # pin to a tag or SHA in production
    args:
      - --preset
      - conventional-commits
    hooks:
      - id: gitsnitch-commit-msg  # lints the staged commit message at commit time
```

Three hooks are available:

| Hook id | Stage | Purpose |
| --- | --- | --- |
| `gitsnitch` | `pre-push` | Lints the full commit range being pushed |
| `gitsnitch-commit-msg` | `commit-msg` | Lints the staged commit message and index diff at commit time |
| `gitsnitch-single-commit`&nbsp;&nbsp; | `manual` | Lints a single commit; supply `--commit-sha` via `args` |

Requires **pre-commit ≥ 4.0.0**. The hooks are implemented in Rust (`language: rust`) and compiled once on first use — no separate install needed.

### Inspecting the highest severity encountered

Every JSON report includes `violation_severity_max_encountered` — the highest severity value seen across all violations, or `0` when none are found. Useful for threshold checks in scripts:

```bash
gitsnitch \
  --preset forbid-wip --preset conventional-commits \
  --target-ref HEAD^^^ --source-ref HEAD \
  --output-format json \
  | jq '.violation_severity_max_encountered'
```

---

If you prefer to install the binary directly, see the options below.

---

## Installation

Pre-built binaries are available for macOS, Linux, and Windows.

### Homebrew (macOS / Linux)

```bash
brew tap iilei/tap
brew install --formula iilei/tap/gitsnitch
```

### cargo-binstall

```bash
cargo binstall gitsnitch
```

### NuGet / Chocolatey (Windows)

```powershell
choco install iilei.gitsnitch
```

### Binary from releases

Download the latest archive from the [releases page](https://github.com/iilei/gitsnitch/releases), extract the binary, and place it on your `PATH`.

### Locally installed binary with pre-commit

Once installed, you can invoke `gitsnitch` directly in CI/CD pipelines, or wire it into pre-commit as a **local hook** using `language: unsupported` (pre-commit ≥ 4.4.0, formerly `language: system`). This skips compilation entirely and delegates to the binary already on your `PATH`:

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
        # gitsnitch_preinstalled requires:
        #   - `gitsnitch` installed and available on `PATH`
        #     -- see https://github.com/iilei/gitsnitch/#installation
        #   - a discoverable GitSnitch config file, for example `.gitsnitchrc`
        #     -- see https://github.com/iilei/gitsnitch/blob/master/.gitsnitchrc.toml
      - id: gitsnitch_preinstalled
        name: gitsnitch (pre-push / preinstalled)
        description: Lint the commit range being pushed.
        entry: |-
            gitsnitch --no-violation-severity-as-exit-code \
                --remap-env-var GITSNITCH_SOURCE_REF=PRE_COMMIT_TO_REF \
                --remap-env-var GITSNITCH_TARGET_REF=PRE_COMMIT_FROM_REF
        language: unsupported
        pass_filenames: false
        always_run: true
        stages: [pre-push]
        minimum_pre_commit_version: 4.4.0
        args:
          - --preset
          - conventional-commits
```

See the [pre-commit docs on `language: unsupported`](https://pre-commit.com/#unsupported) for details.

---

## Authoring custom assertions

Custom assertions are defined declaratively in the GitSnitch config API. In practice, that means you compose rules around commit message fields, diff content, file path patterns, and severity levels instead of writing hook logic by hand.

The diagram below gives a quick map of the config model. Click it to open a full-window view.

<!-- markdownlint-disable MD033 -->
<figure class="diagram-card">
  <a class="diagram-link" href="#config-api-diagram-overlay" aria-label="Open the GitSnitch config API diagram in a full-window view">
    <img
      src="https://raw.githubusercontent.com/iilei/gitsnitch/refs/heads/master/docs/api_design/GitSnitch%20Config%20Api.svg"
      alt="GitSnitch config API diagram for authoring custom assertions"
      loading="lazy"
    />
  </a>
  <figcaption>
    Config model overview for custom assertion authoring.
  </figcaption>
</figure>

<div id="config-api-diagram-overlay" class="diagram-overlay" aria-labelledby="config-api-diagram-title">
  <a class="diagram-overlay-backdrop" href="#authoring-custom-assertions" aria-label="Close full-window diagram view"></a>
  <div class="diagram-overlay-panel">
    <div class="diagram-overlay-header">
      <a class="diagram-overlay-close" href="#authoring-custom-assertions" aria-label="Close full-window diagram view">Close</a>
    </div>
    <a class="diagram-overlay-image-link" href="https://raw.githubusercontent.com/iilei/gitsnitch/refs/heads/master/docs/api_design/GitSnitch%20Config%20Api.svg" target="_blank" rel="noreferrer">
      <img
        src="https://raw.githubusercontent.com/iilei/gitsnitch/refs/heads/master/docs/api_design/GitSnitch%20Config%20Api.svg"
        alt="Full-window GitSnitch config API diagram"
      />
    </a>
  </div>
</div>
<!-- markdownlint-enable MD033 -->
