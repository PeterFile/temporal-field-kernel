use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::sync::{Arc, Mutex};

use tempfile::tempdir;
use tfk_protocol::{
    AdvisoryForecastSignal, CommitRequest, ContinuationDelta, ContinuationInput,
    ContinuationStatus, ContinuationStatusDelta, ContinuationType, EventSource, RawEventInput,
    TemporalDeltaInput,
};
use tfk_store::Store;
use tfk_vector::{
    VectorDocument, VectorDocumentKind, VectorError, VectorHit, VectorIndex, VectorIndexOutcome,
    VectorIndexStatus,
};

#[test]
fn appending_raw_event_writes_jsonl_and_sqlite_index() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();

    let input = RawEventInput::new_text("s1", "cli", EventSource::User, "不要做项目状态机");
    let stored = store.append_raw_event(&input).unwrap();

    let loaded = store.get_raw_event(&stored.id).unwrap().unwrap();
    assert_eq!(loaded.id, stored.id);
    assert_eq!(loaded.session_id, "s1");
    assert_eq!(loaded.adapter_id, "cli");
    assert!(loaded.archive_len > 0);

    let jsonl = fs::read_to_string(archive_dir.join("events-000001.jsonl")).unwrap();
    assert!(jsonl.contains("不要做项目状态机"));
    assert!(jsonl.contains(&stored.id));
}

#[test]
fn default_store_uses_noop_vector_index() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());

    assert_eq!(
        store.vector_index_status(),
        VectorIndexStatus::unavailable("noop", "vector backend is not configured")
    );
}

#[test]
fn appending_raw_event_upserts_raw_event_vector_document() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
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

    let stored = store
        .append_raw_event(&RawEventInput::new_text(
            "s1",
            "cli",
            EventSource::User,
            "index this observation",
        ))
        .unwrap();

    let documents = index.documents.lock().unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].source_id, stored.id);
    assert_eq!(documents[0].kind, VectorDocumentKind::RawEvent);
    assert_eq!(documents[0].text, "index this observation");
}

#[test]
fn vector_upsert_failure_does_not_fail_raw_event_append() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let store = Store::open_with_vector_index(
        data_dir.join("tfk.db"),
        data_dir.join("archive"),
        Arc::new(FailingVectorIndex),
    )
    .unwrap();

    let stored = store
        .append_raw_event(&RawEventInput::new_text(
            "s1",
            "cli",
            EventSource::User,
            "vector backend may be down",
        ))
        .unwrap();

    assert_eq!(
        store.get_raw_event(&stored.id).unwrap().unwrap().content,
        "vector backend may be down"
    );
}

#[test]
fn fts_search_finds_archived_event_content() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());

    let input = RawEventInput::new_text(
        "s1",
        "cli",
        EventSource::User,
        "Temporal Field Kernel 时间场内核",
    );
    let stored = store.append_raw_event(&input).unwrap();

    let hits = store.search_raw_events("时间场").unwrap();
    assert_eq!(hits, vec![stored.id]);
}

#[test]
fn search_treats_query_as_literal_text() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());

    let input = RawEventInput::new_text("s1", "cli", EventSource::User, "状态机 \"不要\" (test)");
    let stored = store.append_raw_event(&input).unwrap();

    let hits = store.search_raw_events("\"不要\" (test)").unwrap();
    assert_eq!(hits, vec![stored.id]);
}

#[test]
fn empty_search_returns_no_hits() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());

    assert!(store.search_raw_events("   ").unwrap().is_empty());
}

#[test]
fn search_escapes_like_wildcards() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());

    let percent = store
        .append_raw_event(&RawEventInput::new_text(
            "s1",
            "cli",
            EventSource::User,
            "literal marker 100%_done",
        ))
        .unwrap();
    let wildcard_candidate = store
        .append_raw_event(&RawEventInput::new_text(
            "s1",
            "cli",
            EventSource::User,
            "literal marker 100xxdone",
        ))
        .unwrap();

    let hits = store.search_raw_events("100%_done").unwrap();
    assert!(hits.contains(&percent.id));
    assert!(!hits.contains(&wildcard_candidate.id));

    let percent_hits = store.search_raw_events("%").unwrap();
    assert!(percent_hits.contains(&percent.id));
    assert!(!percent_hits.contains(&wildcard_candidate.id));
}

#[test]
fn store_files_and_directories_are_owner_only() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    store
        .append_raw_event(&RawEventInput::new_text(
            "s1",
            "cli",
            EventSource::User,
            "private raw event",
        ))
        .unwrap();

    assert_eq!(mode(&data_dir), 0o700);
    assert_eq!(mode(&archive_dir), 0o700);
    assert_eq!(mode(&db_path), 0o600);
    assert_eq!(mode(&archive_dir.join("events-000001.jsonl")), 0o600);
}

#[test]
fn store_rejects_existing_public_data_directory() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("public-data");
    fs::create_dir(&data_dir).unwrap();
    fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o755)).unwrap();

    let error = Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap_err();

    assert!(error.to_string().contains("refusing non-private directory"));
}

#[test]
fn continuation_create_list_get_persists_in_sqlite() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let created_id = {
        let store = Store::open(&db_path, &archive_dir).unwrap();
        let created = store
            .create_continuation(&ContinuationInput {
                title: "项目状态机不是目标".to_string(),
                summary: "继续跟踪这个判断".to_string(),
                continuation_type: ContinuationType::Obligation,
                status: ContinuationStatus::Active,
                parent_id: Some("cont_parent".to_string()),
                raw_event_id: Some("evt_source".to_string()),
            })
            .unwrap();

        assert!(created.id.starts_with("cont_"));
        assert_eq!(created.title, "项目状态机不是目标");
        assert_eq!(created.summary, "继续跟踪这个判断");
        assert_eq!(created.continuation_type, ContinuationType::Obligation);
        assert_eq!(created.status, ContinuationStatus::Active);
        assert_eq!(created.parent_id.as_deref(), Some("cont_parent"));
        assert_eq!(created.raw_event_id.as_deref(), Some("evt_source"));
        assert_eq!(created.created_at, created.updated_at);
        created.id
    };

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    let loaded = reopened.get_continuation(&created_id).unwrap().unwrap();
    assert_eq!(loaded.id, created_id);

    let listed = reopened.list_continuations().unwrap();
    assert_eq!(listed, vec![loaded]);
}

#[test]
fn continuation_search_matches_title_and_summary_as_literal_text() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let target = store
        .create_continuation(&ContinuationInput {
            title: "状态机 100%_literal".to_string(),
            summary: "继续保存 continuation graph provenance".to_string(),
            continuation_type: ContinuationType::Risk,
            status: ContinuationStatus::Active,
            parent_id: None,
            raw_event_id: None,
        })
        .unwrap();
    let other = store
        .create_continuation(&ContinuationInput {
            title: "状态机 100xxliteral".to_string(),
            summary: "unrelated".to_string(),
            continuation_type: ContinuationType::Narrative,
            status: ContinuationStatus::Deferred,
            parent_id: None,
            raw_event_id: None,
        })
        .unwrap();

    let title_hits = store.search_continuations("100%_literal").unwrap();
    assert_eq!(title_hits, vec![target.id.clone()]);

    let summary_hits = store.search_continuations("continuation graph").unwrap();
    assert_eq!(summary_hits, vec![target.id]);
    assert!(!summary_hits.contains(&other.id));
}

#[test]
fn opening_legacy_continuation_table_adds_narrative_default() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    fs::create_dir_all(&data_dir).unwrap();
    fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700)).unwrap();
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE continuations (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              summary TEXT NOT NULL,
              status TEXT NOT NULL,
              parent_id TEXT,
              raw_event_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            INSERT INTO continuations (
              id, title, summary, status, parent_id, raw_event_id, created_at, updated_at
            ) VALUES (
              'cont_legacy', 'legacy', 'old row', 'active', NULL, NULL,
              '2026-05-02T00:00:00Z', '2026-05-02T00:00:00Z'
            );
            "#,
        )
        .unwrap();
    }

    let store = Store::open(&db_path, &archive_dir).unwrap();
    let loaded = store.get_continuation("cont_legacy").unwrap().unwrap();

    assert_eq!(loaded.continuation_type, ContinuationType::Narrative);
}

#[test]
fn temporal_delta_is_appended_and_assimilates_status_updates() {
    let tmp = tempdir().unwrap();
    let mut store = open_test_store(tmp.path());
    let created = store
        .create_continuation(&ContinuationInput {
            title: "verify release".to_string(),
            summary: "risk must be checked".to_string(),
            continuation_type: ContinuationType::Risk,
            status: ContinuationStatus::Active,
            parent_id: None,
            raw_event_id: None,
        })
        .unwrap();
    let delta = TemporalDeltaInput {
        action_id: "a42".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: created.id.clone(),
            delta: ContinuationDelta::Close,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let stored_delta = store.assimilate_delta(&delta).unwrap();

    assert!(stored_delta.id.starts_with("delta_"));
    assert_eq!(stored_delta.action_id, "a42");
    let updated = store.get_continuation(&created.id).unwrap().unwrap();
    assert_eq!(updated.status, ContinuationStatus::Closed);
    assert_eq!(store.list_temporal_deltas().unwrap(), vec![stored_delta]);
}

#[test]
fn advisory_forecast_signals_are_recorded_and_listed() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let signals = vec![AdvisoryForecastSignal {
        name: "forming_future_risk".to_string(),
        model: "static-test".to_string(),
        confidence: 0.8,
        action_name: Some("verify then ship".to_string()),
        reason: Some("unresolved risk".to_string()),
    }];

    let stored = store.record_advisory_forecast_signals(&signals).unwrap();

    assert_eq!(stored.len(), 1);
    assert!(stored[0].id.starts_with("advisory_signal_"));
    assert_eq!(stored[0].name, "forming_future_risk");
    assert_eq!(stored[0].confidence, 0.8);
    assert_eq!(stored[0].model, "static-test");
    assert_eq!(stored[0].action_name.as_deref(), Some("verify then ship"));
    assert_eq!(stored[0].reason.as_deref(), Some("unresolved risk"));

    let listed = store.list_advisory_forecast_signals().unwrap();
    assert_eq!(listed, stored);
}

#[test]
fn commitment_create_reopen_and_active_filtering_uses_linked_continuation_status() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let (commitment_id, continuation_id) = {
        let store = Store::open(&db_path, &archive_dir).unwrap();
        let continuation = store
            .create_continuation(&ContinuationInput {
                title: "send the draft".to_string(),
                summary: "commitment continuation".to_string(),
                continuation_type: ContinuationType::Obligation,
                status: ContinuationStatus::Active,
                parent_id: None,
                raw_event_id: None,
            })
            .unwrap();
        let commitment = store
            .create_commitment(
                &CommitRequest {
                    speaker: "agent".to_string(),
                    statement: "send the draft".to_string(),
                    scope: Some("current_project".to_string()),
                    deadline: Some("2026-05-02".to_string()),
                    revocable: false,
                },
                &continuation.id,
            )
            .unwrap();

        assert!(commitment.id.starts_with("commit_"));
        assert_eq!(commitment.continuation_id, continuation.id);
        assert_eq!(commitment.speaker, "agent");
        assert_eq!(commitment.statement, "send the draft");
        assert_eq!(commitment.scope.as_deref(), Some("current_project"));
        assert_eq!(commitment.deadline.as_deref(), Some("2026-05-02"));
        assert!(!commitment.revocable);
        assert_eq!(commitment.status, ContinuationStatus::Active);
        (commitment.id, continuation.id)
    };

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    let active = reopened.list_active_commitments().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, commitment_id);

    let matching = reopened.search_active_commitments("draft").unwrap();
    assert_eq!(matching, active);

    let mut reopened = reopened;
    reopened
        .assimilate_delta(&TemporalDeltaInput {
            action_id: "a-close".to_string(),
            changes: vec![ContinuationStatusDelta {
                continuation_id,
                delta: ContinuationDelta::Close,
            }],
            claims_made: Vec::new(),
            evidence: Vec::new(),
        })
        .unwrap();
    assert!(reopened.list_active_commitments().unwrap().is_empty());
}

#[test]
fn temporal_delta_maps_supported_assimilation_deltas_to_statuses() {
    let tmp = tempdir().unwrap();
    let mut store = open_test_store(tmp.path());
    let cases = [
        (ContinuationDelta::Activate, ContinuationStatus::Active),
        (ContinuationDelta::Advance, ContinuationStatus::Active),
        (ContinuationDelta::Repair, ContinuationStatus::Active),
        (ContinuationDelta::Verify, ContinuationStatus::Active),
        (ContinuationDelta::Renegotiate, ContinuationStatus::Active),
        (ContinuationDelta::Stabilize, ContinuationStatus::Stabilized),
        (ContinuationDelta::Defer, ContinuationStatus::Deferred),
        (ContinuationDelta::Retire, ContinuationStatus::Retired),
    ];
    let changes: Vec<_> = cases
        .iter()
        .map(|(delta, _)| {
            let continuation = store
                .create_continuation(&ContinuationInput {
                    title: format!("{delta:?} continuation"),
                    summary: "assimilation mapping".to_string(),
                    continuation_type: ContinuationType::Narrative,
                    status: ContinuationStatus::Deferred,
                    parent_id: None,
                    raw_event_id: None,
                })
                .unwrap();
            (continuation.id, *delta)
        })
        .collect();
    let delta = TemporalDeltaInput {
        action_id: "a43".to_string(),
        changes: changes
            .iter()
            .map(|(continuation_id, delta)| ContinuationStatusDelta {
                continuation_id: continuation_id.clone(),
                delta: *delta,
            })
            .collect(),
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    store.assimilate_delta(&delta).unwrap();

    for ((continuation_id, _), (_, expected_status)) in changes.iter().zip(cases) {
        let updated = store.get_continuation(continuation_id).unwrap().unwrap();
        assert_eq!(updated.status, expected_status);
    }
}

#[test]
fn structural_create_and_split_deltas_are_append_only() {
    let tmp = tempdir().unwrap();
    let mut store = open_test_store(tmp.path());
    let continuation = store
        .create_continuation(&ContinuationInput {
            title: "existing continuation".to_string(),
            summary: "structural deltas should not rewrite status".to_string(),
            continuation_type: ContinuationType::Narrative,
            status: ContinuationStatus::Deferred,
            parent_id: None,
            raw_event_id: None,
        })
        .unwrap();
    let delta = TemporalDeltaInput {
        action_id: "a44".to_string(),
        changes: vec![
            ContinuationStatusDelta {
                continuation_id: continuation.id.clone(),
                delta: ContinuationDelta::Create,
            },
            ContinuationStatusDelta {
                continuation_id: continuation.id.clone(),
                delta: ContinuationDelta::Split,
            },
        ],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let stored = store.assimilate_delta(&delta).unwrap();

    assert_eq!(store.list_temporal_deltas().unwrap(), vec![stored]);
    let unchanged = store.get_continuation(&continuation.id).unwrap().unwrap();
    assert_eq!(unchanged.status, ContinuationStatus::Deferred);
}

#[test]
fn temporal_delta_rejects_missing_status_target_and_rolls_back() {
    let tmp = tempdir().unwrap();
    let mut store = open_test_store(tmp.path());
    let existing = store
        .create_continuation(&ContinuationInput {
            title: "existing continuation".to_string(),
            summary: "rollback must preserve this status".to_string(),
            continuation_type: ContinuationType::Narrative,
            status: ContinuationStatus::Active,
            parent_id: None,
            raw_event_id: None,
        })
        .unwrap();
    let delta = TemporalDeltaInput {
        action_id: "a45".to_string(),
        changes: vec![
            ContinuationStatusDelta {
                continuation_id: existing.id.clone(),
                delta: ContinuationDelta::Close,
            },
            ContinuationStatusDelta {
                continuation_id: "missing-continuation".to_string(),
                delta: ContinuationDelta::Advance,
            },
        ],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let error = store.assimilate_delta(&delta).unwrap_err();

    assert!(error.to_string().contains("missing-continuation"));
    assert!(store.list_temporal_deltas().unwrap().is_empty());
    let unchanged = store.get_continuation(&existing.id).unwrap().unwrap();
    assert_eq!(unchanged.status, ContinuationStatus::Active);
}

#[derive(Debug, Default)]
struct RecordingVectorIndex {
    documents: Mutex<Vec<VectorDocument>>,
}

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

#[derive(Debug)]
struct FailingVectorIndex;

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

fn open_test_store(root: &std::path::Path) -> Store {
    let data_dir = root.join("data");
    Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap()
}

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
