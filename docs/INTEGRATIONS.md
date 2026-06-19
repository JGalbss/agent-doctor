# Integrations — Claude Code plugin, Graphite, git hooks, CI

## Claude Code plugin (recommended)

The repo is also a Claude Code **plugin marketplace**. Installing the plugin wires up the
guardrails at once — skill, slash commands, and a commit-gate hook:

```
/plugin marketplace add JGalbss/agent-doctor
/plugin install agent-doctor@agent-doctor
```

What you get (source: `plugins/agent-doctor/`):
- **Skill** — teaches the agent to use `verify`/`gate`/`merge` in your repo.
- **Slash commands** — `/agent-doctor:verify`, `/agent-doctor:gate`, `/agent-doctor:merge`.
- **Commit-gate hook** — a `PreToolUse(Bash)` hook that runs `agent-doctor verify` whenever
  the agent runs `git commit` / `git push` / `gt submit`, and **blocks** it on a real failure
  (policy/lease violation or failing tests). It no-ops if the toolkit isn't installed and
  never blocks for setup reasons — only genuine, deterministic failures.

Install the binary the hook shells out to: `npm i -g @jgalbsss/agent-doctor`.
Validate the plugin locally with `claude plugin validate ./plugins/agent-doctor`.

---

# Integrations — Graphite, git hooks, CI

agent-doctor has no proprietary plugin to install into Graphite, and Graphite has no
plugin SDK to target. It doesn't need one: Graphite runs **standard git hooks**, and
`gt submit` pushes — so a **pre-push hook fires automatically on submit**. The single
command that ties it together is:

```sh
agent-doctor verify   # gate the diff, then optionally run your tests
```

`verify` exits non-zero if the diff violates policy/ACL/leases, or (with `--run`) if your
tests fail.

## Graphite (`gt`) — verify on every submit

Install the hook once (also done by `agent-doctor init --hooks`):

```sh
agent-doctor init --hooks
```

This writes `.git/hooks/pre-push`:

```sh
#!/bin/sh
exec agent-doctor verify
```

Now `gt submit` (which pushes) runs `verify` first. If the gate fails or the tests
fail, the submit is blocked — locally, in seconds, before CI ever runs.

Run your tests too (not just gate):

```sh
# in .git/hooks/pre-push
exec agent-doctor verify --run "npx vitest run"
```

Your test command runs as given (it can scope itself, e.g. `--changed`).

Notes:
- `gt submit --no-verify` bypasses hooks (Graphite honors the standard flag) — for the
  rare escape hatch.
- Graphite respects per-repo hooks; nothing Graphite-specific is required.

## CI — the same check as a GitHub Action

Graphite surfaces GitHub checks in its UI, so run `verify` server-side too. This replaces
"submit and wait" with a fast gate before merge:

```yaml
# .github/workflows/verify.yml
name: verify
on: pull_request
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }          # need history for the diff base
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npm i -g @jgalbsss/agent-doctor
      - run: agent-doctor verify --base "origin/${{ github.base_ref }}" --run "npx vitest run"
```

## Wrapper alias (optional)

If you'd rather verify explicitly than via a hook:

```sh
# ~/.zshrc  — verify, then submit the stack
gship() { agent-doctor verify --run "npx vitest run" && gt submit "$@"; }
```

## What verify is (and isn't)

It's a deterministic **fact check** — policy/lease violations — not a style opinion. It runs the same `gate` the agents use, so
humans and agents pass through the identical bar.
