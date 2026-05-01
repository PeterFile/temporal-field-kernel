use tfk_cli::{build_http_request, extract_http_response_body};

#[test]
fn uds_http_request_includes_method_path_length_and_json_body() {
    let request = build_http_request("POST", "/v1/lens", br#"{"query":"x"}"#);
    let text = String::from_utf8(request).unwrap();

    assert!(text.starts_with("POST /v1/lens HTTP/1.1\r\n"));
    assert!(text.contains("Content-Type: application/json\r\n"));
    assert!(text.contains("Content-Length: 13\r\n"));
    assert!(text.ends_with("\r\n\r\n{\"query\":\"x\"}"));
}

#[test]
fn uds_http_get_request_uses_empty_body() {
    let request = build_http_request("GET", "/v1/continuations", b"");
    let text = String::from_utf8(request).unwrap();

    assert!(text.starts_with("GET /v1/continuations HTTP/1.1\r\n"));
    assert!(text.contains("Content-Length: 0\r\n"));
    assert!(text.ends_with("\r\n\r\n"));
}

#[test]
fn uds_http_response_body_rejects_non_success_status() {
    let response = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\n\r\n{}";
    let error = extract_http_response_body(response).unwrap_err();

    assert!(error
        .to_string()
        .contains("daemon returned HTTP status 500"));
}

#[test]
fn uds_http_response_body_extracts_success_body() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}";
    let body = extract_http_response_body(response).unwrap();

    assert_eq!(body, br#"{"ok":true}"#);
}
