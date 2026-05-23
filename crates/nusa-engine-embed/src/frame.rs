//! Binary embed frame v1 (`NEB1`) — P4-B zero-copy transport spike.
//!
//! Outer framing: 4-byte LE length + payload (same envelope as JSON stdio).
//! Headers remain JSON bytes inside the frame until shared-memory FFI lands.

use std::collections::HashMap;

use bytes::Bytes;

use crate::error::EmbedError;

pub const MAGIC: [u8; 4] = *b"NEB1";
pub const VERSION: u8 = 1;

pub const OP_BOOTSTRAP: u8 = 1;
pub const OP_ACK: u8 = 2;
pub const OP_REQUEST: u8 = 3;
pub const OP_RESPONSE: u8 = 4;
pub const OP_ERROR: u8 = 5;
pub const OP_ASYNC_QUERY: u8 = 6;
pub const OP_ASYNC_RESULT: u8 = 7;

const HEADER_LEN: usize = 8;

/// Result of an embed async I/O round-trip (P4-D).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncQueryResult {
    pub ok: bool,
    pub scalar: Option<i64>,
    pub json: Option<String>,
    pub message: String,
}

/// Encode bootstrap with Laravel `code_dir`.
pub fn encode_bootstrap(code_dir: &str) -> Result<Vec<u8>, EmbedError> {
    let code = code_dir.as_bytes();
    let mut buf = Vec::with_capacity(HEADER_LEN + 4 + code.len());
    write_header(&mut buf, OP_BOOTSTRAP);
    write_u32(
        &mut buf,
        u32::try_from(code.len()).map_err(|_| EmbedError::Json("code_dir too long".into()))?,
    );
    buf.extend_from_slice(code);
    Ok(wrap_length_prefixed(buf))
}

pub fn decode_ack(payload: &[u8]) -> Result<(), EmbedError> {
    let inner = strip_wrapper(payload)?;
    let op = parse_header(inner)?.1;
    if op == OP_ACK {
        Ok(())
    } else {
        Err(EmbedError::Json(format!("expected Ack, got op {op}")))
    }
}

pub fn encode_request(
    method: &str,
    uri: &str,
    headers: &HashMap<String, Vec<String>>,
    body: &[u8],
) -> Result<Vec<u8>, EmbedError> {
    let headers_json = serde_json::to_vec(headers).map_err(|e| EmbedError::Json(e.to_string()))?;
    let method_b = method.as_bytes();
    let uri_b = uri.as_bytes();
    let mut buf = Vec::with_capacity(
        HEADER_LEN + 2 + 4 + 4 + 4 + method_b.len() + uri_b.len() + headers_json.len() + body.len(),
    );
    write_header(&mut buf, OP_REQUEST);
    write_u16(
        &mut buf,
        u16::try_from(method_b.len()).map_err(|_| EmbedError::Json("method too long".into()))?,
    );
    write_u32(
        &mut buf,
        u32::try_from(uri_b.len()).map_err(|_| EmbedError::Json("uri too long".into()))?,
    );
    write_u32(
        &mut buf,
        u32::try_from(headers_json.len())
            .map_err(|_| EmbedError::Json("headers too large".into()))?,
    );
    write_u32(
        &mut buf,
        u32::try_from(body.len()).map_err(|_| EmbedError::Json("body too large".into()))?,
    );
    buf.extend_from_slice(method_b);
    buf.extend_from_slice(uri_b);
    buf.extend_from_slice(&headers_json);
    buf.extend_from_slice(body);
    Ok(wrap_length_prefixed(buf))
}

pub struct DecodedResponse {
    pub status: u16,
    pub headers: HashMap<String, Vec<String>>,
    pub body: Bytes,
}

pub fn decode_response(payload: &[u8]) -> Result<DecodedResponse, EmbedError> {
    let inner = strip_wrapper(payload)?;
    let (rest, op) = parse_header(inner)?;
    if op != OP_RESPONSE {
        return Err(EmbedError::Json(format!("expected Response, got op {op}")));
    }
    let (rest, status_b) = take(rest, 2)?;
    let status = u16::from_le_bytes(status_b.try_into().expect("2 bytes"));
    let (rest, hdr_len_b) = take(rest, 4)?;
    let hdr_len = u32::from_le_bytes(hdr_len_b.try_into().expect("4 bytes")) as usize;
    let (rest, body_len_b) = take(rest, 4)?;
    let body_len = u32::from_le_bytes(body_len_b.try_into().expect("4 bytes")) as usize;
    let (rest, hdr_bytes) = take(rest, hdr_len)?;
    let (rest, body_bytes) = take(rest, body_len)?;
    let _ = rest;
    let headers: HashMap<String, Vec<String>> =
        serde_json::from_slice(hdr_bytes).map_err(|e| EmbedError::Json(e.to_string()))?;
    Ok(DecodedResponse {
        status,
        headers,
        body: Bytes::copy_from_slice(body_bytes),
    })
}

/// Read the NEB1 opcode from a length-prefixed frame.
pub fn frame_op(payload: &[u8]) -> Result<u8, EmbedError> {
    let inner = strip_wrapper(payload)?;
    Ok(parse_header(inner)?.1)
}

/// PHP → Rust async query (P4-D spike).
pub fn encode_async_query(sql: &str) -> Result<Vec<u8>, EmbedError> {
    let sql_b = sql.as_bytes();
    let mut buf = Vec::with_capacity(HEADER_LEN + 4 + sql_b.len());
    write_header(&mut buf, OP_ASYNC_QUERY);
    write_u32(
        &mut buf,
        u32::try_from(sql_b.len()).map_err(|_| EmbedError::Json("sql too long".into()))?,
    );
    buf.extend_from_slice(sql_b);
    Ok(wrap_length_prefixed(buf))
}

pub fn decode_async_query(payload: &[u8]) -> Result<String, EmbedError> {
    let inner = strip_wrapper(payload)?;
    let (rest, op) = parse_header(inner)?;
    if op != OP_ASYNC_QUERY {
        return Err(EmbedError::Json(format!(
            "expected AsyncQuery, got op {op}"
        )));
    }
    let (rest, len_b) = take(rest, 4)?;
    let len = u32::from_le_bytes(len_b.try_into().expect("4 bytes")) as usize;
    let (_rest, sql) = take(rest, len)?;
    Ok(String::from_utf8_lossy(sql).into_owned())
}

const ASYNC_KIND_SCALAR: u8 = 0;
const ASYNC_KIND_JSON: u8 = 1;

pub fn encode_async_result_ok(value: i64) -> Result<Vec<u8>, EmbedError> {
    let mut buf = Vec::with_capacity(HEADER_LEN + 2 + 8);
    write_header(&mut buf, OP_ASYNC_RESULT);
    buf.push(0);
    buf.push(ASYNC_KIND_SCALAR);
    buf.extend_from_slice(&value.to_le_bytes());
    Ok(wrap_length_prefixed(buf))
}

pub fn encode_async_result_json(json: &str) -> Result<Vec<u8>, EmbedError> {
    let payload = json.as_bytes();
    let mut buf = Vec::with_capacity(HEADER_LEN + 2 + 4 + payload.len());
    write_header(&mut buf, OP_ASYNC_RESULT);
    buf.push(0);
    buf.push(ASYNC_KIND_JSON);
    write_u32(
        &mut buf,
        u32::try_from(payload.len()).map_err(|_| EmbedError::Json("json too long".into()))?,
    );
    buf.extend_from_slice(payload);
    Ok(wrap_length_prefixed(buf))
}

pub fn encode_async_result_err(message: &str) -> Result<Vec<u8>, EmbedError> {
    let msg = message.as_bytes();
    let mut buf = Vec::with_capacity(HEADER_LEN + 1 + 4 + msg.len());
    write_header(&mut buf, OP_ASYNC_RESULT);
    buf.push(1);
    write_u32(
        &mut buf,
        u32::try_from(msg.len()).map_err(|_| EmbedError::Json("message too long".into()))?,
    );
    buf.extend_from_slice(msg);
    Ok(wrap_length_prefixed(buf))
}

pub fn decode_async_result(payload: &[u8]) -> Result<AsyncQueryResult, EmbedError> {
    let inner = strip_wrapper(payload)?;
    let (rest, op) = parse_header(inner)?;
    if op != OP_ASYNC_RESULT {
        return Err(EmbedError::Json(format!(
            "expected AsyncResult, got op {op}"
        )));
    }
    let (rest, status_b) = take(rest, 1)?;
    let status = status_b[0];
    if status == 0 {
        if rest.len() == 8 {
            let value = i64::from_le_bytes(rest[..8].try_into().expect("8 bytes"));
            return Ok(AsyncQueryResult {
                ok: true,
                scalar: Some(value),
                json: None,
                message: String::new(),
            });
        }
        let (rest, kind_b) = take(rest, 1)?;
        let kind = kind_b[0];
        return match kind {
            ASYNC_KIND_SCALAR => {
                let (_rest, val_b) = take(rest, 8)?;
                let value = i64::from_le_bytes(val_b.try_into().expect("8 bytes"));
                Ok(AsyncQueryResult {
                    ok: true,
                    scalar: Some(value),
                    json: None,
                    message: String::new(),
                })
            }
            ASYNC_KIND_JSON => {
                let (rest, len_b) = take(rest, 4)?;
                let len = u32::from_le_bytes(len_b.try_into().expect("4 bytes")) as usize;
                let (_rest, json) = take(rest, len)?;
                Ok(AsyncQueryResult {
                    ok: true,
                    scalar: None,
                    json: Some(String::from_utf8_lossy(json).into_owned()),
                    message: String::new(),
                })
            }
            other => Err(EmbedError::Json(format!(
                "unknown async result kind {other}"
            ))),
        };
    }
    let (rest, len_b) = take(rest, 4)?;
    let len = u32::from_le_bytes(len_b.try_into().expect("4 bytes")) as usize;
    let (_rest, msg) = take(rest, len)?;
    Ok(AsyncQueryResult {
        ok: false,
        scalar: None,
        json: None,
        message: String::from_utf8_lossy(msg).into_owned(),
    })
}

pub fn decode_error(payload: &[u8]) -> Result<String, EmbedError> {
    let inner = strip_wrapper(payload)?;
    let (rest, op) = parse_header(inner)?;
    if op != OP_ERROR {
        return Err(EmbedError::Json(format!("expected Error, got op {op}")));
    }
    let (rest, len_b) = take(rest, 4)?;
    let len = u32::from_le_bytes(len_b.try_into().expect("4 bytes")) as usize;
    let (_rest, msg) = take(rest, len)?;
    Ok(String::from_utf8_lossy(msg).into_owned())
}

fn wrap_length_prefixed(mut inner: Vec<u8>) -> Vec<u8> {
    let len = inner.len() as u32;
    let mut out = Vec::with_capacity(4 + inner.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.append(&mut inner);
    out
}

fn strip_wrapper(data: &[u8]) -> Result<&[u8], EmbedError> {
    if data.len() < 4 {
        return Err(EmbedError::Json("frame too short".into()));
    }
    let len = u32::from_le_bytes(data[..4].try_into().expect("4 bytes")) as usize;
    if data.len() < 4 + len {
        return Err(EmbedError::Json("truncated frame".into()));
    }
    Ok(&data[4..4 + len])
}

fn parse_header(data: &[u8]) -> Result<(&[u8], u8), EmbedError> {
    if data.len() < HEADER_LEN {
        return Err(EmbedError::Json("header too short".into()));
    }
    if data[..4] != MAGIC {
        return Err(EmbedError::Json("bad frame magic".into()));
    }
    if data[4] != VERSION {
        return Err(EmbedError::Json(format!(
            "unsupported frame version {}",
            data[4]
        )));
    }
    Ok((&data[HEADER_LEN..], data[5]))
}

fn write_header(buf: &mut Vec<u8>, op: u8) {
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(op);
    buf.push(0);
    buf.push(0);
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn take(data: &[u8], n: usize) -> Result<(&[u8], &[u8]), EmbedError> {
    if data.len() < n {
        return Err(EmbedError::Json("unexpected EOF in frame".into()));
    }
    Ok((&data[n..], &data[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_ack_roundtrip_shape() {
        let frame = encode_bootstrap("/app").expect("encode");
        let inner = strip_wrapper(&frame).expect("wrap");
        let (_, op) = parse_header(inner).expect("header");
        assert_eq!(op, OP_BOOTSTRAP);
    }

    #[test]
    fn request_response_roundtrip() {
        let mut headers = HashMap::new();
        headers.insert("Host".into(), vec!["localhost".into()]);
        let req = encode_request("GET", "/nusa-ping", &headers, b"").expect("req");
        assert!(req.len() > HEADER_LEN);

        let mut resp_inner = Vec::new();
        write_header(&mut resp_inner, OP_RESPONSE);
        write_u16(&mut resp_inner, 200);
        let hdr = serde_json::to_vec(&headers).unwrap();
        write_u32(&mut resp_inner, u32::try_from(hdr.len()).unwrap());
        write_u32(&mut resp_inner, 4);
        resp_inner.extend_from_slice(&hdr);
        resp_inner.extend_from_slice(b"pong");
        let resp = wrap_length_prefixed(resp_inner);

        let decoded = decode_response(&resp).expect("decode");
        assert_eq!(decoded.status, 200);
        assert_eq!(&decoded.body[..], b"pong");
    }

    #[test]
    fn async_query_result_roundtrip() {
        let q = encode_async_query("SELECT 1").expect("query");
        assert_eq!(frame_op(&q).expect("op"), OP_ASYNC_QUERY);
        assert_eq!(decode_async_query(&q).expect("sql"), "SELECT 1");

        let ok = encode_async_result_ok(1).expect("ok");
        let decoded = decode_async_result(&ok).expect("result");
        assert!(decoded.ok);
        assert_eq!(decoded.scalar, Some(1));

        let json = encode_async_result_json(r#"[{"two":2}]"#).expect("json");
        let decoded = decode_async_result(&json).expect("json decode");
        assert_eq!(decoded.json.as_deref(), Some(r#"[{"two":2}]"#));

        let err = encode_async_result_err("nope").expect("err");
        let decoded = decode_async_result(&err).expect("result");
        assert!(!decoded.ok);
        assert_eq!(decoded.message, "nope");
    }
}
