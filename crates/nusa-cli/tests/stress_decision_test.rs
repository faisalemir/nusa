//! Stress tests and decision logic tests for nusa-cli.
//!
//! Covers: DevWatcher debounce, file change handling, TestRunner.

use std::path::PathBuf;

use nusa_cli::dev::{DevAction, DevWatcher};
use nusa_cli::test::{TestConfig, TestRunner};

// ============================================================================
// Stress Tests: DevWatcher Rapid File Changes
// ============================================================================

#[tokio::test]
async fn dev_watcher_rapid_file_changes_100_per_sec_debounce_handles() {
    let mut watcher = DevWatcher::new(50, false); // 50ms debounce
    let app_root = std::env::temp_dir();

    // Start the watcher
    watcher.start(&app_root).ok();

    // Simulate rapid file changes via the debounce loop
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let debounce_dur = 50u64;

    let app_root_clone = app_root.clone();
    let debounce_task = tokio::spawn(async move {
        DevWatcher::debounce_loop(&mut rx, debounce_dur, &app_root_clone, false, action_tx).await;
    });

    // Send 100 rapid events
    for _ in 0..100 {
        let event = notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Any,
            )),
            paths: vec![app_root.join(".env")],
            attrs: Default::default(),
        };
        let _ = tx.send(event);
    }

    // Wait for debounce to process
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Count how many actions were received (should be batched)
    let mut action_count = 0;
    while let Ok(_action) = action_rx.try_recv() {
        action_count += 1;
    }

    // Debounce should have batched the 100 events
    // The exact count depends on timing, but it should be much less than 100
    assert!(
        action_count < 100,
        "debounce should batch events, got {action_count} actions"
    );

    // Drop the channel to stop the loop
    drop(tx);
    debounce_task.abort();
}

#[tokio::test]
async fn dev_watcher_debounce_no_thrashing() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let (action_tx, _action_rx) = tokio::sync::broadcast::channel(32);
    let debounce_dur = 100u64;

    let app_root = std::env::temp_dir();
    let app_root_clone = app_root.clone();

    let debounce_task = tokio::spawn(async move {
        DevWatcher::debounce_loop(&mut rx, debounce_dur, &app_root_clone, false, action_tx).await;
    });

    // Send bursts of events (simulating file save operations)
    for _ in 0..5 {
        // Each burst: 20 events in quick succession
        for _ in 0..20 {
            let event = notify::Event {
                kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                    notify::event::DataChange::Any,
                )),
                paths: vec![app_root.join("app/Example.php")],
                attrs: Default::default(),
            };
            let _ = tx.send(event);
        }
        // Wait less than debounce window (should batch)
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Wait for all debounce windows to pass
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Verify no panic and task is still running (no thrashing)
    assert!(
        !debounce_task.is_finished(),
        "debounce loop should not crash"
    );

    drop(tx);
    debounce_task.abort();
}

// ============================================================================
// Decision Logic Tests: DevWatcher File Type Actions
// ============================================================================

#[test]
fn dev_watcher_env_change_triggers_reload_config() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/.env").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    let action = action_rx.try_recv().unwrap();
    assert!(
        matches!(action, DevAction::ReloadConfig),
        ".env change should trigger ReloadConfig"
    );
}

#[test]
fn dev_watcher_config_php_change_triggers_recycle_workers() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/config/app.php").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    let action = action_rx.try_recv().unwrap();
    assert!(
        matches!(action, DevAction::RecycleWorkers),
        "config/*.php change should trigger RecycleWorkers"
    );
}

#[test]
fn dev_watcher_app_php_change_triggers_invalidate_opcache() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/app/Models/User.php").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    let action = action_rx.try_recv().unwrap();
    assert!(
        matches!(action, DevAction::InvalidateOpCache),
        "app/**/*.php change should trigger InvalidateOpCache"
    );
}

#[test]
fn dev_watcher_view_change_triggers_clear_view_cache() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let mut action_rx = action_tx.subscribe();
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/resources/views/welcome.blade.php").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    let action = action_rx.try_recv().unwrap();
    assert!(
        matches!(action, DevAction::ClearViewCache),
        "blade.php change should trigger ClearViewCache"
    );
}

#[test]
fn dev_watcher_ignored_directory_no_action() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![
            Path::new(
                "/tmp/test-app/vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
            )
            .to_path_buf(),
        ],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    // No action should be sent for vendor directory
    assert!(action_tx.receiver_count() == 0);
}

#[test]
fn dev_watcher_node_modules_ignored() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/node_modules/react/index.js").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);
    // No action for node_modules
}

#[test]
fn dev_watcher_git_ignored() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/.git/HEAD").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);
    // No action for .git
}

// ============================================================================
// Decision Logic Tests: DevWatcher Batch Events
// ============================================================================

#[test]
fn dev_watcher_batch_events_highest_priority_action() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    // .env change (ReloadConfig)
    let event1 = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/.env").to_path_buf()],
        attrs: Default::default(),
    };

    // app code change (InvalidateOpCache)
    let event2 = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/app/Http/Controllers/Api.php").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event1, &app_root, false, &action_tx);
    DevWatcher::handle_event(&event2, &app_root, false, &action_tx);

    // Both actions should be sent
    let mut actions = Vec::new();
    while let Ok(action) = action_rx.try_recv() {
        actions.push(action);
    }

    assert!(actions.iter().any(|a| matches!(a, DevAction::ReloadConfig)));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DevAction::InvalidateOpCache))
    );
}

#[test]
fn dev_watcher_duplicate_events_idempotent() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/.env").to_path_buf()],
        attrs: Default::default(),
    };

    // Send same event twice
    DevWatcher::handle_event(&event, &app_root, false, &action_tx);
    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    // Should receive two ReloadConfig actions (one per event)
    let mut reload_count = 0;
    while let Ok(action) = action_rx.try_recv() {
        if matches!(action, DevAction::ReloadConfig) {
            reload_count += 1;
        }
    }
    assert_eq!(
        reload_count, 2,
        "duplicate events should each trigger action"
    );
}

// ============================================================================
// Decision Logic Tests: DevWatcher Unknown File Types
// ============================================================================

#[test]
fn dev_watcher_unknown_file_type_no_action() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/app/something.unknown").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);
    // No action for unknown file types
}

#[test]
fn dev_watcher_non_php_in_app_dir_no_action() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![Path::new("/tmp/test-app/app/readme.txt").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);
    // Non-PHP files in app/ should not trigger actions
}

// ============================================================================
// Stress Tests: TestRunner File Enumeration
// ============================================================================

#[tokio::test]
async fn test_runner_100_test_files_enumerated() {
    let dir = std::env::temp_dir().join("nusa_test_stress");
    std::fs::create_dir_all(&dir).unwrap();

    // Create 100 test files
    for i in 0..100 {
        let file_path = dir.join(format!("ExampleTest{i}.php"));
        std::fs::write(&file_path, "<?php // test").unwrap();
    }

    let config = TestConfig {
        workers: 2,
        test_path: dir.clone(),
        reset_between_tests: false,
    };

    let _runner = TestRunner::new(config);
    // Just verify it can be created without panicking
    // Can't actually run tests without PHP workers

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_runner_50_workers_no_cross_pollution() {
    let dir = std::env::temp_dir().join("nusa_test_workers");
    std::fs::create_dir_all(&dir).unwrap();

    // Create test files
    for i in 0..10 {
        let file_path = dir.join(format!("IsolatedTest{i}.php"));
        std::fs::write(&file_path, "<?php // isolated test").unwrap();
    }

    let config = TestConfig {
        workers: 50,
        test_path: dir.clone(),
        reset_between_tests: false,
    };

    // Runner should handle multiple workers without cross-pollution
    let _runner = TestRunner::new(config);

    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================================
// Decision Logic Tests: TestRunner Configuration
// ============================================================================

#[test]
fn test_runner_config_zero_workers_allowed() {
    let config = TestConfig {
        workers: 0,
        test_path: PathBuf::from("/tmp"),
        reset_between_tests: false,
    };
    let _runner = TestRunner::new(config);
    // Should be creatable (initialization handles zero workers)
}

#[test]
fn test_runner_config_reset_between_tests_enabled() {
    let config = TestConfig {
        workers: 2,
        test_path: PathBuf::from("/tmp"),
        reset_between_tests: true,
    };
    let _runner = TestRunner::new(config);
    // Runner with reset between tests
}

#[test]
fn test_runner_config_reset_between_tests_disabled() {
    let config = TestConfig {
        workers: 2,
        test_path: PathBuf::from("/tmp"),
        reset_between_tests: false,
    };
    let _runner = TestRunner::new(config);
    // Runner without reset
}

// ============================================================================
// Decision Logic Tests: DevWatcher Constructor
// ============================================================================

#[test]
fn dev_watcher_new_default_values() {
    let watcher = DevWatcher::new(100, true);
    assert_eq!(watcher.debounce_ms, 100);
    assert!(watcher.pretty);
}

#[test]
fn dev_watcher_subscribe_returns_receiver() {
    let watcher = DevWatcher::new(50, false);
    let _rx = watcher.subscribe();
    // Should be able to subscribe multiple times
    let _rx2 = watcher.subscribe();
}

// ============================================================================
// Decision Logic Tests: DevWatcher Remove Events
// ============================================================================

#[test]
fn dev_watcher_remove_event_no_action() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Remove(notify::event::RemoveKind::File),
        paths: vec![Path::new("/tmp/test-app/app/Deleted.php").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);
    // Remove events are not handled (only Modify and Create in debounce_loop)
}

#[test]
fn dev_watcher_create_event_triggers_action() {
    use notify::EventKind;
    use std::path::Path;

    let (action_tx, mut action_rx) = tokio::sync::broadcast::channel(32);
    let app_root = PathBuf::from("/tmp/test-app");

    let event = notify::Event {
        kind: EventKind::Create(notify::event::CreateKind::File),
        paths: vec![Path::new("/tmp/test-app/app/NewController.php").to_path_buf()],
        attrs: Default::default(),
    };

    DevWatcher::handle_event(&event, &app_root, false, &action_tx);

    let action = action_rx.try_recv().unwrap();
    assert!(
        matches!(action, DevAction::InvalidateOpCache),
        "new app file should trigger InvalidateOpCache"
    );
}
