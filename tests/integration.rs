use agent_decision_log::{Decision, DecisionLog};
use serde_json::json;
use std::thread;

fn sample(log: &mut DecisionLog) -> String {
    log.add(
        vec!["a".to_string(), "b".to_string()],
        "a",
        "a is cheaper",
        json!({"turn": 0}),
    )
}

#[test]
fn new_log_is_empty() {
    let log = DecisionLog::new();
    assert!(log.is_empty());
    assert_eq!(log.len(), 0);
    assert!(log.last().is_none());
}

#[test]
fn default_log_is_empty() {
    let log: DecisionLog = Default::default();
    assert_eq!(log.len(), 0);
}

#[test]
fn add_returns_unique_id() {
    let mut log = DecisionLog::new();
    let id1 = sample(&mut log);
    let id2 = sample(&mut log);
    assert_ne!(id1, id2);
    assert_eq!(log.len(), 2);
}

#[test]
fn add_preserves_insertion_order() {
    let mut log = DecisionLog::new();
    let id1 = log.add(vec!["x".to_string()], "x", "first", json!({}));
    let id2 = log.add(vec!["y".to_string()], "y", "second", json!({}));
    assert_eq!(log.decisions[0].id, id1);
    assert_eq!(log.decisions[1].id, id2);
}

#[test]
fn find_by_id_hits_and_misses() {
    let mut log = DecisionLog::new();
    let id = sample(&mut log);
    assert!(log.find_by_id(&id).is_some());
    assert!(log.find_by_id("not-a-real-id").is_none());
}

#[test]
fn last_returns_most_recent() {
    let mut log = DecisionLog::new();
    sample(&mut log);
    let id2 = sample(&mut log);
    assert_eq!(log.last().unwrap().id, id2);
}

#[test]
fn set_outcome_updates_existing() {
    let mut log = DecisionLog::new();
    let id = sample(&mut log);
    assert!(log.set_outcome(&id, "ran fine"));
    assert_eq!(
        log.find_by_id(&id).unwrap().outcome.as_deref(),
        Some("ran fine")
    );
}

#[test]
fn set_outcome_overwrites_previous() {
    let mut log = DecisionLog::new();
    let id = sample(&mut log);
    log.set_outcome(&id, "first");
    log.set_outcome(&id, "second");
    assert_eq!(
        log.find_by_id(&id).unwrap().outcome.as_deref(),
        Some("second")
    );
}

#[test]
fn set_outcome_returns_false_for_missing() {
    let mut log = DecisionLog::new();
    assert!(!log.set_outcome("nope", "x"));
}

#[test]
fn outcome_starts_as_none() {
    let mut log = DecisionLog::new();
    let id = sample(&mut log);
    assert!(log.find_by_id(&id).unwrap().outcome.is_none());
}

#[test]
fn chose_listed_option_true_when_in_options() {
    let mut log = DecisionLog::new();
    log.add(
        vec!["a".to_string(), "b".to_string()],
        "b",
        "picked b",
        json!({}),
    );
    assert!(log.last().unwrap().chose_listed_option());
}

#[test]
fn chose_listed_option_false_when_hallucinated() {
    let mut log = DecisionLog::new();
    log.add(
        vec!["a".to_string(), "b".to_string()],
        "c",
        "made up c",
        json!({}),
    );
    assert!(!log.last().unwrap().chose_listed_option());
}

#[test]
fn add_accepts_string_and_str() {
    let mut log = DecisionLog::new();
    log.add(vec!["a"], "a", "rationale", json!({}));
    log.add(
        vec!["b".to_string()],
        "b".to_string(),
        "r2".to_string(),
        json!({}),
    );
    assert_eq!(log.len(), 2);
}

#[test]
fn meta_round_trips_complex_json() {
    let mut log = DecisionLog::new();
    let meta = json!({
        "turn": 7,
        "tool_args": {"q": "hello", "n": 5},
        "tags": ["retry", "fallback"],
    });
    let id = log.add(vec!["x"], "x", "r", meta.clone());
    assert_eq!(log.find_by_id(&id).unwrap().meta, meta);
}

#[test]
fn id_format_is_dec_prefix() {
    let mut log = DecisionLog::new();
    let id = sample(&mut log);
    assert!(id.starts_with("dec_"));
}

#[test]
fn ids_unique_across_threads() {
    use std::sync::{Arc, Mutex};
    let log = Arc::new(Mutex::new(DecisionLog::new()));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let log_cl = Arc::clone(&log);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let mut guard = log_cl.lock().unwrap();
                guard.add(vec!["a"], "a", "r", json!({}));
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let guard = log.lock().unwrap();
    assert_eq!(guard.len(), 800);
    let mut ids: Vec<&str> = guard.decisions.iter().map(|d| d.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 800);
}

#[test]
fn jsonl_round_trip_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.jsonl");
    let log = DecisionLog::new();
    log.to_jsonl(&path).unwrap();
    let loaded = DecisionLog::from_jsonl(&path).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn jsonl_round_trip_single_decision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("one.jsonl");
    let mut log = DecisionLog::new();
    let id = log.add(vec!["a", "b"], "a", "because reasons", json!({"turn": 4}));
    log.set_outcome(&id, "ok");
    log.to_jsonl(&path).unwrap();

    let loaded = DecisionLog::from_jsonl(&path).unwrap();
    assert_eq!(loaded.len(), 1);
    let d = &loaded.decisions[0];
    assert_eq!(d.id, id);
    assert_eq!(d.chosen, "a");
    assert_eq!(d.outcome.as_deref(), Some("ok"));
    assert_eq!(d.meta, json!({"turn": 4}));
}

#[test]
fn jsonl_round_trip_many_decisions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("many.jsonl");
    let mut log = DecisionLog::new();
    for i in 0..50 {
        let id = log.add(
            vec![format!("opt-{i}")],
            format!("opt-{i}"),
            format!("rationale {i}"),
            json!({"i": i}),
        );
        if i % 2 == 0 {
            log.set_outcome(&id, format!("done {i}"));
        }
    }
    log.to_jsonl(&path).unwrap();

    let loaded = DecisionLog::from_jsonl(&path).unwrap();
    assert_eq!(loaded.len(), 50);
    assert_eq!(loaded.decisions[0].chosen, "opt-0");
    assert_eq!(loaded.decisions[49].chosen, "opt-49");
    assert_eq!(loaded.decisions[10].outcome.as_deref(), Some("done 10"));
    assert!(loaded.decisions[11].outcome.is_none());
}

#[test]
fn jsonl_one_object_per_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lines.jsonl");
    let mut log = DecisionLog::new();
    log.add(vec!["a"], "a", "r1", json!({}));
    log.add(vec!["b"], "b", "r2", json!({}));
    log.to_jsonl(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = raw.lines().collect();
    assert_eq!(lines.len(), 2);
    for line in lines {
        let _: serde_json::Value = serde_json::from_str(line).unwrap();
    }
}

#[test]
fn jsonl_skips_blank_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blanks.jsonl");
    let mut log = DecisionLog::new();
    log.add(vec!["a"], "a", "r", json!({}));
    log.to_jsonl(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("\n\n{}\n\n", raw.trim())).unwrap();

    let loaded = DecisionLog::from_jsonl(&path).unwrap();
    assert_eq!(loaded.len(), 1);
}

#[test]
fn jsonl_rejects_malformed_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.jsonl");
    std::fs::write(&path, "this is not json\n").unwrap();
    let err = DecisionLog::from_jsonl(&path).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn from_jsonl_missing_file_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist.jsonl");
    assert!(DecisionLog::from_jsonl(&path).is_err());
}

#[test]
fn decision_clone_round_trip() {
    let mut log = DecisionLog::new();
    sample(&mut log);
    let d1 = log.decisions[0].clone();
    let d2: Decision = d1.clone();
    assert_eq!(d1, d2);
}

#[test]
fn log_clone_round_trip() {
    let mut log = DecisionLog::new();
    sample(&mut log);
    sample(&mut log);
    let log2 = log.clone();
    assert_eq!(log, log2);
}

#[test]
fn timestamp_is_set_on_add() {
    use std::time::SystemTime;
    let before = SystemTime::now();
    let mut log = DecisionLog::new();
    sample(&mut log);
    let after = SystemTime::now();
    let t = log.decisions[0].timestamp;
    assert!(t >= before);
    assert!(t <= after);
}

#[test]
fn json_payload_compact_keys() {
    let mut log = DecisionLog::new();
    let id = log.add(vec!["a"], "a", "r", json!({"k": "v"}));
    let line = serde_json::to_string(&log.decisions[0]).unwrap();
    assert!(line.contains("\"id\":\""));
    assert!(line.contains(&id));
    assert!(line.contains("\"chosen\":\"a\""));
    assert!(line.contains("\"options\":[\"a\"]"));
}

#[test]
fn outcome_omitted_when_none() {
    let mut log = DecisionLog::new();
    log.add(vec!["a"], "a", "r", json!({}));
    let line = serde_json::to_string(&log.decisions[0]).unwrap();
    assert!(!line.contains("\"outcome\""));
}

#[test]
fn outcome_present_when_set() {
    let mut log = DecisionLog::new();
    let id = log.add(vec!["a"], "a", "r", json!({}));
    log.set_outcome(&id, "done");
    let line = serde_json::to_string(&log.decisions[0]).unwrap();
    assert!(line.contains("\"outcome\":\"done\""));
}

#[test]
fn empty_options_are_allowed() {
    let mut log = DecisionLog::new();
    log.add(Vec::<String>::new(), "fallback", "no options", json!({}));
    assert_eq!(log.last().unwrap().options.len(), 0);
    assert!(!log.last().unwrap().chose_listed_option());
}

#[test]
fn iter_matches_decisions_slice() {
    let mut log = DecisionLog::new();
    log.add(vec!["a"], "a", "r1", json!({}));
    log.add(vec!["b"], "b", "r2", json!({}));
    let via_iter: Vec<&str> = log.iter().map(|d| d.chosen.as_str()).collect();
    let via_ref: Vec<&str> = (&log).into_iter().map(|d| d.chosen.as_str()).collect();
    assert_eq!(via_iter, vec!["a", "b"]);
    assert_eq!(via_ref, vec!["a", "b"]);
}

#[test]
fn hallucinated_flags_off_menu_choices() {
    let mut log = DecisionLog::new();
    log.add(vec!["a", "b"], "a", "fine", json!({}));
    log.add(vec!["a", "b"], "z", "off menu", json!({}));
    log.add(Vec::<String>::new(), "empty", "no options", json!({}));
    let flagged: Vec<&str> = log.hallucinated().map(|d| d.chosen.as_str()).collect();
    assert_eq!(flagged, vec!["z", "empty"]);
}

#[test]
fn hallucinated_empty_when_all_choices_listed() {
    let mut log = DecisionLog::new();
    log.add(vec!["a", "b"], "a", "r", json!({}));
    log.add(vec!["c", "d"], "d", "r", json!({}));
    assert_eq!(log.hallucinated().count(), 0);
}

#[test]
fn pending_tracks_unresolved_decisions() {
    let mut log = DecisionLog::new();
    let id1 = log.add(vec!["a"], "a", "r", json!({}));
    let id2 = log.add(vec!["b"], "b", "r", json!({}));
    log.add(vec!["c"], "c", "r", json!({}));
    assert_eq!(log.pending().count(), 3);
    log.set_outcome(&id1, "done");
    log.set_outcome(&id2, "done");
    let still_pending: Vec<&str> = log.pending().map(|d| d.chosen.as_str()).collect();
    assert_eq!(still_pending, vec!["c"]);
}

#[test]
fn append_jsonl_creates_and_grows_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stream.jsonl");

    let mut first = DecisionLog::new();
    first.add(vec!["a"], "a", "r1", json!({"turn": 0}));
    first.append_jsonl(&path).unwrap();

    let mut second = DecisionLog::new();
    second.add(vec!["b"], "b", "r2", json!({"turn": 1}));
    second.append_jsonl(&path).unwrap();

    let loaded = DecisionLog::from_jsonl(&path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.decisions[0].chosen, "a");
    assert_eq!(loaded.decisions[1].chosen, "b");
}

#[test]
fn append_jsonl_on_fresh_path_equals_to_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let appended = dir.path().join("appended.jsonl");
    let written = dir.path().join("written.jsonl");

    let mut log = DecisionLog::new();
    log.add(vec!["a", "b"], "a", "r", json!({"k": "v"}));
    log.add(vec!["c"], "c", "r2", json!({}));

    log.append_jsonl(&appended).unwrap();
    log.to_jsonl(&written).unwrap();

    let a = std::fs::read_to_string(&appended).unwrap();
    let w = std::fs::read_to_string(&written).unwrap();
    assert_eq!(a, w);
}
