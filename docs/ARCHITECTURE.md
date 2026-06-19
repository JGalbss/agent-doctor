# agent-doctor — Architecture

> The **agent toolkit** built on this kernel (policy/gate + leases, semantic merge) is
> described in [TOOLKIT.md](./TOOLKIT.md).

A react.doctor-style health scanner for Effect TS codebases. Scan a repo, score it 0–100,
report Effect anti-patterns with code frames, hand off fixes to AI agents. Built for massive
monorepos: target ~1–2s for 10k files.

## Decisions (June 2026)

### Two-tier analysis

- **Tier 1 (this repo, v1): Rust core on oxc.** `ignore` (ripgrep's walker) → `rayon`
  parallel per-file pipeline → `oxc_parser` → `oxc_semantic` → rule engine modeled on
  oxlint's `Rule` trait (per-node dispatch over the flattened `AstNodes` vector).
  Import-aware matching via the parser ModuleRecord + symbol resolution — we know a binding
  came from `effect` even when aliased. Covers the ~60% of the rule catalog that is
  pure-AST detectable (see RULES.md).
- **Tier 2 (`agent-doctor --deep`, v1.x): orchestrate `@effect/tsgo`** — Effect's pinned
  typescript-go distribution with `@effect/language-service` (50+ type-aware diagnostics:
  `floatingEffect`, `layerMergeAllWithDependencies`, channel-type rules) compiled in.
  Shell out, parse diagnostics, merge into the report. Never reimplement type-aware
  analysis in Rust — even oxlint outsources this (tsgolint).

### Why not X

- **oxlint JS plugins** (react-doctor's approach): alpha, don't own the product surface,
  JS rule execution slower than native. react-doctor rents oxlint's walker because React
  rules need the JS ecosystem; Effect rules don't.
- **swc**: no semantic layer (no scopes/symbols), parser ~3x slower than oxc.
- **tree-sitter / ast-grep**: no symbol/import resolution → can't distinguish
  `import { Effect } from "effect"` from a local named `Effect`. ast-grep may later serve
  as an embedded engine for user-defined custom rules.
- **Pure TS (ts-morph/compiler API)**: 10–50x too slow at 10k files.

### oxc pinning

oxc is 0.x with weekly breaking releases — pinned to 0.135.x in the workspace manifest.
`oxc_linter` is `publish = false` on crates.io; we copy its rule-trait architecture, we
don't import it.

### Version-aware rule profiles

Detect installed `effect` major from package.json/lockfile. Several rules invert between
v3 and v4 (`Layer.scoped`→`Layer.effect`, `Effect.Service` shape, barrel imports,
`Schema.Data`). v4-migration rules only fire in the v4 profile (or as a `--migrate` audit
against a v3 codebase).

### Scoring (react-doctor's model, server-optional)

`score = max(0, round(100 − 1.5 × |distinct rules with errors| − 0.75 × |distinct rules with warnings|))`
— penalty per distinct rule fired, not per occurrence. Labels: ≥75 Great, ≥50 Needs work,
<50 Critical. Computed locally (react-doctor does it server-side for leaderboards; we
don't need a server for v1).

### Product surface (sequenced)

1. `npx @jgalbsss/agent-doctor` / `agent-doctor [dir]` — scan, animated score, grouped report
   (top 3 rule groups, `--verbose` for all), code frames, `--json` (versioned schema).
2. Scopes: `--scope full|files|changed|lines` + `--base <ref>` with content-hash
   fingerprint baseline (react-doctor's `compute-diagnostic-delta` model) for CI deltas.
3. Agent handoff: post-scan prompt → install skill + launch detected CLI agent with an
   engineered fix payload; per-rule fix recipes as remote markdown.
4. GitHub Action (changed-files, sticky PR comment, inline review comments, score status).
5. `--deep` tsgo tier; LSP daemon + editor extensions later.

### Distribution

Biome model: standalone per-platform binaries via npm `optionalDependencies`
(`@agent-doctor/cli-darwin-arm64`, …) + JS shim bin. No napi until an in-process JS API
is needed.

## Layout

- `crates/core` — engine: walker, parse pipeline, rule trait + registry, diagnostics, scoring.
- `crates/cli` — `agent-doctor` binary: args, report rendering, JSON output.
- `references/` (gitignored) — cloned: `effect` (v3 main), `effect-v4` (effect-smol,
  4.0.0-beta.x — the real v4), `skills`, `language-service`, `react-doctor`, `million`.
  react-doctor's `AGENTS.md` is a canonical Effect v4 idiom reference; the
  language-service `src/diagnostics/` is the de facto spec for Effect lint semantics.
