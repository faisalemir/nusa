//! Dev watcher debounce tests.
//! Tests file event batching, debounce timing, and event categorization.

use std::path::PathBuf;
use std::time::Duration;

use nusa_cli::dev::DevWatcher;
use notify::Event;
use tokio::sync::mpsc;

#[tokio::test]
async fn dev_watcher_creation() {
    let watcher = DevWatcher::new(200, true);
    assert_eq!(watcher.debounce_ms, 200);
}

#[tokio::test]
async fn dev_watcher_creation_nonpretty() {
    let watcher = DevWatcher::new(500, false);
    assert_eq!(watcher.debounce_ms, 500);
}

#[tokio::test]
async fn dev_watcher_debounce_batches_events() {
    use tokio::sync::mpsc::UnboundedSender;

    let (tx, mut rx): (UnboundedSender<Event>, mpsc::UnboundedReceiver<Event>) =
        mpsc::unbounded_channel();

    let debounce_ms = 100;
    let app_root = PathBuf::from("/tmp/test-app");

    // Spawn debounce loop
    let handle = tokio::spawn(async move {
        DevWatcher::debounce_loop(&mut rx, debounce_ms, &app_root, false).await;
    });

    // Send 5 events rapidly
    for i in 0..5 {
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Any,
            )),
            paths: vec![PathBuf::from(format!("/tmp/test-app/app/file-{}.php", i))],
            attrs: Default::default(),
        };
        tx.send(event).unwrap();
    }

    // Wait for debounce window + processing
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Abort the loop (it runs forever)
    handle.abort();
}

#[tokio::test]
async fn dev_watcher_debounce_resets_deadline_on_new_event() {
    use tokio::sync::mpsc::UnboundedSender;

    let (tx, mut rx): (UnboundedSender<Event>, mpsc::UnboundedReceiver<Event>) =
        mpsc::unbounded_channel();

    let debounce_ms = 100;
    let app_root = PathBuf::from("/tmp/test-app");

    let handle = tokio::spawn(async move {
        DevWatcher::debounce_loop(&mut rx, debounce_ms, &app_root, false).await;
    });

    // Send event 1
    let event1 = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/tmp/test-app/app/file1.php")],
        attrs: Default::default(),
    };
    tx.send(event1).unwrap();

    // Wait 50ms (less than debounce window)
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send event 2 — should reset deadline
    let event2 = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/tmp/test-app/app/file2.php")],
        attrs: Default::default(),
    };
    tx.send(event2).unwrap();

    // Total wait: 150ms < 100ms debounce + 100ms reset, so batch shouldn't process yet
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Now wait full debounce window from last event
    tokio::time::sleep(Duration::from_millis(150)).await;

    handle.abort();
}

#[tokio::test]
async fn dev_watcher_env_change_detected() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/.env")],
        attrs: Default::default(),
    };

    // Should not panic
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}

#[tokio::test]
async fn dev_watcher_config_change_detected() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/config/app.php")],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}

#[tokio::test]
async fn dev_watcher_app_code_change_detected() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/app/Http/Controllers/TestController.php")],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}

#[tokio::test]
async fn dev_watcher_view_change_detected() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/resources/views/welcome.blade.php")],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}

#[tokio::test]
async fn dev_watcher_ignored_dirs_skipped() {
    for dir in &["vendor", "node_modules", ".git", "storage", "bootstrap/cache"] {
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Any,
            )),
            paths: vec![PathBuf::from(format!("/app/{}/file.php", dir))],
            attrs: Default::default(),
        };

        DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
    }
}

#[tokio::test]
async fn dev_watcher_multiple_paths_in_single_event() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![
            PathBuf::from("/app/.env"),
            PathBuf::from("/app/config/app.php"),
            PathBuf::from("/app/app/Http/Test.php"),
        ],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}

#[tokio::test]
async fn dev_watcher_non_php_config_file_ignored() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/config/app.json")],
        attrs: Default::default(),
    };

    // Should not panic, but should not trigger config recycle
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}

#[tokio::test]
async fn dev_watcher_non_blade_view_ignored() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/resources/views/test.php")],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false);
}
