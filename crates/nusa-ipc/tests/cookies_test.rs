//! Cookie header parsing for IPC requests.

use std::collections::HashMap;

use nusa_ipc::cookies::parse_cookie_header;

#[test]
fn cookies_parse_single_header() {
    let mut headers = HashMap::new();
    headers.insert(
        "Cookie".into(),
        vec!["session=abc123; XSRF-TOKEN=token".into()],
    );
    let cookies = parse_cookie_header(&headers);
    assert_eq!(cookies.get("session").map(String::as_str), Some("abc123"));
    assert_eq!(cookies.get("XSRF-TOKEN").map(String::as_str), Some("token"));
}

#[test]
fn cookies_parse_case_insensitive_header_name() {
    let mut headers = HashMap::new();
    headers.insert("cookie".into(), vec!["nusa=1".into()]);
    let cookies = parse_cookie_header(&headers);
    assert_eq!(cookies.get("nusa").map(String::as_str), Some("1"));
}

#[test]
fn cookies_empty_when_no_cookie_header() {
    let headers = HashMap::new();
    assert!(parse_cookie_header(&headers).is_empty());
}
