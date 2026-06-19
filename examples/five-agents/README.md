# Demo: 5 agents build a ledger system on the toolkit

Run it (builds nothing but the release binary, then uses a throwaway repo):

```sh
cargo build --release -p agent-doctor
bash examples/five-agents/run.sh
```

`agent.sh` is a stand-in for a real coding agent: the orchestrator pipes it a task
spec as JSON on stdin, it returns the file changes as JSON on stdout. Swap it for a
wrapper around `claude -p` and the deterministic loop around it is unchanged.

## The task

"Implement a ledger system." The orchestrator (or you) decomposes it along module
boundaries into **5 subtasks with disjoint targets**, so 5 agents can work without
colliding:

| agent | task | owns (lease) |
|-------|------|--------------|
| a1 | accounts module     | `src/accounts.ts` |
| a2 | transactions module | `src/transactions.ts` |
| a3 | balances module     | `src/balances.ts` |
| a4 | validation module   | `src/validation.ts` |
| a5 | api module          | `src/api.ts` (after a1, a2, a3) |

## What each step shows

1. **`agent-doctor init`** — one command scaffolds the policy, gitignores local
   state, writes the MCP config, and registers the semantic merge driver.
2. **The ledger** — the DAG of tasks (a5 depends on a1–a3, so it runs only once they
   land; the rest are independent).
3. **`agent-doctor orchestrate`** — for each task the runner leases its region,
   assembles a context pack, runs the agent, **gates** the diff, and selects the
   **impacted tests** — deterministic shell around the nondeterministic agent. All 5
   complete; each lands with exactly the 1 test that covers it.
4. **`agent-doctor impact`** — after a follow-up edit to accounts, only
   `test/accounts.test.ts` is selected — not the whole suite.
5. **`agent-doctor gate`** — a rogue agent edits a protected path; the gate denies it
   and exits non-zero. No protected file is touched, regardless of the agent.
6. **Semantic merge** — two agents extend the *same* shared `registry.ts`
   concurrently. Vanilla git conflicts on the overlapping lines; the semantic driver
   merges them cleanly, keeping both functions.

## Why the leases matter

Because the 5 targets are disjoint, the coordinator hands out 5 non-overlapping
leases, so the work is parallelizable by construction. If two tasks targeted the same
region, the second would block until the first released — collisions are prevented up
front, not reconciled after.
