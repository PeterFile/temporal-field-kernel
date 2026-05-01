use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;
use tfk_protocol::{EventSource, RawEventInput};
use tfk_store::Store;

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

fn open_test_store(root: &std::path::Path) -> Store {
    let data_dir = root.join("data");
    Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap()
}

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
