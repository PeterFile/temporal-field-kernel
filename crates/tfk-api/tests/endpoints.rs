use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tempfile::tempdir;
use tfk_core::{PreflightResult, PreflightSignals};
use tfk_protocol::{ApiEnvelope, EventSource, LensCard, LensRequest, RawEventInput};
use tfk_store::{Store, StoredRawEvent};
use tower::ServiceExt;

#[tokio::test]
async fn observe_endpoint_persists_raw_event_and_lens_can_recall_it() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let input = RawEventInput::new_text("s1", "cli", EventSource::User, "不要做项目状态机");
    let observe_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &input))
        .await
        .unwrap();

    assert_eq!(observe_response.status(), StatusCode::OK);
    let observe_envelope: ApiEnvelope<StoredRawEvent> = read_json(observe_response).await;
    assert!(observe_envelope.ok);
    let stored = observe_envelope.data.unwrap();
    assert_eq!(stored.session_id, "s1");
    assert_eq!(stored.adapter_id, "cli");
    assert_eq!(stored.content, "不要做项目状态机");
    assert!(stored.archive_len > 0);

    let lens = LensRequest {
        query: "项目状态机".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    assert_eq!(lens_envelope.provenance[0].kind, "raw_event");
    assert_eq!(lens_envelope.provenance[0].id, stored.id);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.stance, "grounded_recall");
    assert!(card.why_now.contains("1 matching raw event"));
}

#[tokio::test]
async fn preflight_endpoint_returns_confirmation_decision() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let signals = PreflightSignals {
        uncertainty: 0.9,
        irreversibility: 0.8,
        externality: 0.7,
        option_value_loss: 0.0,
    };
    let response = app
        .oneshot(json_request("POST", "/v1/preflight", &signals))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<PreflightResult> = read_json(response).await;
    assert!(envelope.ok);
    let result = envelope.data.unwrap();
    assert!(result.requires_confirmation);
    assert_eq!(result.threshold, 0.5);
    assert!(result.risk_product > result.threshold);
}

fn open_test_store(root: &std::path::Path) -> Store {
    let data_dir = root.join("data");
    Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap()
}

fn json_request<T: serde::Serialize>(method: &str, uri: &str, body: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

async fn read_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
