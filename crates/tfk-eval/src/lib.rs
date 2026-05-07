use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tfk_core::{ForecastScorer, PreflightScorer, TimeFieldContinuation, TimeFieldLensEngine};
use tfk_model_client::{ForecastPredictionClient, StaticForecastClient};
use tfk_protocol::{
    AdvisoryForecastSignal, CommitRequest, ContinuationDelta, ContinuationRelationEdge,
    ContinuationRelationKind, ContinuationStatus, ContinuationStatusDelta, ContinuationType,
    EventModality, EventSource, EvidenceStatus, ForecastRequest, LensAdvisoryForecastSignal,
    LensCard, LensRequest, PreflightSignals, RawEventInput, TemporalDeltaInput,
};
use tfk_store::Store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaySummary {
    pub fixture_path: String,
    pub ingested_count: usize,
    pub hit_count: usize,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastReplaySummary {
    pub fixture_path: String,
    pub top_action: String,
    pub expected_top_action: String,
    pub advisory_signal_count: usize,
    pub advisory_signal_names: Vec<String>,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionLoopReplaySummary {
    pub fixture_path: String,
    pub commitment_constraint_count: usize,
    pub active_pressure_count_before_assimilate: usize,
    pub preflight_requires_confirmation: bool,
    pub forecast_top_action: String,
    pub assimilation_action_matches_forecast: bool,
    pub assimilated_status: String,
    pub reopened_status: String,
    pub commitment_constraint_count_after_assimilate: usize,
    pub active_pressure_count_after_assimilate: usize,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LensLinkedRawEventReplaySummary {
    pub fixture_path: String,
    pub raw_event_hit_count: usize,
    pub before_stance: String,
    pub before_active_continuation_count: usize,
    pub before_active_continuation_titles: Vec<String>,
    pub assimilated_status: String,
    pub after_stance: String,
    pub after_active_continuation_count: usize,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationBoundaryReplaySummary {
    pub fixture_path: String,
    pub before_stance: String,
    pub before_boundary_kinds: Vec<String>,
    pub before_active_continuation_count: usize,
    pub assimilated_status: String,
    pub after_boundary_kinds: Vec<String>,
    pub after_active_continuation_count: usize,
    pub after_stance: String,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationRankingReplaySummary {
    pub fixture_path: String,
    pub continuation_count: usize,
    pub relation_count: usize,
    pub expected_top_title: String,
    pub actual_top_title: String,
    pub actual_top_id: String,
    pub expected_ordered_titles: Vec<String>,
    pub actual_ordered_ids: Vec<String>,
    pub actual_ordered_titles: Vec<String>,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LensAdvisorySignalReplaySummary {
    pub fixture_path: String,
    pub continuation_count: usize,
    pub expected_active_continuation_titles: Vec<String>,
    pub actual_active_continuation_titles: Vec<String>,
    pub expected_advisory_signal_names: Vec<String>,
    pub actual_advisory_signal_names: Vec<String>,
    pub advisory_signal_count: usize,
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

pub fn replay_forecast_fixture(path: &Path) -> anyhow::Result<ForecastReplaySummary> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let fixture: ForecastFixture = serde_json::from_reader(file)
        .with_context(|| format!("invalid fixture {}", path.display()))?;
    let mut result = ForecastScorer.score(&fixture.request);
    let client = StaticForecastClient::new(fixture.advisory_signals);
    let signals = client
        .forecast(&fixture.request)
        .context("failed to run forecast advisory model client")?;
    result.advisory_signals.extend(signals);

    let top_action = result
        .ranked_actions
        .first()
        .map(|action| action.name.clone())
        .unwrap_or_default();
    let advisory_signal_names: Vec<_> = result
        .advisory_signals
        .iter()
        .map(|signal| signal.name.clone())
        .collect();
    let expected_count = fixture
        .expected_advisory_signal_count
        .unwrap_or(advisory_signal_names.len());
    let expected_names = if fixture.expected_advisory_signal_names.is_empty() {
        advisory_signal_names.clone()
    } else {
        fixture.expected_advisory_signal_names
    };
    let ok = top_action == fixture.expected_top_action
        && advisory_signal_names.len() == expected_count
        && advisory_signal_names == expected_names;

    Ok(ForecastReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        top_action,
        expected_top_action: fixture.expected_top_action,
        advisory_signal_count: advisory_signal_names.len(),
        advisory_signal_names,
        ok,
    })
}

pub fn replay_action_loop_fixture(path: &Path) -> anyhow::Result<ActionLoopReplaySummary> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let fixture: ActionLoopFixture = serde_json::from_reader(file)
        .with_context(|| format!("invalid fixture {}", path.display()))?;
    let tmp = tempfile::tempdir().context("failed to create action-loop temp directory")?;
    let data_dir = tmp.path().join("store");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let mut store =
        Store::open(&db_path, &archive_dir).context("failed to open action-loop store")?;

    let continuation = store
        .create_continuation(&continuation_input_from_commitment(&fixture.commitment))
        .context("failed to create action-loop continuation")?;
    let _commitment = store
        .create_commitment(&fixture.commitment, &continuation.id)
        .context("failed to create action-loop commitment")?;

    let before =
        lens_from_store(&store, &fixture.lens).context("failed to run pre-assimilate lens")?;
    let preflight =
        PreflightScorer::with_threshold(fixture.preflight_threshold).score(fixture.preflight);
    let forecast = ForecastScorer.score(&fixture.forecast);
    let forecast_top_action = forecast
        .ranked_actions
        .first()
        .map(|action| action.name.clone())
        .unwrap_or_default();
    let assimilation_action_matches_forecast =
        fixture.assimilation.action_id == forecast_top_action;

    store
        .assimilate_delta(&TemporalDeltaInput {
            action_id: fixture.assimilation.action_id,
            changes: vec![ContinuationStatusDelta {
                continuation_id: continuation.id.clone(),
                delta: fixture.assimilation.delta,
            }],
            claims_made: Vec::new(),
            evidence: Vec::new(),
        })
        .context("failed to assimilate action-loop delta")?;
    let assimilated_status = store
        .get_continuation(&continuation.id)
        .context("failed to read assimilated continuation")?
        .map(|stored| status_name(stored.status).to_string())
        .unwrap_or_default();

    drop(store);
    let reopened =
        Store::open(&db_path, &archive_dir).context("failed to reopen action-loop store")?;
    let reopened_status = reopened
        .get_continuation(&continuation.id)
        .context("failed to read reopened continuation")?
        .map(|stored| status_name(stored.status).to_string())
        .unwrap_or_default();
    let after =
        lens_from_store(&reopened, &fixture.lens).context("failed to run post-assimilate lens")?;

    let ok = before.commitment_constraints.len() == fixture.expected.commitment_constraint_count
        && before.active_continuations.len()
            == fixture.expected.active_pressure_count_before_assimilate
        && preflight.requires_confirmation == fixture.expected.preflight_requires_confirmation
        && forecast_top_action == fixture.expected.forecast_top_action
        && assimilation_action_matches_forecast
        && assimilated_status == fixture.expected.assimilated_status
        && reopened_status == fixture.expected.reopened_status
        && after.commitment_constraints.len()
            == fixture
                .expected
                .commitment_constraint_count_after_assimilate
        && after.active_continuations.len()
            == fixture.expected.active_pressure_count_after_assimilate;

    Ok(ActionLoopReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        commitment_constraint_count: before.commitment_constraints.len(),
        active_pressure_count_before_assimilate: before.active_continuations.len(),
        preflight_requires_confirmation: preflight.requires_confirmation,
        forecast_top_action,
        assimilation_action_matches_forecast,
        assimilated_status,
        reopened_status,
        commitment_constraint_count_after_assimilate: after.commitment_constraints.len(),
        active_pressure_count_after_assimilate: after.active_continuations.len(),
        ok,
    })
}

pub fn replay_lens_linked_raw_event_fixture(
    path: &Path,
) -> anyhow::Result<LensLinkedRawEventReplaySummary> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let fixture: LensLinkedRawEventFixture = serde_json::from_reader(file)
        .with_context(|| format!("invalid fixture {}", path.display()))?;
    let tmp = tempfile::tempdir().context("failed to create lens-linked temp directory")?;
    let data_dir = tmp.path().join("store");
    let mut store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive"))
        .context("failed to open lens-linked store")?;

    let raw_event = store
        .append_raw_event(&fixture.raw_event.into())
        .context("failed to append linked raw event")?;
    let mut continuation_input = fixture.continuation;
    continuation_input.raw_event_id = Some(raw_event.id.clone());
    let continuation = store
        .create_continuation(&continuation_input)
        .context("failed to create linked continuation")?;

    let raw_event_hit_count = store
        .search_raw_events(&fixture.lens.query)
        .context("failed to search linked raw events")?
        .len();
    let before = lens_from_store(&store, &fixture.lens)
        .context("failed to run pre-assimilate linked lens")?;

    store
        .assimilate_delta(&TemporalDeltaInput {
            action_id: fixture.assimilation.action_id,
            changes: vec![ContinuationStatusDelta {
                continuation_id: continuation.id.clone(),
                delta: fixture.assimilation.delta,
            }],
            claims_made: Vec::new(),
            evidence: Vec::new(),
        })
        .context("failed to close linked continuation")?;
    let assimilated_status = store
        .get_continuation(&continuation.id)
        .context("failed to read closed linked continuation")?
        .map(|stored| status_name(stored.status).to_string())
        .unwrap_or_default();

    let after = lens_from_store(&store, &fixture.lens)
        .context("failed to run post-assimilate linked lens")?;
    let before_active_continuation_titles: Vec<_> = before
        .active_continuations
        .iter()
        .map(|continuation| continuation.title.clone())
        .collect();

    let ok = raw_event_hit_count == fixture.expected.raw_event_hit_count
        && before.stance == fixture.expected.before_stance
        && before.active_continuations.len() == fixture.expected.before_active_continuation_count
        && before_active_continuation_titles == fixture.expected.before_active_continuation_titles
        && assimilated_status == fixture.expected.assimilated_status
        && after.stance == fixture.expected.after_stance
        && after.active_continuations.len() == fixture.expected.after_active_continuation_count;

    Ok(LensLinkedRawEventReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        raw_event_hit_count,
        before_stance: before.stance,
        before_active_continuation_count: before.active_continuations.len(),
        before_active_continuation_titles,
        assimilated_status,
        after_stance: after.stance,
        after_active_continuation_count: after.active_continuations.len(),
        ok,
    })
}

pub fn replay_relation_boundary_fixture(
    path: &Path,
) -> anyhow::Result<RelationBoundaryReplaySummary> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let fixture: RelationBoundaryFixture = serde_json::from_reader(file)
        .with_context(|| format!("invalid fixture {}", path.display()))?;
    let tmp = tempfile::tempdir().context("failed to create relation-boundary temp directory")?;
    let data_dir = tmp.path().join("store");
    let mut store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive"))
        .context("failed to open relation-boundary store")?;

    let mut continuation_ids = HashMap::new();
    for continuation in fixture.continuations {
        let stored = store
            .create_continuation(&continuation.input)
            .with_context(|| format!("failed to create continuation {}", continuation.label))?;
        continuation_ids.insert(continuation.label, stored.id);
    }

    let from_id = continuation_ids
        .get(&fixture.relation.from)
        .with_context(|| format!("unknown relation from label {}", fixture.relation.from))?
        .clone();
    let to_id = continuation_ids
        .get(&fixture.relation.to)
        .with_context(|| format!("unknown relation to label {}", fixture.relation.to))?
        .clone();
    store
        .create_continuation_relation(&ContinuationRelationEdge {
            from_id,
            to_id,
            kind: fixture.relation.kind,
            reason: fixture.relation.reason,
        })
        .context("failed to create continuation relation")?;

    let before = lens_from_store(&store, &fixture.lens)
        .context("failed to run pre-assimilate relation-boundary lens")?;
    let target_id = continuation_ids
        .get(&fixture.assimilation.target)
        .with_context(|| {
            format!(
                "unknown assimilation target label {}",
                fixture.assimilation.target
            )
        })?
        .clone();
    store
        .assimilate_delta(&TemporalDeltaInput {
            action_id: fixture.assimilation.action_id,
            changes: vec![ContinuationStatusDelta {
                continuation_id: target_id.clone(),
                delta: fixture.assimilation.delta,
            }],
            claims_made: Vec::new(),
            evidence: Vec::new(),
        })
        .context("failed to assimilate relation-boundary delta")?;
    let assimilated_status = store
        .get_continuation(&target_id)
        .context("failed to read assimilated relation-boundary continuation")?
        .map(|stored| status_name(stored.status).to_string())
        .unwrap_or_default();
    let after = lens_from_store(&store, &fixture.lens)
        .context("failed to run post-assimilate relation-boundary lens")?;

    let before_boundary_kinds = boundary_kinds(&before);
    let after_boundary_kinds = boundary_kinds(&after);
    let ok = before.stance == fixture.expected.before_stance
        && before_boundary_kinds == fixture.expected.before_boundary_kinds
        && before.active_continuations.len() == fixture.expected.before_active_continuation_count
        && assimilated_status == fixture.expected.assimilated_status
        && after_boundary_kinds == fixture.expected.after_boundary_kinds
        && after.active_continuations.len() == fixture.expected.after_active_continuation_count
        && after.stance == fixture.expected.after_stance;

    Ok(RelationBoundaryReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        before_stance: before.stance,
        before_boundary_kinds,
        before_active_continuation_count: before.active_continuations.len(),
        assimilated_status,
        after_boundary_kinds,
        after_active_continuation_count: after.active_continuations.len(),
        after_stance: after.stance,
        ok,
    })
}

pub fn replay_relation_ranking_fixture(
    path: &Path,
) -> anyhow::Result<RelationRankingReplaySummary> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let fixture: RelationRankingFixture = serde_json::from_reader(file)
        .with_context(|| format!("invalid fixture {}", path.display()))?;
    let tmp = tempfile::tempdir().context("failed to create relation-ranking temp directory")?;
    let data_dir = tmp.path().join("store");
    let store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive"))
        .context("failed to open relation-ranking store")?;

    let continuation_count = fixture.continuations.len();
    let relation_count = fixture.relations.len();
    let mut continuation_ids = HashMap::new();
    for continuation in &fixture.continuations {
        let stored = store
            .create_continuation(&continuation.input)
            .with_context(|| format!("failed to create continuation {}", continuation.label))?;
        continuation_ids.insert(continuation.label.clone(), stored.id);
    }

    for relation in &fixture.relations {
        let from_id = continuation_ids
            .get(&relation.from)
            .with_context(|| format!("unknown relation from label {}", relation.from))?
            .clone();
        let to_id = continuation_ids
            .get(&relation.to)
            .with_context(|| format!("unknown relation to label {}", relation.to))?
            .clone();
        store
            .create_continuation_relation(&ContinuationRelationEdge {
                from_id,
                to_id,
                kind: relation.kind,
                reason: relation.reason.clone(),
            })
            .context("failed to create continuation relation")?;
    }

    let card =
        lens_from_store(&store, &fixture.lens).context("failed to run relation-ranking lens")?;
    let actual_ordered_ids: Vec<_> = card
        .active_continuations
        .iter()
        .map(|continuation| continuation.id.clone())
        .collect();
    let actual_ordered_titles: Vec<_> = card
        .active_continuations
        .iter()
        .map(|continuation| continuation.title.clone())
        .collect();
    let actual_top_title = actual_ordered_titles.first().cloned().unwrap_or_default();
    let actual_top_id = actual_ordered_ids.first().cloned().unwrap_or_default();
    let expected_top_title = fixture.expected.top_title;
    let expected_ordered_titles = fixture.expected.ordered_titles;
    let ok =
        actual_top_title == expected_top_title && actual_ordered_titles == expected_ordered_titles;

    Ok(RelationRankingReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        continuation_count,
        relation_count,
        expected_top_title,
        actual_top_title,
        actual_top_id,
        expected_ordered_titles,
        actual_ordered_ids,
        actual_ordered_titles,
        ok,
    })
}

pub fn replay_lens_advisory_signal_fixture(
    path: &Path,
) -> anyhow::Result<LensAdvisorySignalReplaySummary> {
    let file =
        File::open(path).with_context(|| format!("failed to open fixture {}", path.display()))?;
    let fixture: LensAdvisorySignalFixture = serde_json::from_reader(file)
        .with_context(|| format!("invalid fixture {}", path.display()))?;
    let tmp = tempfile::tempdir().context("failed to create lens-advisory temp directory")?;
    let data_dir = tmp.path().join("store");
    let store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive"))
        .context("failed to open lens-advisory store")?;

    let continuation_count = fixture.continuations.len();
    for continuation in &fixture.continuations {
        store
            .create_continuation(&continuation.input)
            .with_context(|| format!("failed to create continuation {}", continuation.label))?;
    }
    store
        .record_advisory_forecast_signals(&fixture.advisory_signals)
        .context("failed to persist advisory forecast signals")?;

    let card = lens_from_store(&store, &fixture.lens)
        .context("failed to run lens advisory signal projection")?;
    let actual_active_continuation_titles: Vec<_> = card
        .active_continuations
        .iter()
        .map(|continuation| continuation.title.clone())
        .collect();
    let actual_advisory_signal_names: Vec<_> = card
        .advisory_forecast_signals
        .iter()
        .map(|signal| signal.name.clone())
        .collect();
    let advisory_signal_count = actual_advisory_signal_names.len();
    let expected_active_continuation_titles = fixture.expected.active_continuation_titles;
    let expected_advisory_signal_names = fixture.expected.advisory_signal_names;
    let ok = actual_active_continuation_titles == expected_active_continuation_titles
        && actual_advisory_signal_names == expected_advisory_signal_names;

    Ok(LensAdvisorySignalReplaySummary {
        fixture_path: path.to_string_lossy().to_string(),
        continuation_count,
        expected_active_continuation_titles,
        actual_active_continuation_titles,
        expected_advisory_signal_names,
        actual_advisory_signal_names,
        advisory_signal_count,
        ok,
    })
}

#[derive(Debug, Deserialize)]
struct ForecastFixture {
    request: ForecastRequest,
    expected_top_action: String,
    #[serde(default)]
    advisory_signals: Vec<AdvisoryForecastSignal>,
    #[serde(default)]
    expected_advisory_signal_count: Option<usize>,
    #[serde(default)]
    expected_advisory_signal_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ActionLoopFixture {
    commitment: CommitRequest,
    lens: LensRequest,
    preflight: PreflightSignals,
    #[serde(default = "default_preflight_threshold")]
    preflight_threshold: f64,
    forecast: ForecastRequest,
    assimilation: ActionLoopAssimilation,
    expected: ActionLoopExpected,
}

#[derive(Debug, Deserialize)]
struct ActionLoopAssimilation {
    action_id: String,
    delta: ContinuationDelta,
}

#[derive(Debug, Deserialize)]
struct ActionLoopExpected {
    commitment_constraint_count: usize,
    active_pressure_count_before_assimilate: usize,
    preflight_requires_confirmation: bool,
    forecast_top_action: String,
    assimilated_status: String,
    reopened_status: String,
    commitment_constraint_count_after_assimilate: usize,
    active_pressure_count_after_assimilate: usize,
}

#[derive(Debug, Deserialize)]
struct LensLinkedRawEventFixture {
    raw_event: FixtureEventRecord,
    continuation: tfk_protocol::ContinuationInput,
    lens: LensRequest,
    assimilation: ActionLoopAssimilation,
    expected: LensLinkedRawEventExpected,
}

#[derive(Debug, Deserialize)]
struct LensLinkedRawEventExpected {
    raw_event_hit_count: usize,
    before_stance: String,
    before_active_continuation_count: usize,
    before_active_continuation_titles: Vec<String>,
    assimilated_status: String,
    after_stance: String,
    after_active_continuation_count: usize,
}

#[derive(Debug, Deserialize)]
struct RelationBoundaryFixture {
    continuations: Vec<RelationBoundaryContinuation>,
    relation: RelationBoundaryRelation,
    lens: LensRequest,
    assimilation: RelationBoundaryAssimilation,
    expected: RelationBoundaryExpected,
}

#[derive(Debug, Deserialize)]
struct RelationBoundaryContinuation {
    label: String,
    input: tfk_protocol::ContinuationInput,
}

#[derive(Debug, Deserialize)]
struct RelationBoundaryRelation {
    from: String,
    to: String,
    kind: ContinuationRelationKind,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelationBoundaryAssimilation {
    target: String,
    action_id: String,
    delta: ContinuationDelta,
}

#[derive(Debug, Deserialize)]
struct RelationBoundaryExpected {
    before_stance: String,
    before_boundary_kinds: Vec<String>,
    before_active_continuation_count: usize,
    assimilated_status: String,
    after_boundary_kinds: Vec<String>,
    after_active_continuation_count: usize,
    after_stance: String,
}

#[derive(Debug, Deserialize)]
struct RelationRankingFixture {
    continuations: Vec<RelationRankingContinuation>,
    relations: Vec<RelationRankingRelation>,
    lens: LensRequest,
    expected: RelationRankingExpected,
}

#[derive(Debug, Deserialize)]
struct RelationRankingContinuation {
    label: String,
    input: tfk_protocol::ContinuationInput,
}

#[derive(Debug, Deserialize)]
struct RelationRankingRelation {
    from: String,
    to: String,
    kind: ContinuationRelationKind,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelationRankingExpected {
    top_title: String,
    ordered_titles: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LensAdvisorySignalFixture {
    continuations: Vec<LensAdvisorySignalContinuation>,
    advisory_signals: Vec<AdvisoryForecastSignal>,
    lens: LensRequest,
    expected: LensAdvisorySignalExpected,
}

#[derive(Debug, Deserialize)]
struct LensAdvisorySignalContinuation {
    label: String,
    input: tfk_protocol::ContinuationInput,
}

#[derive(Debug, Deserialize)]
struct LensAdvisorySignalExpected {
    active_continuation_titles: Vec<String>,
    advisory_signal_names: Vec<String>,
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

fn lens_from_store(store: &Store, request: &LensRequest) -> anyhow::Result<LensCard> {
    let advisory_signals = store
        .search_advisory_forecast_signals(&request.query)
        .context("failed to search advisory forecast signals")?;
    let continuation_ids = store
        .search_continuations(&request.query)
        .context("failed to search continuations")?;
    let mut continuations = Vec::new();
    for id in &continuation_ids {
        if let Some(continuation) = store
            .get_continuation(id)
            .context("failed to load continuation search hit")?
        {
            continuations.push(continuation);
        }
    }

    let mut commitment_constraints = store
        .active_commitments_for_continuations(&continuation_ids)
        .context("failed to load active commitment constraints")?;
    for commitment in store
        .search_active_commitments(&request.query)
        .context("failed to search active commitments")?
    {
        if !commitment_constraints
            .iter()
            .any(|stored| stored.id == commitment.id)
        {
            if let Some(continuation) = store
                .get_continuation(&commitment.continuation_id)
                .context("failed to load active commitment continuation")?
            {
                if !continuations
                    .iter()
                    .any(|stored| stored.id == continuation.id)
                {
                    continuations.push(continuation);
                }
            }
            commitment_constraints.push(commitment);
        }
    }

    let mut raw_event_count = 0;
    let mut promoted_raw_event_ids = Vec::new();
    if continuations.is_empty() {
        let hits = store
            .search_raw_events(&request.query)
            .context("failed to search raw events")?;
        let linked_continuations = store
            .active_continuations_for_raw_event_ids(&hits)
            .context("failed to load active continuations linked to raw events")?;
        if linked_continuations.is_empty() {
            for id in hits {
                if store
                    .get_raw_event(&id)
                    .context("failed to load raw event search hit")?
                    .is_some()
                {
                    raw_event_count += 1;
                }
            }
        } else {
            let linked_ids: Vec<_> = linked_continuations
                .iter()
                .map(|continuation| continuation.id.clone())
                .collect();
            commitment_constraints = store
                .active_commitments_for_continuations(&linked_ids)
                .context("failed to load linked active commitment constraints")?;
            continuations = linked_continuations;
            promoted_raw_event_ids = hits;
        }
    }

    let selected_continuation_ids: Vec<_> = continuations
        .iter()
        .map(|continuation| continuation.id.clone())
        .collect();
    let relations = store
        .active_continuation_relations_for_continuation_ids(&selected_continuation_ids)
        .context("failed to load active continuation relations")?;
    let time_field_continuations: Vec<_> = continuations
        .into_iter()
        .map(|continuation| TimeFieldContinuation {
            id: continuation.id,
            title: continuation.title,
            summary: if continuation
                .raw_event_id
                .as_ref()
                .is_some_and(|raw_event_id| promoted_raw_event_ids.contains(raw_event_id))
            {
                // The raw-event search already proved the query matches evidence linked to
                // this continuation. Seed only this in-memory core projection; do not mutate
                // the stored continuation or widen the fixture/protocol schema.
                format!("{} {}", continuation.summary, request.query)
            } else {
                continuation.summary
            },
            continuation_type: continuation.continuation_type,
            status: continuation.status,
        })
        .collect();
    let mut card = TimeFieldLensEngine.generate_with_relations(
        request,
        &time_field_continuations,
        &relations,
        raw_event_count,
    );
    card.commitment_constraints = commitment_constraints;
    card.advisory_forecast_signals = advisory_signals
        .iter()
        .map(lens_advisory_forecast_signal)
        .collect();
    Ok(card)
}

fn lens_advisory_forecast_signal(
    signal: &tfk_store::StoredAdvisoryForecastSignal,
) -> LensAdvisoryForecastSignal {
    LensAdvisoryForecastSignal {
        id: signal.id.clone(),
        name: signal.name.clone(),
        confidence: signal.confidence,
        model: signal.model.clone(),
        action_name: signal.action_name.clone(),
        reason: signal.reason.clone(),
        created_at: signal.created_at.clone(),
    }
}

fn boundary_kinds(card: &LensCard) -> Vec<String> {
    card.boundaries
        .iter()
        .map(|boundary| boundary.kind.clone())
        .collect()
}

fn continuation_input_from_commitment(
    commitment: &CommitRequest,
) -> tfk_protocol::ContinuationInput {
    tfk_protocol::ContinuationInput {
        title: commitment.statement.clone(),
        summary: format!(
            "speaker={}; statement={}; scope={}; deadline={}; revocable={}",
            commitment.speaker,
            commitment.statement,
            commitment.scope.as_deref().unwrap_or("unspecified"),
            commitment.deadline.as_deref().unwrap_or("unspecified"),
            commitment.revocable
        ),
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: None,
    }
}

fn status_name(status: ContinuationStatus) -> &'static str {
    match status {
        ContinuationStatus::Active => "active",
        ContinuationStatus::Stabilized => "stabilized",
        ContinuationStatus::Deferred => "deferred",
        ContinuationStatus::Closed => "closed",
        ContinuationStatus::Retired => "retired",
    }
}

fn default_preflight_threshold() -> f64 {
    0.5
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
