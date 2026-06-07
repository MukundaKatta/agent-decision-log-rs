/*!
agent-decision-log: WHY-layer decision log for AI agents.

Records every branch decision your agent makes — the options considered, the
chosen path, the rationale, and (later) the observed outcome. Pairs with
`agentsnap` (CALLS), `agenttrace` (COST), and `agent-citation` (WHERE).

```rust
use agent_decision_log::DecisionLog;
use serde_json::json;

let mut log = DecisionLog::new();
let id = log.add(
    vec!["search_web", "ask_user"],
    "search_web",
    "Query is specific enough to search without clarification.",
    json!({"turn": 3}),
);
log.set_outcome(&id, "Found 5 relevant docs.");

let d = log.find_by_id(&id).unwrap();
assert_eq!(d.chosen, "search_web");
assert!(d.chose_listed_option());
```
*/

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique, time-prefixed decision id (`dec_<hex-time>_<hex-seq>`).
fn new_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dec_{ts:016x}_{n:08x}")
}

// ---- Decision -------------------------------------------------------------

/// One recorded branch decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// Unique id for this decision.
    pub id: String,
    /// When the decision was recorded.
    pub timestamp: SystemTime,
    /// All options that were considered.
    pub options: Vec<String>,
    /// The chosen option.
    pub chosen: String,
    /// Why this option was chosen.
    pub rationale: String,
    /// The observed outcome, if set later.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outcome: Option<String>,
    /// Arbitrary structured metadata.
    pub meta: Value,
}

impl Decision {
    /// `true` when the chosen option appears in the listed options.
    ///
    /// A `false` result flags a hallucinated branch: the agent committed to an
    /// option it never listed as a candidate.
    pub fn chose_listed_option(&self) -> bool {
        self.options.iter().any(|o| o == &self.chosen)
    }
}

// ---- DecisionLog ----------------------------------------------------------

/// Append-only log of agent branch decisions.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionLog {
    /// Recorded decisions, in insertion order.
    pub decisions: Vec<Decision>,
}

impl DecisionLog {
    /// Create an empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a decision and return its generated id.
    ///
    /// `options`, `chosen`, and `rationale` accept anything convertible into a
    /// `String` (e.g. `&str` or `String`).
    pub fn add<O, S, R>(&mut self, options: Vec<O>, chosen: S, rationale: R, meta: Value) -> String
    where
        O: Into<String>,
        S: Into<String>,
        R: Into<String>,
    {
        let id = new_id();
        self.decisions.push(Decision {
            id: id.clone(),
            timestamp: SystemTime::now(),
            options: options.into_iter().map(Into::into).collect(),
            chosen: chosen.into(),
            rationale: rationale.into(),
            outcome: None,
            meta,
        });
        id
    }

    /// Set the outcome for an existing decision by id. Returns `true` if found.
    ///
    /// `outcome` accepts anything convertible into a `String` (e.g. `&str` or
    /// `String`).
    pub fn set_outcome<S: Into<String>>(&mut self, id: &str, outcome: S) -> bool {
        for d in &mut self.decisions {
            if d.id == id {
                d.outcome = Some(outcome.into());
                return true;
            }
        }
        false
    }

    /// Find a decision by id.
    pub fn find_by_id(&self, id: &str) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == id)
    }

    /// The most recently recorded decision, if any.
    pub fn last(&self) -> Option<&Decision> {
        self.decisions.last()
    }

    /// Number of recorded decisions.
    pub fn len(&self) -> usize {
        self.decisions.len()
    }

    /// `true` when no decisions have been recorded.
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty()
    }

    /// Write the log as JSONL — one decision object per line.
    pub fn to_jsonl<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        for d in &self.decisions {
            let line = serde_json::to_string(d)?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        writer.flush()
    }

    /// Load a log from a JSONL file. Blank lines are skipped; a malformed line
    /// yields an [`io::ErrorKind::InvalidData`] error.
    pub fn from_jsonl<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut decisions = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let decision: Decision = serde_json::from_str(&line)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            decisions.push(decision);
        }
        Ok(Self { decisions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn record_creates_entry() {
        let mut log = DecisionLog::new();
        let id = log.add(vec!["a", "b"], "a", "because a", json!({}));
        let e = log.find_by_id(&id).unwrap();
        assert_eq!(e.chosen, "a");
        assert_eq!(e.options, vec!["a", "b"]);
        assert_eq!(e.rationale, "because a");
        assert!(e.outcome.is_none());
    }

    #[test]
    fn len_increments() {
        let mut log = DecisionLog::new();
        assert_eq!(log.len(), 0);
        log.add(vec!["x"], "x", "r", json!({}));
        log.add(vec!["y"], "y", "r", json!({}));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn unique_ids() {
        let mut log = DecisionLog::new();
        let a = log.add(vec!["x"], "x", "r", json!({}));
        let b = log.add(vec!["x"], "x", "r", json!({}));
        assert_ne!(a, b);
    }

    #[test]
    fn chose_listed_option_detects_hallucination() {
        let mut log = DecisionLog::new();
        log.add(vec!["a", "b"], "c", "made up", json!({}));
        assert!(!log.last().unwrap().chose_listed_option());
    }
}
