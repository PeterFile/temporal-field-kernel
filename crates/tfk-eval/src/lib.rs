use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tfk_protocol::{EventModality, EventSource, EvidenceStatus, RawEventInput};
use tfk_store::Store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaySummary {
    pub fixture_path: String,
    pub ingested_count: usize,
    pub hit_count: usize,
    pub ok: bool,
}

pub fn load_fixture_events(path: &Path) -> anyhow::Result<Vec<RawEventInput>> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "failed to read fixture line {} in {}",
                index + 1,
                path.display()
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let record: FixtureEventRecord = serde_json::from_str(trimmed)
            .with_context(|| format!("invalid fixture JSON at {}:{}", path.display(), index + 1))?;
        events.push(record.into());
    }

    Ok(events)
}

pub fn replay_fixture(path: &Path, query: &str) -> anyhow::Result<ReplaySummary> {
    let events = load_fixture_events(path)?;
    let tmp = tempfile::tempdir().context("failed to create replay temp directory")?;
    let data_dir = tmp.path().join("store");
    let store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive"))
        .context("failed to open replay store")?;

    for event in &events {
        store
            .append_raw_event(event)
            .context("failed to append fixture raw event")?;
    }

    let mut hit_count = 0;
    for id in store
        .search_raw_events(query)
        .context("failed to search replay store")?
    {
        if store
            .get_raw_event(&id)
            .context("failed to load replay search hit")?
            .is_some()
        {
            hit_count += 1;
        }
    }

    Ok(ReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        ingested_count: events.len(),
        hit_count,
        ok: hit_count > 0,
    })
}

#[derive(Debug, Deserialize)]
struct FixtureEventRecord {
    #[serde(default = "default_session_id")]
    session_id: String,
    #[serde(default = "default_adapter_id")]
    adapter_id: String,
    source: EventSource,
    #[serde(default = "default_modality")]
    modality: EventModality,
    content: String,
    #[serde(default)]
    act_type: Option<String>,
    #[serde(default = "default_evidence_status")]
    evidence_status: EvidenceStatus,
    #[serde(default)]
    time_utc: Option<String>,
}

impl From<FixtureEventRecord> for RawEventInput {
    fn from(record: FixtureEventRecord) -> Self {
        Self {
            session_id: record.session_id,
            adapter_id: record.adapter_id,
            source: record.source,
            modality: record.modality,
            content: record.content,
            act_type: record.act_type,
            evidence_status: record.evidence_status,
            time_utc: record.time_utc,
        }
    }
}

fn default_session_id() -> String {
    "temporalbench".to_string()
}

fn default_adapter_id() -> String {
    "temporalbench_fixture".to_string()
}

fn default_modality() -> EventModality {
    EventModality::Text
}

fn default_evidence_status() -> EvidenceStatus {
    EvidenceStatus::Observed
}
