//! # agent-decision-log
//!
//! WHY-layer decision log for AI agents. Records the reasoning behind each
//! branch in an agent run: what options were considered, which one was
//! chosen, why, and what happened afterward.
//!
//! Sibling of [`agentsnap`] (CALLS) and [`agenttrace`] (COST + LATENCY).
//! Together they cover the three audit dimensions of an agent run.
//!
//! [`agentsnap`]: https://crates.io/crates/agentsnap
//! [`agenttrace`]: https://crates.io/crates/agenttrace
//!
//! ## Quick example
//!
//! ```
//! use agent_decision_log::DecisionLog;
//! use serde_json::json;
//!
//! let mut log = DecisionLog::new();
//! let id = log.add(
//!     vec!["search_web", "ask_user"],
//!     "search_web",
//!     "Query is specific enough to search without clarification.",
//!     json!({"turn": 3}),
//! );
//! log.set_outcome(&id, "Found 5 relevant docs.");
//!
//! let d = log.find_by_id(&id).unwrap();
//! assert_eq!(d.chosen, "search_web");
//! assert_eq!(d.outcome.as_deref(), Some("Found 5 relevant docs."));
//! ```
//!
//! ## Round-trip to JSONL
//!
//! ```
//! # fn main() -> std::io::Result<()> {
//! use agent_decision_log::DecisionLog;
//! use serde_json::json;
//!
//! let dir = tempfile::tempdir()?;
//! let path = dir.path().join("decisions.jsonl");
//!
//! let mut log = DecisionLog::new();
//! log.add(
//!     vec!["a", "b"],
//!     "a",
//!     "a is cheaper",
//!     json!({}),
//! );
//! log.to_jsonl(&path)?;
//!
//! let loaded = DecisionLog::from_jsonl(&path)?;
//! assert_eq!(loaded.len(), 1);
//! # Ok(())
//! # }
//! ```

#![deny(missing_docs)]

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A single decision point in an agent run.
///
/// The shape is intentionally wide so different agent frameworks can map
/// onto the same record without inventing new schemas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// Stable unique id, used by [`DecisionLog::set_outcome`] to find the record.
    pub id: String,

    /// When the decision was recorded.
    #[serde(with = "system_time_serde")]
    pub timestamp: SystemTime,

    /// Candidate options the model considered at this branch.
    pub options: Vec<String>,

    /// The option the model actually picked. May or may not be in `options`.
    pub chosen: String,

    /// Free or structured text explaining the pick.
    pub rationale: String,

    /// Post-hoc observation of what happened. `None` until
    /// [`DecisionLog::set_outcome`] is called.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,

    /// Free-form metadata. Stored verbatim.
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl Decision {
    /// Whether the chosen value was actually one of the candidates.
    ///
    /// `false` here usually means the model hallucinated a tool name.
    pub fn chose_listed_option(&self) -> bool {
        self.options.iter().any(|o| o == &self.chosen)
    }
}

/// Append-only log of agent decisions.
///
/// Holds an in-memory `Vec<Decision>`. Persist with [`DecisionLog::to_jsonl`]
/// and round-trip with [`DecisionLog::from_jsonl`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DecisionLog {
    /// The recorded decisions, in insertion order.
    pub decisions: Vec<Decision>,
}

impl DecisionLog {
    /// Build an empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new decision and return its id.
    ///
    /// The id is generated from a high-resolution timestamp plus a process
    /// local atomic counter, so successive calls always produce distinct
    /// ids even when issued in the same nanosecond.
    pub fn add<O, C, R>(
        &mut self,
        options: Vec<O>,
        chosen: C,
        rationale: R,
        meta: serde_json::Value,
    ) -> String
    where
        O: Into<String>,
        C: Into<String>,
        R: Into<String>,
    {
        let id = new_id();
        let decision = Decision {
            id: id.clone(),
            timestamp: SystemTime::now(),
            options: options.into_iter().map(Into::into).collect(),
            chosen: chosen.into(),
            rationale: rationale.into(),
            outcome: None,
            meta,
        };
        self.decisions.push(decision);
        id
    }

    /// Attach an outcome to an existing decision.
    ///
    /// Returns `true` if a decision with that id was found, `false`
    /// otherwise. Existing outcomes are overwritten.
    pub fn set_outcome<S: Into<String>>(&mut self, id: &str, outcome: S) -> bool {
        if let Some(d) = self.decisions.iter_mut().find(|d| d.id == id) {
            d.outcome = Some(outcome.into());
            true
        } else {
            false
        }
    }

    /// Find a decision by its id.
    pub fn find_by_id(&self, id: &str) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == id)
    }

    /// The most recently added decision, if any.
    pub fn last(&self) -> Option<&Decision> {
        self.decisions.last()
    }

    /// Number of recorded decisions.
    pub fn len(&self) -> usize {
        self.decisions.len()
    }

    /// Whether the log has no decisions.
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty()
    }

    /// Write each decision as one JSON line at `path`.
    ///
    /// Overwrites any existing file. The output is UTF-8 JSON with a
    /// trailing newline after each record, suitable for streaming ingest
    /// tools like `jq -c` or DuckDB's `read_json_auto`.
    pub fn to_jsonl<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let file = File::create(path)?;
        let mut w = BufWriter::new(file);
        for d in &self.decisions {
            let line = serde_json::to_string(d).map_err(io_invalid_data)?;
            w.write_all(line.as_bytes())?;
            w.write_all(b"\n")?;
        }
        w.flush()
    }

    /// Read a JSONL file produced by [`DecisionLog::to_jsonl`].
    ///
    /// Blank lines are skipped. Any malformed line returns an `InvalidData`
    /// error and stops the load; partial state is not returned.
    pub fn from_jsonl<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut decisions = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let d: Decision = serde_json::from_str(trimmed).map_err(io_invalid_data)?;
            decisions.push(d);
        }
        Ok(Self { decisions })
    }
}

fn io_invalid_data(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

// SystemTime nanos since epoch + process atomic counter. The counter is
// what guarantees uniqueness inside the same nanosecond; the timestamp is
// what keeps ids roughly sortable.
fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dec_{:016x}{:08x}", nanos, seq)
}

mod system_time_serde {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let nanos = t
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        nanos.to_string().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let raw = String::deserialize(d)?;
        let nanos: u128 = raw.parse().map_err(serde::de::Error::custom)?;
        let secs = (nanos / 1_000_000_000) as u64;
        let sub = (nanos % 1_000_000_000) as u32;
        Ok(UNIX_EPOCH + Duration::new(secs, sub))
    }
}
