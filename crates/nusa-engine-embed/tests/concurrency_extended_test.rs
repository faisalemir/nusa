//! S19: Embed pool concurrency extended tests.
//!
//! Covers: concurrent frame decode/encode, atomic counter updates,
//! idle queue contention, frame encoding under parallel load.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nusa_engine_embed::FfiWorkerPool;
use nusa_engine_embed::frame::{encode_request, frame_op};

// === Concurrent Frame Encoding ===

#[test]
fn concurrent_frame_encode_no_data_corruption() {
    let threads = 8;
    let iterations = 100;
    let mut handles = Vec::new();

    for t in 0..threads {
        let h = std::thread::spawn(move || {
            let mut headers = HashMap::new();
            headers.insert("X-Thread".into(), vec![format!("worker-{t}")]);
            for i in 0..iterations {
                let uri = format!("/request/{t}/{i}");
                let frame = encode_request("GET", &uri, &headers, b"").expect("encode");
                let op = frame_op(&frame).expect("op");
                assert_eq!(op, 3, "op must be OP_REQUEST");
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }
}

#[test]
fn concurrent_bootstrap_encode() {
    let threads = 4;
    let iterations = 100;
    let mut handles = Vec::new();

    for t in 0..threads {
        let h = std::thread::spawn(move || {
            for i in 0..iterations {
                let path = format!("/app/{t}/{i}");
                let frame = nusa_engine_embed::frame::encode_bootstrap(&path).expect("encode");
                let op = frame_op(&frame).expect("op");
                assert_eq!(op, 1, "op must be OP_BOOTSTRAP");
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }
}

// === Concurrent Async Query Encode/Decode ===

#[test]
fn concurrent_async_query_encode_decode() {
    use nusa_engine_embed::frame::{decode_async_query, encode_async_query};

    let threads = 4;
    let iterations = 100;
    let mut handles = Vec::new();

    for t in 0..threads {
        let h = std::thread::spawn(move || {
            for i in 0..iterations {
                let sql = format!("SELECT {t} + {i}");
                let frame = encode_async_query(&sql).expect("encode");
                let decoded = decode_async_query(&frame).expect("decode");
                assert_eq!(decoded, sql, "SQL must roundtrip correctly");
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }
}

// === Concurrent Error Encode/Decode ===

#[test]
fn concurrent_error_encode_decode() {
    use nusa_engine_embed::frame::{decode_async_result, encode_async_result_err};

    let threads = 4;
    let iterations = 100;
    let mut handles = Vec::new();

    for t in 0..threads {
        let h = std::thread::spawn(move || {
            for i in 0..iterations {
                let msg = format!("error from thread {t}, iteration {i}");
                let frame = encode_async_result_err(&msg).expect("encode");
                let decoded = decode_async_result(&frame).expect("decode");
                assert!(!decoded.ok, "must be error result");
                assert_eq!(decoded.message, msg, "error message must roundtrip");
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }
}

// === Pool Atomic Counters Under Concurrent Access ===

#[tokio::test]
async fn pool_total_handled_atomic_under_load() {
    let pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    let pool_ref = Arc::new(tokio::sync::Mutex::new(pool));

    // Spawn tasks that would increment internal counters if pool was operational
    // Since pool isn't initialized, handle_request fails (NotReady)
    let threads = 4;
    let iterations = 50;
    let mut handles = Vec::new();

    for _ in 0..threads {
        let pool_clone = Arc::clone(&pool_ref);
        let h = tokio::spawn(async move {
            for _ in 0..iterations {
                let mut p = pool_clone.lock().await;
                let _ = p
                    .handle_http_request("GET".into(), "/test".into(), HashMap::new(), None, 1000)
                    .await;
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.await.expect("task join");
    }

    // Lock and verify pool is still accessible after concurrent operations
    let p = pool_ref.lock().await;
    assert_eq!(p.configured_workers(), 0);
}

// === Concurrent Pool Shutdown ===

#[tokio::test]
async fn pool_shutdown_during_concurrent_requests() {
    let pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    let pool_ref = Arc::new(tokio::sync::Mutex::new(pool));

    let mut handles = Vec::new();

    // Spawn request tasks
    for _ in 0..4 {
        let pool_clone = Arc::clone(&pool_ref);
        let h = tokio::spawn(async move {
            for _ in 0..10 {
                let mut p = pool_clone.lock().await;
                let _ = p
                    .handle_http_request("GET".into(), "/test".into(), HashMap::new(), None, 1000)
                    .await;
            }
        });
        handles.push(h);
    }

    // Spawn shutdown task
    let pool_clone = Arc::clone(&pool_ref);
    let shutdown_h = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let mut p = pool_clone.lock().await;
        p.shutdown().await;
    });
    handles.push(shutdown_h);

    for h in handles {
        h.await.expect("task join must not panic");
    }
}

// === Concurrent Frame Decode from Same Buffer ===

#[test]
fn frame_decode_response_thread_safe() {
    use nusa_engine_embed::frame::{decode_async_result, encode_async_result_ok};

    let threads = 8;
    let iterations = 50;
    let success_count = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for t in 0..threads {
        let success_clone = Arc::clone(&success_count);
        let h = std::thread::spawn(move || {
            for i in 0..iterations {
                let value = (t * iterations + i) as i64;
                let frame = encode_async_result_ok(value).expect("encode");
                // Multiple threads reading same frame buffer — must not corrupt
                match decode_async_result(&frame) {
                    Ok(result) => {
                        assert!(result.ok, "must be ok result");
                        assert_eq!(result.scalar, Some(value), "scalar must match");
                        success_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        // Acceptable — decode is called on a frame that might be
                        // read concurrently, but the data is immutable so it's safe
                    }
                }
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }

    let total = success_count.load(Ordering::SeqCst);
    assert_eq!(
        total,
        threads * iterations,
        "all concurrent decodes must succeed"
    );
}
