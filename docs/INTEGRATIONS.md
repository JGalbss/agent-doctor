# Integrations — Claude Code plugin, git hooks, CI

## Claude Code plugin (recommended)

The repo is also a Claude Code **plugin marketplace**. Installing the plugin ships the
**agent-doctor skill**, so a coding agent runs the linter on its own TypeScript before
committing:

```
/plugin marketplace add JGalbss/agent-doctor
/plugin install agent-doctor@agent-doctor
```

What you get (source: `plugins/agent-doctor/`):
- **Skill** — teaches the agent to run `agent-doctor` on changed files and respect the
  findings (especially under `--agent-strict`).

Install the binary the skill shells out to: `npm i -g @jgalbsss/agent-doctor`.
Validate the plugin locally with `claude plugin validate ./plugins/agent-doctor`.

---

## Git pre-commit / pre-push hook

The linter is a fast, deterministic check, so it slots into a standard git hook. Add a
`.git/hooks/pre-push` (or pre-commit):

```sh
#!/bin/sh
# Fail the push if changed TypeScript trips an error-level (or --agent-strict) rule.
exec agent-doctor --scope changed --base main --agent-strict
```

`--agent-strict` escalates the agent-slop rules to errors so the command exits non-zero;
drop it to gate on only the core error-level rules.

## CI — the same check as a GitHub Action

Run the linter server-side on every PR:

```yaml
# .github/workflows/lint.yml
name: agent-doctor
on: pull_request
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }          # need history for the diff base
      - run: npm i -g @jgalbsss/agent-doctor
      - run: agent-doctor --scope changed --base "origin/${{ github.base_ref }}" --agent-strict
```

## What the linter is (and isn't)

It's a deterministic **fact check** — rule violations with file locations and a 0–100 health
score — not a freeform style opinion. Humans and agents pass through the identical bar.
