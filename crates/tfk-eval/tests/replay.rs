use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use tfk_eval::{
    load_fixture_events, replay_action_loop_fixture, replay_fixture, replay_forecast_fixture,
    replay_lens_linked_raw_event_fixture,
};
use tfk_model_client::{ForecastPredictionClient, StaticForecastClient};
use tfk_protocol::{AdvisoryForecastSignal, EventSource, EvidenceStatus, ForecastRequest};
use tfk_store::Store;
use tfk_vector::{
    VectorDocument, VectorDocumentKind, VectorError, VectorHit, VectorIndex, VectorIndexOutcome,
    VectorIndexStatus,
};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/correction_persistence/events.jsonl")
}

fn forecast_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/forecast_advisory/basic_forecast.json")
}

fn action_loop_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/action_loop/commit_forecast_assimilate.json")
}

fn lens_linked_raw_event_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/lens_linked_raw_event/basic.json")
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

#[test]
fn forecast_fixture_replay_checks_expected_top_action_and_advisory_signal() {
    let summary = replay_forecast_fixture(&forecast_fixture_path()).unwrap();

    assert_eq!(summary.top_action, "verify then ship");
    assert_eq!(summary.expected_top_action, "verify then ship");
    assert_eq!(summary.advisory_signal_count, 1);
    assert_eq!(summary.advisory_signal_names, vec!["forming_future_risk"]);
    assert!(summary.ok);
}

#[test]
fn forecast_fixture_advisory_signals_roundtrip_through_static_model_client() {
    let value: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(forecast_fixture_path()).unwrap()).unwrap();
    let request = serde_json::from_value(value["request"].clone()).unwrap();
    let fixture_signals = serde_json::from_value(value["advisory_signals"].clone()).unwrap();
    let client = StaticForecastClient::new(fixture_signals);
    let signals = client.forecast(&request).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "forming_future_risk");
}

#[test]
fn vector_advisory_fixture_persists_with_noop_vector_fallback() {
    let (request, fixture_signals) = forecast_fixture_request_and_signals();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("store");
    let store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap();
    assert_eq!(
        store.vector_index_status(),
        VectorIndexStatus::unavailable("noop", "vector backend is not configured")
    );

    let client = StaticForecastClient::new(fixture_signals);
    let signals = client.forecast(&request).unwrap();
    let stored = store.record_advisory_forecast_signals(&signals).unwrap();

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "forming_future_risk");
    assert_eq!(stored[0].model, "fixture-static");
    assert_eq!(stored[0].action_name.as_deref(), Some("verify then ship"));
    assert_eq!(store.list_advisory_forecast_signals().unwrap(), stored);
}

#[test]
fn vector_advisory_fixture_indexes_signal_when_vector_index_is_injected() {
    let (request, fixture_signals) = forecast_fixture_request_and_signals();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("store");
    let index = Arc::new(RecordingVectorIndex::default());
    let store = Store::open_with_vector_index(
        data_dir.join("tfk.db"),
        data_dir.join("archive"),
        index.clone(),
    )
    .unwrap();
    assert_eq!(
        store.vector_index_status(),
        VectorIndexStatus::available("recording")
    );

    let client = StaticForecastClient::new(fixture_signals);
    let signals = client.forecast(&request).unwrap();
    let stored = store.record_advisory_forecast_signals(&signals).unwrap();

    let documents = index.documents.lock().unwrap();
    assert_eq!(documents.len(), stored.len());
    assert_eq!(documents[0].source_id, stored[0].id);
    assert_eq!(
        documents[0].kind,
        VectorDocumentKind::AdvisoryForecastSignal
    );
    assert!(documents[0].embedding.is_none());
    for needle in [
        "forming_future_risk",
        "fixture-static",
        "verify then ship",
        "unresolved risk",
    ] {
        assert!(documents[0].text.contains(needle), "missing {needle}");
    }
}

#[test]
fn vector_advisory_fixture_persists_when_vector_index_fails() {
    let (request, fixture_signals) = forecast_fixture_request_and_signals();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("store");
    let store = Store::open_with_vector_index(
        data_dir.join("tfk.db"),
        data_dir.join("archive"),
        Arc::new(FailingVectorIndex),
    )
    .unwrap();

    let client = StaticForecastClient::new(fixture_signals);
    let signals = client.forecast(&request).unwrap();
    let stored = store.record_advisory_forecast_signals(&signals).unwrap();

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "forming_future_risk");
    assert_eq!(store.list_advisory_forecast_signals().unwrap(), stored);
}

#[test]
fn forecast_replay_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "forecast",
            "--fixture",
            forecast_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["top_action"], "verify then ship");
    assert_eq!(value["expected_top_action"], "verify then ship");
    assert_eq!(value["advisory_signal_count"], 1);
    assert_eq!(value["advisory_signal_names"][0], "forming_future_risk");
    assert_eq!(value["ok"], true);
}

#[test]
fn action_loop_fixture_replay_checks_commit_forecast_assimilate_lens_closure() {
    let summary = replay_action_loop_fixture(&action_loop_fixture_path()).unwrap();

    assert_eq!(summary.commitment_constraint_count, 1);
    assert_eq!(summary.active_pressure_count_before_assimilate, 1);
    assert!(summary.preflight_requires_confirmation);
    assert_eq!(summary.forecast_top_action, "dry-run migration plan");
    assert!(summary.assimilation_action_matches_forecast);
    assert_eq!(summary.assimilated_status, "closed");
    assert_eq!(summary.reopened_status, "closed");
    assert_eq!(summary.commitment_constraint_count_after_assimilate, 0);
    assert_eq!(summary.active_pressure_count_after_assimilate, 0);
    assert!(summary.ok);
}

#[test]
fn action_loop_replay_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "action-loop",
            "--fixture",
            action_loop_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["commitment_constraint_count"], 1);
    assert_eq!(value["active_pressure_count_before_assimilate"], 1);
    assert_eq!(value["preflight_requires_confirmation"], true);
    assert_eq!(value["forecast_top_action"], "dry-run migration plan");
    assert_eq!(value["assimilation_action_matches_forecast"], true);
    assert_eq!(value["assimilated_status"], "closed");
    assert_eq!(value["reopened_status"], "closed");
    assert_eq!(value["commitment_constraint_count_after_assimilate"], 0);
    assert_eq!(value["active_pressure_count_after_assimilate"], 0);
    assert_eq!(value["ok"], true);
}

#[test]
fn lens_linked_raw_event_replay_promotes_active_link_then_falls_back_after_close() {
    let summary =
        replay_lens_linked_raw_event_fixture(&lens_linked_raw_event_fixture_path()).unwrap();

    assert_eq!(summary.raw_event_hit_count, 1);
    assert_eq!(summary.before_stance, "act");
    assert_eq!(summary.before_active_continuation_count, 1);
    assert_eq!(
        summary.before_active_continuation_titles,
        vec!["owner handoff"]
    );
    assert_eq!(summary.assimilated_status, "closed");
    assert_eq!(summary.after_stance, "grounded_recall");
    assert_eq!(summary.after_active_continuation_count, 0);
    assert!(summary.ok);
}

#[test]
fn lens_linked_raw_event_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "lens-linked-raw-event",
            "--fixture",
            lens_linked_raw_event_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["raw_event_hit_count"], 1);
    assert_eq!(value["before_stance"], "act");
    assert_eq!(value["before_active_continuation_count"], 1);
    assert_eq!(
        value["before_active_continuation_titles"][0],
        "owner handoff"
    );
    assert_eq!(value["assimilated_status"], "closed");
    assert_eq!(value["after_stance"], "grounded_recall");
    assert_eq!(value["after_active_continuation_count"], 0);
    assert_eq!(value["ok"], true);
}

fn forecast_fixture_request_and_signals() -> (ForecastRequest, Vec<AdvisoryForecastSignal>) {
    let value: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(forecast_fixture_path()).unwrap()).unwrap();
    let request = serde_json::from_value(value["request"].clone()).unwrap();
    let signals = serde_json::from_value(value["advisory_signals"].clone()).unwrap();
    (request, signals)
}

#[derive(Debug, Default)]
struct RecordingVectorIndex {
    documents: Mutex<Vec<VectorDocument>>,
}

#[derive(Debug)]
struct FailingVectorIndex;

impl VectorIndex for RecordingVectorIndex {
    fn status(&self) -> VectorIndexStatus {
        VectorIndexStatus::available("recording")
    }

    fn upsert(&self, document: &VectorDocument) -> tfk_vector::Result<VectorIndexOutcome> {
        self.documents.lock().unwrap().push(document.clone());
        Ok(VectorIndexOutcome::Indexed)
    }

    fn search(
        &self,
        _query_embedding: &[f32],
        _limit: usize,
    ) -> tfk_vector::Result<Vec<VectorHit>> {
        Ok(Vec::new())
    }
}

impl VectorIndex for FailingVectorIndex {
    fn status(&self) -> VectorIndexStatus {
        VectorIndexStatus::available("failing")
    }

    fn upsert(&self, _document: &VectorDocument) -> tfk_vector::Result<VectorIndexOutcome> {
        Err(VectorError::Backend("boom".to_string()))
    }

    fn search(
        &self,
        _query_embedding: &[f32],
        _limit: usize,
    ) -> tfk_vector::Result<Vec<VectorHit>> {
        Ok(Vec::new())
    }
}
