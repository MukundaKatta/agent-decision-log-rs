/*!
agent-decision-log: WHY-layer decision log for AI agents.

Records every branch decision your agent makes — the options considered, the
chosen path, the rationale, and (later) the observed outcome. Pairs with
`agentsnap` (CALLS), `agenttrace` (COST), and `agent-citation` (WHERE).

```rust
use agent_decision_log::DecisionLog;

let mut log = DecisionLog::new();
let entry = log.record(
    &["search", "cache", "skip"],
    "search",
    "Cache is stale, must re-fetch",
);
assert_eq!(entry.chosen, "search");
assert_eq!(log.all().len(), 1);
```
*/

use serde_json::{Map, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dec_{:016x}_{:04x}", ts.wrapping_mul(6364136223846793005), n)
}

fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

// ---- DecisionEntry --------------------------------------------------------

/// One recorded branch decision.
#[derive(Debug, Clone)]
pub struct DecisionEntry {
    pub id: String,
    /// All options that were considered.
    pub options: Vec<String>,
    /// The chosen option.
    pub chosen: String,
    /// Why this option was chosen.
    pub rationale: String,
    /// The observed outcome, if set later.
    pub outcome: Option<String>,
    pub timestamp: f64,
    pub metadata: Map<String, Value>,
}

impl DecisionEntry {
    pub fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("id".to_owned(), Value::String(self.id.clone()));
        m.insert(
            "options".to_owned(),
            Value::Array(self.options.iter().map(|s| Value::String(s.clone())).collect()),
        );
        m.insert("chosen".to_owned(), Value::String(self.chosen.clone()));
        m.insert("rationale".to_owned(), Value::String(self.rationale.clone()));
        if let Some(ref out) = self.outcome {
            m.insert("outcome".to_owned(), Value::String(out.clone()));
        }
        m.insert("timestamp".to_owned(), Value::from(self.timestamp));
        if !self.metadata.is_empty() {
            m.insert("metadata".to_owned(), Value::Object(self.metadata.clone()));
        }
        Value::Object(m)
    }
}

// ---- DecisionLog ----------------------------------------------------------

/// Append-only log of agent branch decisions.
#[derive(Debug, Default, Clone)]
pub struct DecisionLog {
    entries: Vec<DecisionEntry>,
}

impl DecisionLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a decision. Returns a reference to the stored entry.
    pub fn record(&mut self, options: &[&str], chosen: &str, rationale: &str) -> &DecisionEntry {
        let entry = DecisionEntry {
            id: new_id(),
            options: options.iter().map(|s| s.to_string()).collect(),
            chosen: chosen.to_owned(),
            rationale: rationale.to_owned(),
            outcome: None,
            timestamp: now_f64(),
            metadata: Map::new(),
        };
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// Record with extra metadata.
    pub fn record_with_meta(
        &mut self,
        options: &[&str],
        chosen: &str,
        rationale: &str,
        metadata: Map<String, Value>,
    ) -> &DecisionEntry {
        let entry = DecisionEntry {
            id: new_id(),
            options: options.iter().map(|s| s.to_string()).collect(),
            chosen: chosen.to_owned(),
            rationale: rationale.to_owned(),
            outcome: None,
            timestamp: now_f64(),
            metadata,
        };
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// Set the outcome for an existing decision by id. Returns `true` if found.
    pub fn set_outcome(&mut self, id: &str, outcome: &str) -> bool {
        for e in &mut self.entries {
            if e.id == id {
                e.outcome = Some(outcome.to_owned());
                return true;
            }
        }
        false
    }

    /// Find an entry by id.
    pub fn find(&self, id: &str) -> Option<&DecisionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// All entries in insertion order.
    pub fn all(&self) -> &[DecisionEntry] {
        &self.entries
    }

    /// Entries where `chosen == option`.
    pub fn by_chosen(&self, option: &str) -> Vec<&DecisionEntry> {
        self.entries.iter().filter(|e| e.chosen == option).collect()
    }

    /// Entries that have an outcome set.
    pub fn with_outcomes(&self) -> Vec<&DecisionEntry> {
        self.entries.iter().filter(|e| e.outcome.is_some()).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.entries.iter().map(|e| e.to_json()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_creates_entry() {
        let mut log = DecisionLog::new();
        let e = log.record(&["a", "b"], "a", "because a");
        assert_eq!(e.chosen, "a");
        assert_eq!(e.options, vec!["a", "b"]);
        assert_eq!(e.rationale, "because a");
        assert!(e.outcome.is_none());
    }

    #[test]
    fn len_increments() {
        let mut log = DecisionLog::new();
        assert_eq!(log.len(), 0);
        log.record(&["x"], "x", "r");
        log.record(&["y"], "y", "r");
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn find_by_id() {
        let mut log = DecisionLog::new();
        let id = log.record(&["a"], "a", "r").id.clone();
        let found = log.find(&id).unwrap();
        assert_eq!(found.id, id);
    }

    #[test]
    fn find_missing_returns_none() {
        let log = DecisionLog::new();
        assert!(log.find("nope").is_none());
    }

    #[test]
    fn set_outcome_true() {
        let mut log = DecisionLog::new();
        let id = log.record(&["a"], "a", "r").id.clone();
        assert!(log.set_outcome(&id, "success"));
        assert_eq!(log.find(&id).unwrap().outcome, Some("success".to_string()));
    }

    #[test]
    fn set_outcome_false_when_missing() {
        let mut log = DecisionLog::new();
        assert!(!log.set_outcome("no-such-id", "out"));
    }

    #[test]
    fn by_chosen_filters() {
        let mut log = DecisionLog::new();
        log.record(&["a", "b"], "a", "r");
        log.record(&["a", "b"], "b", "r");
        log.record(&["a", "b"], "a", "r");
        assert_eq!(log.by_chosen("a").len(), 2);
        assert_eq!(log.by_chosen("b").len(), 1);
    }

    #[test]
    fn with_outcomes_filters() {
        let mut log = DecisionLog::new();
        let id1 = log.record(&["a"], "a", "r").id.clone();
        log.record(&["b"], "b", "r");
        log.set_outcome(&id1, "done");
        assert_eq!(log.with_outcomes().len(), 1);
    }

    #[test]
    fn is_empty_initially() {
        let log = DecisionLog::new();
        assert!(log.is_empty());
    }

    #[test]
    fn clear_resets() {
        let mut log = DecisionLog::new();
        log.record(&["a"], "a", "r");
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn to_json_is_array() {
        let mut log = DecisionLog::new();
        log.record(&["a"], "a", "r");
        let j = log.to_json();
        assert!(j.is_array());
        assert_eq!(j.as_array().unwrap().len(), 1);
    }

    #[test]
    fn entry_to_json_fields() {
        let mut log = DecisionLog::new();
        let id = log.record(&["a", "b"], "a", "rationale").id.clone();
        let e = log.find(&id).unwrap();
        let j = e.to_json();
        assert_eq!(j["chosen"], "a");
        assert_eq!(j["rationale"], "rationale");
        assert!(j["options"].is_array());
    }

    #[test]
    fn unique_ids() {
        let mut log = DecisionLog::new();
        let a = log.record(&["x"], "x", "r").id.clone();
        let b = log.record(&["x"], "x", "r").id.clone();
        assert_ne!(a, b);
    }

    #[test]
    fn record_with_meta_stores_metadata() {
        let mut log = DecisionLog::new();
        let mut meta = serde_json::Map::new();
        meta.insert("cost".to_string(), serde_json::json!(0.5));
        let id = log.record_with_meta(&["a"], "a", "r", meta).id.clone();
        let e = log.find(&id).unwrap();
        assert_eq!(e.metadata["cost"], 0.5);
    }

    #[test]
    fn timestamp_non_zero() {
        let mut log = DecisionLog::new();
        let e = log.record(&["a"], "a", "r");
        assert!(e.timestamp > 0.0);
    }

    #[test]
    fn outcome_in_json_when_set() {
        let mut log = DecisionLog::new();
        let id = log.record(&["a"], "a", "r").id.clone();
        log.set_outcome(&id, "win");
        let j = log.find(&id).unwrap().to_json();
        assert_eq!(j["outcome"], "win");
    }
}
