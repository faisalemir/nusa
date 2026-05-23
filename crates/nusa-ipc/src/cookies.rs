//! Parse HTTP `Cookie` headers into IPC `cookies` map.

use std::collections::HashMap;

/// Parse `Cookie` header value(s) into name → value (first `=` split per segment).
pub fn parse_cookie_header(headers: &HashMap<String, Vec<String>>) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    for (name, values) in headers {
        if !name.eq_ignore_ascii_case("cookie") {
            continue;
        }
        for value in values {
            for segment in value.split(';') {
                let segment = segment.trim();
                if segment.is_empty() {
                    continue;
                }
                if let Some((key, val)) = segment.split_once('=') {
                    let key = key.trim();
                    if !key.is_empty() {
                        cookies.insert(key.to_string(), val.trim().to_string());
                    }
                }
            }
        }
    }
    cookies
}
