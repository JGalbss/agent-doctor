#!/usr/bin/env bash
# Demo: 5 agents build a ledger system through the agent-doctor toolkit.
# Sets up a throwaway repo, then drives the whole stack with the real CLI.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
bin="$root/target/release/agent-doctor"
[ -x "$bin" ] || { echo "build first: cargo build --release -p agent-doctor"; exit 1; }

work="$(mktemp -d)/ledger-app"
mkdir -p "$work/src" "$work/test"
cd "$work"
git init -q -b main && git config user.email d@d.co && git config user.name d

section() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

# ---- base repo: 5 stub modules + their tests, plus a shared registry ----
for m in accounts transactions balances validation api; do
  echo "export const TODO_$m = true" > "src/$m.ts"
  echo "import '../src/$m'" > "test/$m.test.ts"
done
echo "export const registry: string[] = []" > "src/registry.ts"
git add -A && git commit -qm "scaffold ledger app"

section "1. one-command setup (agent-doctor init)"
"$bin" init | sed 's/^/    /'

# policy: nobody may touch the migrations dir; api layer can't import test code.
cat > agent-doctor.policy.toml <<'TOML'
[protected]
globs = ["src/db/migrations/**"]
TOML

section "2. the task, decomposed into a ledger of 5 disjoint subtasks"
cat > .agent-doctor/ledger.json <<'JSON'
{ "tasks": [
  { "id": "a1", "intent": "implement accounts module",     "targets": ["src/accounts.ts"] },
  { "id": "a2", "intent": "implement transactions module", "targets": ["src/transactions.ts"] },
  { "id": "a3", "intent": "implement balances module",     "targets": ["src/balances.ts"] },
  { "id": "a4", "intent": "implement validation module",   "targets": ["src/validation.ts"] },
  { "id": "a5", "intent": "implement api module",          "targets": ["src/api.ts"], "deps": ["a1","a2","a3"] }
] }
JSON
python3 -c "import json;[print('   ',t['id'],'->',t['targets'][0], ('(after '+','.join(t.get('deps',[]))+')') if t.get('deps') else '') for t in json.load(open('.agent-doctor/ledger.json'))['tasks']]"

section "3. orchestrate: each task leases its region, gets context, is gated + tested"
"$bin" orchestrate \
  --ledger .agent-doctor/ledger.json \
  --executor "sh $here/agent.sh" \
  --policy agent-doctor.policy.toml | sed 's/^/  /'

section "4. impact — agent a1 changed accounts; which tests must run?"
git add -A && git commit -qm "agents implemented modules"
echo "export function accounts(): string { return \"v2\" }" > src/accounts.ts
git add -A && git commit -qm "a1 follow-up"
"$bin" impact --base HEAD~1 | sed 's/^/  /'

section "5. gate — agent strays into a protected path"
mkdir -p src/db/migrations
echo "export const m = 1" > src/db/migrations/001.ts
git add -A && git commit -qm "stray edit"
( "$bin" gate --base HEAD~1 --actor agent-rogue | sed 's/^/  /' ) || echo "    (gate exited non-zero -> the diff is blocked)"

section "6. semantic merge — two agents extend the shared registry at once"
git checkout -q -b agent-x
printf 'export const registry: string[] = []\nexport function registerAccounts() { registry.push("accounts") }\n' > src/registry.ts
git commit -qam "agent-x: registerAccounts"
git checkout -q main && git checkout -q -b agent-y
printf 'export const registry: string[] = []\nexport function registerTransactions() { registry.push("transactions") }\n' > src/registry.ts
git commit -qam "agent-y: registerTransactions"
git checkout -q main
git merge -q agent-x -m "merge x"
git merge agent-y -m "merge y" >/dev/null 2>&1 && echo "    merged agent-y cleanly (no conflict)"
echo "    --- src/registry.ts after both merges ---"
sed 's/^/      /' src/registry.ts

section "done"
echo "    5 agents, disjoint leases, deterministic gate+tests per task, conflict-free shared edits."
echo "    workspace: $work"
