#!/bin/sh
# A stand-in for a real coding agent. The orchestrator pipes a task spec (JSON)
# on stdin; we emit a TaskOutcome (JSON) on stdout. A real agent would be a
# wrapper around `claude -p` here — the deterministic loop around it is identical.
# (Uses `python3 -c` so the task spec on stdin is read by the program, not eaten
# by a heredoc.)
exec python3 -c 'import sys, json
spec = json.load(sys.stdin)
t = spec["task"]
target = t["targets"][0]
name = target.split("/")[-1][:-3]
code = "// " + t["intent"] + "\nexport function " + name + "(): string {\n  return \"" + name + " implemented\"\n}\n"
print(json.dumps({"changes": [{"path": target, "new_source": code}], "summary": "implemented " + name}))'
