# agent-decision-log

WHY-layer decision log for AI agents. Records the reasoning behind each
branch in an agent run: what options were considered, which one was chosen,
the rationale, and what happened.

Sibling of [`agentsnap`](https://crates.io/crates/agentsnap) (CALLS) and
[`agenttrace`](https://crates.io/crates/agenttrace) (COST + LATENCY).
Together they cover the three audit dimensions of an agent run.

## Install

```toml
[dependencies]
agent-decision-log = "0.1"
```

## Usage

```rust
use agent_decision_log::DecisionLog;
use serde_json::json;

let mut log = DecisionLog::new();
let id = log.add(
    vec!["search_web".into(), "ask_user".into()],
    "search_web",
    "Query is specific enough to search without clarification.",
    json!({"turn": 3}),
);
log.set_outcome(&id, "Found 5 relevant docs.");

let d = log.find_by_id(&id).unwrap();
assert_eq!(d.chosen, "search_web");
```

## Auditing a run

The headline use is catching branches the agent never actually offered itself
as a candidate (a *hallucinated* branch), and spotting decisions it took but
never resolved:

```rust
use agent_decision_log::DecisionLog;
use serde_json::json;

let mut log = DecisionLog::new();
log.add(vec!["search_web", "ask_user"], "search_web", "specific query", json!({}));
let id = log.add(vec!["a", "b"], "c", "off-menu choice", json!({}));

// Branches whose chosen option was never listed:
for d in log.hallucinated() {
    println!("hallucinated branch: chose {:?}", d.chosen);
}

// Decisions still missing an outcome:
assert_eq!(log.pending().count(), 2);
log.set_outcome(&id, "recovered");
assert_eq!(log.pending().count(), 1);

// Iterate in insertion order:
for d in &log {
    let _ = d.rationale.len();
}
```

## JSONL persistence

```rust
use agent_decision_log::DecisionLog;

let mut log = DecisionLog::new();
// ... log.add(...) ...
log.to_jsonl("decisions.jsonl").unwrap();        // truncates and writes
log.append_jsonl("decisions.jsonl").unwrap();    // appends across turns

let loaded = DecisionLog::from_jsonl("decisions.jsonl").unwrap();
```

Each decision serializes as one JSON object per line. Good for `jq -c`
piping or DuckDB's `read_json_auto`. Use `append_jsonl` to stream new
decisions onto a growing file across multiple agent turns or processes,
and `to_jsonl` when you want to (re)write the whole log.

## API

- `Decision { id, timestamp, options, chosen, rationale, outcome, meta }`
- `DecisionLog::new()`
- `DecisionLog::add(options, chosen, rationale, meta) -> id`
- `DecisionLog::set_outcome(id, outcome) -> bool`
- `DecisionLog::find_by_id(id) -> Option<&Decision>`
- `DecisionLog::last() -> Option<&Decision>`
- `DecisionLog::iter() -> impl Iterator<Item = &Decision>` (also `for d in &log`)
- `DecisionLog::hallucinated() -> impl Iterator<Item = &Decision>` — chosen option not in `options`
- `DecisionLog::pending() -> impl Iterator<Item = &Decision>` — no outcome yet
- `DecisionLog::len() / is_empty()`
- `DecisionLog::to_jsonl(path) / append_jsonl(path) / from_jsonl(path)`
- `Decision::chose_listed_option() -> bool`

## License

MIT
