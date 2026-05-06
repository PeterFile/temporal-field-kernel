use std::path::PathBuf;
use std::process::Command;

use tfk_eval::{
    load_fixture_events, replay_action_loop_fixture, replay_fixture, replay_forecast_fixture,
};
use tfk_model_client::{ForecastPredictionClient, StaticForecastClient};
use tfk_protocol::{EventSource, EvidenceStatus};

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
