# agent-doctor — the agent toolkit

A small, deliberately bounded set of **guardrails for coding agents**, built on the same
Rust/oxc engine as the linter. The scope is chosen by one rule:

> **Keep only what a better model makes *more* useful, not less.** Anything a smarter agent
> would just do itself — gather context, plan, orchestrate, pick which tests to run — we don't
> build. (See the "bitter lesson" / Vercel's text-to-SQL: hand-built scaffolding around a model
> gets chalked up the moment the model improves.)

That leaves three things, each of which gets *more* valuable as you run more agents:

## 1. Gate — deterministic guardrails (`crates/policy`)
A diff is checked against `agent-doctor.policy.toml`: protected paths, architecture layering,
per-path ACLs, and **leases** (who owns which area right now). Violations are facts, and the
gate exits non-zero. `agent-doctor gate` / `agent-doctor verify`.

Why it survives: org policy and ownership aren't something a model outgrows. More agents →
more need to keep them out of each other's lanes.

## 2. Semantic merge — no conflict hell (`crates/merge`)
An AST-level 3-way merge for TypeScript. Two agents (or two tickets) adding different
functions to the same file merge with **zero conflict**; reordering and reformatting aren't
conflicts; only edits to the *same declaration* conflict. Drops in as a git merge driver.

Why it survives: git's line-merge is permanently dumb. Concurrent edits always need merging,
and the spurious-conflict problem grows with parallelism.

## 3. The linter — health checks (`crates/core`)
The original product: scan an Effect TS repo, score it, report anti-patterns. Useful as a
guardrail (`--agent-strict` is a CI gate). Honest caveat: *style* rules are the most
exposed-to-the-bitter-lesson part — as models write more idiomatic code, hard-coded "don't use
a ternary" rules age out. The durable value is the gate + merge above.

## Version control: use jj, don't build one
The "git is painful for agents" problem (stale branches, juggling two tickets, conflict noise)
is best solved by **[jj](https://github.com/jj-vcs/jj) + our semantic merge driver**, not by a
bespoke VCS. jj gives working-copy-as-commit, instant switching between changes, auto-rebase,
and conflicts-as-data (never a blocking half-merge); the merge driver kills the dumb conflicts;
leases keep two tickets in different lanes. We deliberately do **not** ship our own VCS model.

## What was removed (and why)
An earlier version had a context server, an orchestrator/runner, impact-based test selection,
and a homegrown op-log VCS. All cut: agents gather their own context and plan their own work;
test runners already do `--changed`; jj already is the VCS. Keeping them would have been
scaffolding to chalk up later.

## Distribution
- CLI: `npm i -g @jgalbsss/agent-doctor` (or `npx`).
- Claude Code plugin (`plugins/agent-doctor/`): skill + slash commands + a commit-gate hook.
- See [INTEGRATIONS.md](./INTEGRATIONS.md) for Graphite/git-hook/CI wiring.
