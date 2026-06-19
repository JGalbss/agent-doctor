# agent-doctor

Health checks for Effect TS codebases — [react.doctor](https://www.react.doctor/), but for
[Effect](https://effect.website/). Scans a repo, scores it 0–100, and reports Effect
anti-patterns with file locations. Written in Rust on the [oxc](https://oxc.rs) toolchain:
~40ms for 1,100 files, ~200ms for 1,800.

```
  agent doctor  v0.1.0

  ███████████████████████████░░░  91/100 — Great

  ✖ require-yield-star  error · Correctness · 2 issues
    Inside Effect.gen, effects must be yielded with `yield*`. ...
    src/program.ts:4:17  const value = yield Effect.succeed(1)
```

## Usage

```sh
cargo build --release
agent-doctor <dir>                      # scan everything
agent-doctor <dir> --verbose --json     # full report / machine-readable
agent-doctor --scope changed            # only files changed vs main (PR mode)
agent-doctor --scope lines --base main  # only issues on lines you touched
agent-doctor rules                      # list all 101 rules
agent-doctor explain no-map-returning-effect   # why + how to rewrite it
agent-doctor rules --json               # full catalog with rewrite recipes
agent-doctor --deep                     # merge type-aware @effect/language-service findings
agent-doctor lsp                        # run as a language server (editor diagnostics)
agent-doctor --adopt --scope lines      # experimental: vanilla-TS → Effect migration
                                         # recommendations, on exactly your PR's lines
agent-doctor --agent                    # experimental "agent doctor": flag the non-Effect
                                         # slop LLM agents emit (if/else, ternaries, raw loops…)
agent-doctor --agent-strict             # same, but escalate to errors and exit non-zero (CI gate)
```

## Toolkit — guardrails for coding agents

Beyond the linter, agent-doctor adds two deterministic guardrails, deliberately scoped to what
stays relevant as models improve (gate + semantic merge); it does **not** try to gather context,
plan, or orchestrate for the agent. Design + rationale: [docs/TOOLKIT.md](docs/TOOLKIT.md).

```sh
agent-doctor gate --base main --actor a   # gate the diff vs policy/ACL/leases (deny exits non-zero)
agent-doctor verify --run "npx vitest run" # gate, then run your tests (pre-push hook / CI)
agent-doctor merge BASE OURS THEIRS        # semantic (AST) 3-way merge of TypeScript
```

### Set up in your repo

```sh
agent-doctor init          # interactive walkthrough (shadcn-style)
agent-doctor init --yes    # accept all recommended options (CI / scripted)
```

`init` writes `agent-doctor.policy.toml`, `.agent-doctor/.gitignore`, and registers the semantic
merge driver in git config + `.gitattributes`. In a terminal it then prompts (or use flags) to
install the **Claude Code skill** (`--skills`) and the **pre-push hook** (`--hooks`). Idempotent;
`--force` overwrites.

### Conflict-free parallel work

Two agents (or two tickets) editing the same file merge cleanly when they touch different
declarations — the merge driver `init` registers, also settable by hand:

```sh
git config merge.agent-doctor.driver "agent-doctor merge %O %A %B"
echo '*.ts  merge=agent-doctor' >> .gitattributes
```

For the broader "git is painful for agents" problem (stale branches, juggling two tickets),
pair this with [jj](https://github.com/jj-vcs/jj) — see [docs/TOOLKIT.md](docs/TOOLKIT.md).

### Graphite / git hooks / CI · Claude Code plugin

`agent-doctor verify` (gate + your tests) wires up as a pre-push hook so it fires on `gt submit`,
or as a GitHub Action; or install the Claude Code plugin for the skill + commit-gate hook.
Full setup: [docs/INTEGRATIONS.md](docs/INTEGRATIONS.md).

## Docs site

`site/` is an Astro site rendering the full rule catalog with side-by-side bad→good
rewrites, search, and category filters. `npm run gen` regenerates its data from
`agent-doctor rules --json`; `npm run dev` to work on it locally.

## Status

Early but real: **101 rules live** across correctness, idiomatic, architecture,
performance, and v4-migration categories — every rule ships with a bad→good rewrite
recipe (`explain`), and 120+ integration tests cover the catalog (bad patterns fire,
clean code stays silent; example coverage is test-enforced). Rule sources: the Effect-TS
skills repo, the @effect/language-service diagnostic catalog, the effect-smol v4
MIGRATION guide, and the EffectPatterns community corpus (304 patterns). The full spec
is in [docs/RULES.md](docs/RULES.md); architecture and roadmap in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

- Import-aware matching: `import { Effect as E } from "effect"` and
  `import * as Effect from "effect/Effect"` both resolve correctly.
- Version-aware profiles: effect major detected from package.json; v4-migration rules
  fire on v4 codebases automatically, or on v3 with `--migrate`.
- Test-file classification: findings in `*.test.ts` / `test/` paths stay in the report
  but don't count toward the score (except test-specific rules).
- Score model: penalty per distinct rule fired (errors 1.5, warnings 0.75), info rules
  never affect the score.
- Diff scoping: `--scope changed` (files) / `--scope lines` vs `--base` (defaults to the
  merge-base with main) — untracked files count as fully changed.
- `--deep` tier: merges the ~78 type-aware diagnostics from
  `@effect/language-service` (its headless `diagnostics --format json` CLI) as `ls/*`
  rules — we never reimplement type analysis.
- `agent-doctor lsp`: stdio language server publishing the syntactic rule set as
  editor diagnostics (full-sync; rule id as the diagnostic code).
- `--adopt` (experimental): flags vanilla TS that should migrate to Effect — async
  functions, `.then()` chains, `new Promise`, `Promise.all`, sequential awaits in loops —
  each with the clean Effect rewrite. `prefer-foreach-over-yield-loop` (yield loops
  inside Effect.gen → `Effect.forEach`) is always on as info.
- `--agent` (experimental, "agent doctor"): flags the non-Effect, non-functional patterns
  LLM agents reach for by default — `if/else` chains, ternaries, `x === "literal"` guards,
  raw `for`/`while` loops, `let`/`var` mutation, inline `import()`/`require()`, reassignment / in-place payload mutation
  (intermediate states), and copy-pasted function bodies — each with the clean
  Effect/`Match`/combinator rewrite. Defaults to `warn`; `--agent-strict` escalates to `error`
  and exits non-zero so it can gate CI. It also runs a **cross-file pass**: a repo-wide index
  of named/bound functions flags ones that duplicate another by body (exact / fuzzy), name, or
  shape (params + call set) — so an agent reusing context sees "this helper already exists"
  instead of re-creating it. All duplicate/similarity findings stay info suggestions.
- Planned: suppression comments, config file, editor extension packaging, agent
  handoff, npm distribution as per-platform binaries.

## Development

Requires rustc ≥ 1.94 (`rust-toolchain.toml` pins stable via rustup; if a Homebrew rust
shadows it, `brew unlink rust` or pass `RUSTC=$HOME/.rustup/toolchains/<host>/bin/rustc`).

Reference repos for rule development are expected (gitignored) under `references/`:
`effect` (v3), `effect-v4` (effect-smol), `skills`, `language-service`, `react-doctor`.
