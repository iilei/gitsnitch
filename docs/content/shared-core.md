# gitsnitch 🗡️🦆

![duck with a knife](https://cdn.jsdelivr.net/gh/iilei/gitsnitch@master/gitsnitch_banner.png)

> **Move from observation to enforcement — one exit code at a time.**

gitsnitch is a declarative policy engine for Git repositories.

It helps teams introduce and enforce repository standards incrementally—whether through local hooks, CI pipelines or GitOps workflows.

Unlike traditional Git linters, gitsnitch is designed around gradual adoption. Policies can begin as observations, evolve into warnings and eventually become enforced engineering standards without changing the workflow.

---

## Why?

Repository standards rarely emerge all at once.

Teams grow.

Repositories evolve.

New contributors join.

Automation gets introduced.

Eventually, organizations want to make certain conventions explicit—not because existing history is "wrong", but because consistent history becomes increasingly valuable over time.

gitsnitch provides a declarative way to express those conventions and integrate them into existing development workflows.

---

## Philosophy

GitSnitch is intentionally designed around:

- local developer ergonomics
- centrally enforceable policies
- machine-readable automation signals
- incremental adoption instead of hard lock-in

Or, put differently:

**"Commit often."** remains excellent advice.

You decide whether a rule should merely make the duck honk—or stop the pipeline.

---

## Progressive enforcement

Not every rule should immediately fail a build.

gitsnitch uses configurable exit codes, allowing teams to introduce policies gradually.

For example:

```yaml
exit_code_threshold: 10
```

can initially report findings without interrupting development.

As adoption matures, the threshold can be adjusted until policies become part of the normal engineering workflow.

**Move from observation to enforcement—one exit code at a time.**

---

## In other words &hellip;

- policy-as-code for commit history
- repository governance for Git
- engineering standards that travel with every repository

Not solely another linter.
