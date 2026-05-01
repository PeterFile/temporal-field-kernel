use std::path::PathBuf;
use std::process::Command;

use tfk_eval::{load_fixture_events, replay_fixture};
use tfk_protocol::{EventSource, EvidenceStatus};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/correction_persistence/events.jsonl")
}

#[test]
fn parses_minimal_temporalbench_jsonl_fixture() {
    let events = load_fixture_events(&fixture_path()).unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].session_id, "temporalbench");
    assert_eq!(events[0].adapter_id, "temporalbench_fixture");
    assert_eq!(events[0].source, EventSource::User);
    assert_eq!(events[0].evidence_status, EvidenceStatus::Observed);
    assert_eq!(events[0].content, "不要把这个设计成项目状态机");
}

#[test]
fn replay_summary_reports_local_query_hits() {
    let summary = replay_fixture(&fixture_path(), "continuation").unwrap();

    assert_eq!(summary.ingested_count, 2);
    assert_eq!(summary.hit_count, 1);
    assert!(summary.ok);
}

#[test]
fn replay_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "replay",
            "--fixture",
            fixture_path().to_str().unwrap(),
            "--query",
            "continuation",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["fixture_path"],
        fixture_path().to_string_lossy().as_ref()
    );
    assert_eq!(value["ingested_count"], 2);
    assert_eq!(value["hit_count"], 1);
    assert_eq!(value["ok"], true);
}
