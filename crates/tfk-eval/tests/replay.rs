use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use tfk_eval::{
    load_fixture_events, replay_action_loop_fixture, replay_commitment_forecast_fixture,
    replay_fixture, replay_forecast_fixture, replay_lens_advisory_signal_fixture,
    replay_lens_linked_raw_event_fixture, replay_relation_boundary_fixture,
    replay_relation_ranking_fixture, replay_rules_lens_influence_fixture,
    replay_semantic_lens_influence_fixture, replay_vector_lens_influence_fixture,
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

fn commitment_consequence_choice_action_loop_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/action_loop/commitment_consequence_choice.json")
}

fn commitment_lifecycle_retire_action_loop_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/action_loop/commitment_lifecycle_retire.json")
}

fn commitment_defer_boundary_action_loop_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/action_loop/commitment_defer_boundary.json")
}

fn commitment_stabilize_boundary_action_loop_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/action_loop/commitment_stabilize_boundary.json")
}

fn lens_linked_raw_event_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/lens_linked_raw_event/basic.json")
}

fn lens_advisory_signal_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/lens_advisory_signal/basic.json")
}

fn relation_boundary_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/relation_boundary/basic.json")
}

fn relation_ranking_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/relation_ranking/basic.json")
}

fn semantic_lens_influence_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/semantic_lens_influence/basic.json")
}

fn semantic_lens_wildcard_literal_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/semantic_lens_influence/wildcard_literal.json")
}

fn semantic_lens_backslash_literal_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/semantic_lens_influence/backslash_literal.json")
}

fn rules_lens_influence_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/rules_lens_influence/review_now.json")
}

fn vector_lens_influence_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/vector_lens_influence/basic.json")
}

fn vector_lens_stale_hits_ignored_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/vector_lens_influence/stale_hits_ignored.json")
}

fn vector_lens_dedupe_and_distance_order_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/vector_lens_influence/dedupe_and_distance_order.json")
}

fn commitment_forecast_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/temporalbench/forecast_commitment/basic.json")
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
fn commitment_forecast_fixture_replay_scores_against_active_commitment() {
    let summary = replay_commitment_forecast_fixture(&commitment_forecast_fixture_path()).unwrap();

    assert_eq!(summary.commitment_constraint_count, 1);
    assert_eq!(summary.commitment_bound_action_count, 2);
    assert_eq!(summary.expected_top_action, "verify rollback evidence");
    assert_eq!(summary.actual_top_action, "verify rollback evidence");
    assert_eq!(summary.constrained_action, "ship irreversible release");
    assert!(summary.constrained_action_requires_confirmation);
    assert!(summary.ok);
}

#[test]
fn commitment_forecast_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "commitment-forecast",
            "--fixture",
            commitment_forecast_fixture_path().to_str().unwrap(),
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
    assert_eq!(value["commitment_bound_action_count"], 2);
    assert_eq!(value["expected_top_action"], "verify rollback evidence");
    assert_eq!(value["actual_top_action"], "verify rollback evidence");
    assert_eq!(value["constrained_action"], "ship irreversible release");
    assert_eq!(value["constrained_action_requires_confirmation"], true);
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
fn action_loop_consequence_choice_replay_keeps_commitment_active_after_verify() {
    let summary =
        replay_action_loop_fixture(&commitment_consequence_choice_action_loop_fixture_path())
            .unwrap();

    assert_eq!(summary.commitment_constraint_count, 1);
    assert_eq!(summary.active_pressure_count_before_assimilate, 1);
    assert!(summary.preflight_requires_confirmation);
    assert_eq!(summary.forecast_top_action, "verify rollback evidence");
    assert!(summary.assimilation_action_matches_forecast);
    assert_eq!(summary.assimilated_status, "active");
    assert_eq!(summary.reopened_status, "active");
    assert_eq!(summary.commitment_constraint_count_after_assimilate, 1);
    assert_eq!(summary.active_pressure_count_after_assimilate, 1);
    assert!(summary.ok);
}

#[test]
fn action_loop_lifecycle_retire_replay_releases_revocable_commitment() {
    let summary =
        replay_action_loop_fixture(&commitment_lifecycle_retire_action_loop_fixture_path())
            .unwrap();

    assert_eq!(summary.commitment_constraint_count, 1);
    assert_eq!(summary.active_pressure_count_before_assimilate, 1);
    assert!(!summary.preflight_requires_confirmation);
    assert_eq!(summary.forecast_top_action, "archive release cleanup note");
    assert!(summary.assimilation_action_matches_forecast);
    assert_eq!(summary.assimilated_status, "retired");
    assert_eq!(summary.reopened_status, "retired");
    assert_eq!(summary.commitment_constraint_count_after_assimilate, 0);
    assert_eq!(summary.active_pressure_count_after_assimilate, 0);
    assert!(summary.ok);
}

#[test]
fn action_loop_defer_boundary_replay_removes_active_commitment_constraint() {
    let summary =
        replay_action_loop_fixture(&commitment_defer_boundary_action_loop_fixture_path()).unwrap();

    assert_eq!(summary.commitment_constraint_count, 1);
    assert_eq!(summary.active_pressure_count_before_assimilate, 1);
    assert!(!summary.preflight_requires_confirmation);
    assert_eq!(summary.forecast_top_action, "schedule follow-up checkpoint");
    assert!(summary.assimilation_action_matches_forecast);
    assert_eq!(summary.assimilated_status, "deferred");
    assert_eq!(summary.reopened_status, "deferred");
    assert_eq!(summary.commitment_constraint_count_after_assimilate, 0);
    assert_eq!(summary.active_pressure_count_after_assimilate, 1);
    assert!(summary.ok);
}

#[test]
fn action_loop_stabilize_boundary_replay_removes_active_commitment_constraint() {
    let summary =
        replay_action_loop_fixture(&commitment_stabilize_boundary_action_loop_fixture_path())
            .unwrap();

    assert_eq!(summary.commitment_constraint_count, 1);
    assert_eq!(summary.active_pressure_count_before_assimilate, 1);
    assert!(!summary.preflight_requires_confirmation);
    assert_eq!(summary.forecast_top_action, "record stable operating rule");
    assert!(summary.assimilation_action_matches_forecast);
    assert_eq!(summary.assimilated_status, "stabilized");
    assert_eq!(summary.reopened_status, "stabilized");
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

#[test]
fn lens_advisory_signal_replay_projects_matching_signal_only() {
    let summary =
        replay_lens_advisory_signal_fixture(&lens_advisory_signal_fixture_path()).unwrap();

    assert_eq!(summary.continuation_count, 1);
    assert_eq!(
        summary.actual_active_continuation_titles,
        vec!["irreversible release decision"]
    );
    assert_eq!(
        summary.expected_advisory_signal_names,
        vec!["rollback_evidence_gap"]
    );
    assert_eq!(
        summary.actual_advisory_signal_names,
        vec!["rollback_evidence_gap"]
    );
    assert_eq!(summary.advisory_signal_count, 1);
    assert!(summary.ok);
}

#[test]
fn lens_advisory_signal_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "lens-advisory-signal",
            "--fixture",
            lens_advisory_signal_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["continuation_count"], 1);
    assert_eq!(
        value["actual_active_continuation_titles"][0],
        "irreversible release decision"
    );
    assert_eq!(
        value["actual_advisory_signal_names"][0],
        "rollback_evidence_gap"
    );
    assert_eq!(value["advisory_signal_count"], 1);
    assert_eq!(value["ok"], true);
}

#[test]
fn relation_boundary_replay_emits_relation_block_then_clears_after_close() {
    let summary = replay_relation_boundary_fixture(&relation_boundary_fixture_path()).unwrap();

    assert_eq!(summary.before_stance, "verify");
    assert_eq!(summary.before_boundary_kinds, vec!["relation_block"]);
    assert_eq!(summary.before_active_continuation_count, 2);
    assert_eq!(summary.assimilated_status, "closed");
    assert_eq!(summary.after_boundary_kinds, Vec::<String>::new());
    assert_eq!(summary.after_active_continuation_count, 1);
    assert_eq!(summary.after_stance, "act");
    assert!(summary.ok);
}

#[test]
fn relation_boundary_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "relation-boundary",
            "--fixture",
            relation_boundary_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["before_stance"], "verify");
    assert_eq!(value["before_boundary_kinds"][0], "relation_block");
    assert_eq!(value["before_active_continuation_count"], 2);
    assert_eq!(value["assimilated_status"], "closed");
    assert_eq!(value["after_boundary_kinds"].as_array().unwrap().len(), 0);
    assert_eq!(value["after_active_continuation_count"], 1);
    assert_eq!(value["after_stance"], "act");
    assert_eq!(value["ok"], true);
}

#[test]
fn relation_ranking_replay_checks_relation_kind_weighted_order() {
    let summary = replay_relation_ranking_fixture(&relation_ranking_fixture_path()).unwrap();

    let expected_titles = vec![
        "relation ranking prerequisite action".to_string(),
        "relation ranking umbrella action".to_string(),
        "relation ranking supported action".to_string(),
        "relation ranking baseline action".to_string(),
        "relation ranking child action".to_string(),
        "relation ranking dependent action".to_string(),
    ];
    assert_eq!(summary.continuation_count, 6);
    assert_eq!(summary.relation_count, 4);
    assert_eq!(summary.expected_top_title, expected_titles[0]);
    assert_eq!(summary.actual_top_title, expected_titles[0]);
    assert_eq!(summary.expected_ordered_titles, expected_titles);
    assert_eq!(summary.actual_ordered_titles, expected_titles);
    assert_eq!(summary.actual_ordered_ids.len(), 6);
    assert!(summary
        .actual_ordered_ids
        .iter()
        .all(|id| id.starts_with("cont_")));
    assert!(summary.ok);
}

#[test]
fn relation_ranking_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "relation-ranking",
            "--fixture",
            relation_ranking_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["continuation_count"], 6);
    assert_eq!(value["relation_count"], 4);
    assert_eq!(
        value["expected_top_title"],
        "relation ranking prerequisite action"
    );
    assert_eq!(
        value["actual_top_title"],
        "relation ranking prerequisite action"
    );
    assert_eq!(value["actual_ordered_titles"].as_array().unwrap().len(), 6);
    assert_eq!(
        value["actual_ordered_titles"][1],
        "relation ranking umbrella action"
    );
    assert_eq!(
        value["actual_ordered_titles"][2],
        "relation ranking supported action"
    );
    assert!(value["actual_top_id"]
        .as_str()
        .unwrap()
        .starts_with("cont_"));
    assert_eq!(value["actual_ordered_ids"].as_array().unwrap().len(), 6);
    assert_eq!(value["ok"], true);
}

#[test]
fn semantic_lens_influence_replay_ranks_exact_phrase_before_distributed_overlap() {
    let summary =
        replay_semantic_lens_influence_fixture(&semantic_lens_influence_fixture_path()).unwrap();

    assert_eq!(summary.continuation_count, 4);
    assert_eq!(summary.expected_top_title, "rollback gate");
    assert_eq!(summary.actual_top_title, "rollback gate");
    assert_eq!(
        summary.actual_ordered_titles,
        vec![
            "rollback gate".to_string(),
            "semantic lens rollback target".to_string(),
        ]
    );
    assert!(summary.ok);
}

#[test]
fn semantic_lens_influence_replay_treats_wildcard_query_as_literal() {
    let summary =
        replay_semantic_lens_influence_fixture(&semantic_lens_wildcard_literal_fixture_path())
            .unwrap();

    assert_eq!(summary.continuation_count, 4);
    assert_eq!(summary.expected_top_title, "100%_literal");
    assert_eq!(summary.actual_top_title, "100%_literal");
    assert_eq!(
        summary.actual_ordered_titles,
        vec!["100%_literal".to_string()]
    );
    assert!(summary.ok);
}

#[test]
fn semantic_lens_influence_replay_treats_backslash_query_as_literal() {
    let summary =
        replay_semantic_lens_influence_fixture(&semantic_lens_backslash_literal_fixture_path())
            .unwrap();

    assert_eq!(summary.continuation_count, 4);
    assert_eq!(summary.expected_top_title, r"100\literal");
    assert_eq!(summary.actual_top_title, r"100\literal");
    assert_eq!(
        summary.actual_ordered_titles,
        vec![r"100\literal".to_string()]
    );
    assert!(summary.ok);
}

#[test]
fn semantic_lens_influence_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "semantic-lens-influence",
            "--fixture",
            semantic_lens_influence_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["continuation_count"], 4);
    assert_eq!(value["actual_top_title"], "rollback gate");
    assert_eq!(value["actual_ordered_titles"].as_array().unwrap().len(), 2);
    assert_eq!(value["actual_ordered_titles"][0], "rollback gate");
    assert_eq!(
        value["actual_ordered_titles"][1],
        "semantic lens rollback target"
    );
    assert_eq!(value["ok"], true);
}

#[test]
fn rules_lens_influence_replay_promotes_rule_derived_review_target() {
    let summary =
        replay_rules_lens_influence_fixture(&rules_lens_influence_fixture_path()).unwrap();

    assert_eq!(summary.continuation_count, 2);
    assert_eq!(summary.expected_top_title, "rules influence review target");
    assert_eq!(summary.actual_top_title, "rules influence review target");
    assert_eq!(
        summary.actual_ordered_titles,
        vec![
            "rules influence review target".to_string(),
            "rules influence baseline risk".to_string(),
        ]
    );
    assert_eq!(
        summary.actual_rule_fact_predicates,
        vec![
            "needs_review".to_string(),
            "path_choice".to_string(),
            "risk_marker".to_string(),
            "timing_attention".to_string(),
        ]
    );
    assert!(summary
        .rule_fact_ids
        .iter()
        .any(|id| id.contains("path_choice") && id.contains("review_now")));
    assert!(summary.ok);
}

#[test]
fn rules_lens_influence_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "rules-lens-influence",
            "--fixture",
            rules_lens_influence_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["continuation_count"], 2);
    assert_eq!(value["actual_top_title"], "rules influence review target");
    assert_eq!(
        value["actual_rule_fact_predicates"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(value["ok"], true);
}

#[test]
fn vector_lens_influence_replay_promotes_fake_vector_hit_over_lexical_baseline() {
    let summary =
        replay_vector_lens_influence_fixture(&vector_lens_influence_fixture_path()).unwrap();

    assert_eq!(summary.continuation_count, 2);
    assert_eq!(summary.vector_hit_count, 1);
    assert_eq!(summary.expected_top_title, "latent release safety boundary");
    assert_eq!(summary.actual_top_title, "latent release safety boundary");
    assert_eq!(summary.expected_top_source, "vector");
    assert_eq!(summary.actual_top_source, "vector");
    assert_eq!(
        summary.actual_ordered_titles,
        vec![
            "latent release safety boundary".to_string(),
            "lexical beacon baseline".to_string(),
        ]
    );
    assert_eq!(summary.actual_vector_hit_labels, vec!["vector_target"]);
    assert!(summary.ok);
}

#[test]
fn vector_lens_influence_replay_ignores_stale_vector_hits() {
    let summary =
        replay_vector_lens_influence_fixture(&vector_lens_stale_hits_ignored_fixture_path())
            .unwrap();

    assert_eq!(summary.continuation_count, 4);
    assert_eq!(summary.vector_hit_count, 1);
    assert_eq!(summary.expected_top_title, "active vector boundary");
    assert_eq!(summary.actual_top_title, "active vector boundary");
    assert_eq!(summary.expected_top_source, "vector");
    assert_eq!(summary.actual_top_source, "vector");
    assert_eq!(
        summary.actual_ordered_titles,
        vec![
            "active vector boundary".to_string(),
            "lexical stale baseline".to_string(),
        ]
    );
    assert_eq!(summary.actual_vector_hit_labels, vec!["active_target"]);
    assert!(summary.ok);
}

#[test]
fn vector_lens_influence_replay_dedupes_hits_and_orders_by_distance() {
    let summary =
        replay_vector_lens_influence_fixture(&vector_lens_dedupe_and_distance_order_fixture_path())
            .unwrap();

    assert_eq!(summary.continuation_count, 3);
    assert_eq!(summary.vector_hit_count, 2);
    assert_eq!(summary.expected_top_title, "near vector target");
    assert_eq!(summary.actual_top_title, "near vector target");
    assert_eq!(summary.expected_top_source, "vector");
    assert_eq!(summary.actual_top_source, "vector");
    assert_eq!(
        summary.actual_ordered_titles,
        vec![
            "near vector target".to_string(),
            "far vector target".to_string(),
            "lexical distance baseline".to_string(),
        ]
    );
    assert_eq!(summary.actual_vector_hit_labels.len(), 2);
    assert!(summary
        .actual_vector_hit_labels
        .iter()
        .any(|label| label == "far_target"));
    assert!(summary
        .actual_vector_hit_labels
        .iter()
        .any(|label| label == "near_target"));
    assert!(summary.ok);
}

#[test]
fn vector_lens_influence_cli_prints_structured_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_tfk-eval"))
        .args([
            "vector-lens-influence",
            "--fixture",
            vector_lens_influence_fixture_path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["continuation_count"], 2);
    assert_eq!(value["vector_hit_count"], 1);
    assert_eq!(value["actual_top_title"], "latent release safety boundary");
    assert_eq!(value["actual_top_source"], "vector");
    assert_eq!(value["actual_vector_hit_labels"][0], "vector_target");
    assert_eq!(value["actual_ordered_titles"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["actual_ordered_titles"][0],
        "latent release safety boundary"
    );
    assert_eq!(value["actual_ordered_titles"][1], "lexical beacon baseline");
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
