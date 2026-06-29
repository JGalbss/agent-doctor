---
name: agent-doctor
description: Use the agent-doctor linter in this repo to check Effect TS / TypeScript for non-idiomatic code and Effect anti-patterns (if/else chains, ternaries, string-equality guards, oversized files, missing yield*). Run it before committing or submitting a PR/stack, or whenever you've written or edited TypeScript.
---

# agent-doctor

This repo uses **agent-doctor** — a fast, deterministic AST-level linter for Effect TS /
TypeScript. It returns facts (rule violations with locations and a health score), not
opinions. Prefer it over ad-hoc greps to check whether your changes are idiomatic.

## After you write or edit TypeScript

Scan your changes before committing:

```sh
agent-doctor --scope changed --base main   # only files changed vs main
agent-doctor --agent-strict                # flag non-Effect agent slop and exit non-zero
```

`--agent` flags the patterns LLM agents tend to emit — if/else chains, ternaries,
string-equality guards (`x === "foo"`), raw loops, `let`, oversized files, and duplicated
function bodies. `--agent-strict` escalates those to errors so the command exits non-zero
(a CI / pre-commit gate). Fix the flagged code; don't suppress it unless a human told you to.

## Understand a rule

```sh
agent-doctor rules                 # list every rule (id, severity, category)
agent-doctor explain <rule-id>     # what it means + a before/after rewrite recipe
```

## Useful flags

- `--scope changed|lines --base <ref>` — scan only changed files, or only changed lines.
- `--deep` — also merge the type-aware `@effect/language-service` diagnostics.
- `--no-react` — skip the React tier (see below).
- `--json` — machine-readable report (for tooling / CI).
- `--migrate` — run the Effect v3 → v4 migration audit.

## React projects

In a React repo (a `react` dependency in package.json), agent-doctor automatically runs
[react-doctor](https://www.react-doctor.com/)'s full rule set and merges its findings as
`rd/*` rules — no extra command. Install it so the tier can run: `npm i -D react-doctor`.

## Editor diagnostics

`agent-doctor lsp` runs as a language server over stdio for live in-editor diagnostics.
