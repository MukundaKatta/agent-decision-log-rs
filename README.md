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

## JSONL persistence

```rust
use agent_decision_log::DecisionLog;

let mut log = DecisionLog::new();
// ... log.add(...) ...
log.to_jsonl("decisions.jsonl").unwrap();

let loaded = DecisionLog::from_jsonl("decisions.jsonl").unwrap();
```

Each decision serializes as one JSON object per line. Good for `jq -c`
piping or DuckDB's `read_json_auto`.

## API

- `Decision { id, timestamp, options, chosen, rationale, outcome, meta }`
- `DecisionLog::new()`
- `DecisionLog::add(options, chosen, rationale, meta) -> id`
- `DecisionLog::set_outcome(id, outcome) -> bool`
- `DecisionLog::find_by_id(id) -> Option<&Decision>`
- `DecisionLog::last() -> Option<&Decision>`
- `DecisionLog::len() / is_empty()`
- `DecisionLog::to_jsonl(path) / from_jsonl(path)`
- `Decision::chose_listed_option() -> bool`

## License

MIT
