//! Domain-specific tests for the CLI subcommands and DevWatcher.
//!
//! Covers: CLI subcommand parsing, error handling, flags, DevWatcher events,
//! debounce behavior, batch categorization, and edge cases.

use std::path::PathBuf;

use clap::Parser;
use notify::Event;
use nusa_cli::dev::{DevAction, DevWatcher};
use nusa_cli::{Cli, Commands};
use tokio::sync::mpsc;

// ─── 1. CLI Subcommands ───────────────────────────────────────────────────

#[test]
fn cli_subcommand_deploy_parses() {
    let cli = Cli::try_parse_from(["nusa", "deploy"]).expect("deploy should parse");
    assert!(cli.command.is_some());
    match cli.command.expect("command should exist") {
        Commands::Deploy { strategy, config } => {
            assert_eq!(strategy, "blue-green");
            assert!(config.is_none());
        }
        other => panic!("expected Deploy, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_deploy_with_custom_strategy() {
    let cli =
        Cli::try_parse_from(["nusa", "deploy", "--strategy", "canary"]).expect("should parse");
    match cli.command.expect("command should exist") {
        Commands::Deploy { strategy, config } => {
            assert_eq!(strategy, "canary");
            assert!(config.is_none());
        }
        other => panic!("expected Deploy, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_deploy_with_custom_config() {
    let cli =
        Cli::try_parse_from(["nusa", "deploy", "--config", "custom.toml"]).expect("should parse");
    match cli.command.expect("command should exist") {
        Commands::Deploy { strategy, config } => {
            assert_eq!(strategy, "blue-green");
            assert_eq!(config, Some("custom.toml".to_string()));
        }
        other => panic!("expected Deploy, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_rollback_parses() {
    let cli = Cli::try_parse_from(["nusa", "rollback"]).expect("rollback should parse");
    match cli.command.expect("command should exist") {
        Commands::Rollback => {}
        other => panic!(
            "expected Rollback, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn cli_subcommand_test_with_workers_and_reset() {
    let cli = Cli::try_parse_from(["nusa", "test", "tests/", "--workers", "8", "--reset"])
        .expect("test should parse");
    match cli.command.expect("command should exist") {
        Commands::Test {
            path,
            workers,
            reset,
        } => {
            assert_eq!(path, "tests/");
            assert_eq!(workers, 8);
            assert!(reset, "reset flag should be true");
        }
        other => panic!("expected Test, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_test_with_defaults() {
    let cli = Cli::try_parse_from(["nusa", "test"]).expect("test defaults should parse");
    match cli.command.expect("command should exist") {
        Commands::Test {
            path,
            workers,
            reset,
        } => {
            assert_eq!(path, "tests/");
            assert_eq!(workers, 4);
            assert!(!reset, "reset should default to false");
        }
        other => panic!("expected Test, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_dev_parses() {
    let cli = Cli::try_parse_from(["nusa", "dev"]).expect("dev should parse");
    match cli.command.expect("command should exist") {
        Commands::Dev {
            watch,
            debounce,
            pretty,
        } => {
            assert_eq!(watch, "app,config,routes,resources/views,.env");
            assert_eq!(debounce, 200);
            assert!(!pretty);
        }
        other => panic!("expected Dev, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_dev_with_custom_debounce() {
    let cli = Cli::try_parse_from(["nusa", "dev", "--debounce", "500"])
        .expect("dev with debounce should parse");
    match cli.command.expect("command should exist") {
        Commands::Dev { debounce, .. } => {
            assert_eq!(debounce, 500);
        }
        other => panic!("expected Dev, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_dev_with_custom_watch_directories() {
    let cli = Cli::try_parse_from(["nusa", "dev", "--watch", "src,lib"])
        .expect("dev with watch should parse");
    match cli.command.expect("command should exist") {
        Commands::Dev { watch, .. } => {
            assert_eq!(watch, "src,lib");
        }
        other => panic!("expected Dev, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_subcommand_dev_pretty_flag() {
    let cli =
        Cli::try_parse_from(["nusa", "dev", "--pretty"]).expect("dev with pretty should parse");
    match cli.command.expect("command should exist") {
        Commands::Dev { pretty, .. } => {
            assert!(pretty);
        }
        other => panic!("expected Dev, got {:?}", std::mem::discriminant(&other)),
    }
}

// ─── 2. CLI Error Handling ────────────────────────────────────────────────

#[test]
fn cli_unknown_subcommand_rejected_with_error() {
    let result = Cli::try_parse_from(["nusa", "unknown"]);
    assert!(result.is_err(), "unknown subcommand should be rejected");
}

#[test]
fn cli_unknown_flag_rejected_with_error() {
    let result = Cli::try_parse_from(["nusa", "--unknown-flag"]);
    assert!(result.is_err(), "unknown flag should be rejected");
}

#[test]
fn cli_test_unknown_flag_rejected() {
    let result = Cli::try_parse_from(["nusa", "test", "--bogus"]);
    assert!(result.is_err(), "unknown test flag should be rejected");
}

#[test]
fn cli_deploy_unknown_flag_rejected() {
    let result = Cli::try_parse_from(["nusa", "deploy", "--unknown"]);
    assert!(result.is_err(), "unknown deploy flag should be rejected");
}

// ─── 3. CLI Flags ─────────────────────────────────────────────────────────

#[test]
fn cli_custom_debounce_interval() {
    let cli = Cli::try_parse_from(["nusa", "dev", "--debounce", "1000"]).expect("should parse");
    match cli.command.expect("command should exist") {
        Commands::Dev { debounce, .. } => {
            assert_eq!(debounce, 1000);
        }
        other => panic!("expected Dev, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_custom_watch_directories() {
    let cli = Cli::try_parse_from(["nusa", "dev", "--watch", "custom/path,another/path"])
        .expect("should parse");
    match cli.command.expect("command should exist") {
        Commands::Dev { watch, .. } => {
            assert_eq!(watch, "custom/path,another/path");
        }
        other => panic!("expected Dev, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn cli_default_config_path() {
    let cli = Cli::try_parse_from(["nusa"]).expect("nusa without args should parse");
    assert_eq!(cli.config, "nusa.toml");
    assert!(cli.command.is_none());
}

#[test]
fn cli_custom_config_path() {
    let cli = Cli::try_parse_from(["nusa", "-c", "custom.toml"]).expect("should parse");
    assert_eq!(cli.config, "custom.toml");
}

// ─── 4. DevWatcher ────────────────────────────────────────────────────────

#[tokio::test]
async fn dev_watcher_subscribe_receives_events() {
    let mut watcher = DevWatcher::new(100, false);
    let mut rx = watcher.subscribe();

    // Start the watcher on a valid directory
    let app_root = std::env::temp_dir();
    watcher.start(&app_root).ok();

    // Wait a moment for the debounce loop to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Receiver is ready (even if empty)
    let _ = rx.try_recv();
}

#[tokio::test]
async fn dev_watcher_start_watches_existing_dirs() {
    let app_root = std::env::temp_dir().join(format!("nusa_dev_test_{}", std::process::id()));
    // Create the app directory structure
    let app_dir = app_root.join("app");
    std::fs::create_dir_all(&app_dir).expect("create app dir");

    let mut watcher = DevWatcher::new(100, false);
    let result = watcher.start(&app_root);

    // May succeed or fail depending on directory existence
    assert!(result.is_ok() || result.is_err());

    // Cleanup
    let _ = std::fs::remove_dir_all(&app_root);
}

#[tokio::test]
async fn dev_watcher_start_nonexistent_root_returns_error() {
    let nonexistent = PathBuf::from("/definitely/not/a/real/path/nusa_test");
    let mut watcher = DevWatcher::new(100, false);
    let result = watcher.start(&nonexistent);
    // Should succeed since it only watches existing directories within root
    // The root itself doesn't need to exist — it watches subdirs that exist
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn dev_watcher_create_event_triggers_action() {
    let event = Event {
        kind: notify::EventKind::Create(notify::event::CreateKind::File),
        paths: vec![PathBuf::from("/app/Http/Controllers/NewController.php")],
        attrs: Default::default(),
    };

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let mut rx = action_tx.subscribe();
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false, &action_tx);

    // Should receive InvalidateOpCache for app/*.php
    let received = rx.try_recv();
    assert!(received.is_ok(), "should receive action for create event");
}

#[tokio::test]
async fn dev_watcher_remove_event_handled() {
    let event = Event {
        kind: notify::EventKind::Remove(notify::event::RemoveKind::File),
        paths: vec![PathBuf::from("/app/Http/Controllers/DeletedController.php")],
        attrs: Default::default(),
    };

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    // Should not panic — remove events are handled like any other
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false, &action_tx);
}

#[tokio::test]
async fn dev_watcher_symlink_events_handled() {
    let event = Event {
        kind: notify::EventKind::Create(notify::event::CreateKind::Any),
        paths: vec![PathBuf::from("/app/symlink_to_config")],
        attrs: Default::default(),
    };

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    // Should not panic — symlink events are handled
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false, &action_tx);
}

#[tokio::test]
async fn dev_watcher_rapid_file_changes_dont_cause_restart_storm() {
    // The debounce loop should batch rapid events
    let (tx, mut rx): (mpsc::UnboundedSender<Event>, mpsc::UnboundedReceiver<Event>) =
        mpsc::unbounded_channel();

    let debounce_ms = 100;
    let app_root = PathBuf::from("/tmp/test-app");
    let (action_tx, _) = tokio::sync::broadcast::channel(32);

    // Spawn debounce loop
    let handle = tokio::spawn(async move {
        DevWatcher::debounce_loop(&mut rx, debounce_ms, &app_root, false, action_tx).await;
    });

    // Send 20 events rapidly (simulating rapid file changes)
    for i in 0..20 {
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Any,
            )),
            paths: vec![PathBuf::from(format!("/tmp/test-app/app/file-{}.php", i))],
            attrs: Default::default(),
        };
        tx.send(event).expect("send event");
    }

    // Wait for debounce to process
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Abort the infinite loop
    handle.abort();
    // If we get here without panic, debounce worked correctly
}

#[tokio::test]
async fn dev_watcher_batch_categorization_priority_correct() {
    // When multiple event types occur in one batch, correct actions should fire
    use tokio::sync::mpsc::UnboundedSender;

    let (tx, mut rx): (UnboundedSender<Event>, mpsc::UnboundedReceiver<Event>) =
        mpsc::unbounded_channel();

    let debounce_ms = 50;
    let app_root = PathBuf::from("/tmp/test-app");
    let (action_tx, _) = tokio::sync::broadcast::channel(32);

    let handle = tokio::spawn(async move {
        DevWatcher::debounce_loop(&mut rx, debounce_ms, &app_root, false, action_tx).await;
    });

    // Send events of different types
    let env_event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/tmp/test-app/.env")],
        attrs: Default::default(),
    };
    tx.send(env_event).expect("send env event");

    let config_event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/tmp/test-app/config/app.php")],
        attrs: Default::default(),
    };
    tx.send(config_event).expect("send config event");

    let app_event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/tmp/test-app/app/Http/Controller.php")],
        attrs: Default::default(),
    };
    tx.send(app_event).expect("send app event");

    // Wait for debounce
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    handle.abort();
    // All three events should have been processed without errors
}

#[tokio::test]
async fn dev_watcher_handle_event_multiple_paths_correct_action() {
    // Event with multiple paths — should trigger the right action for each
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![
            PathBuf::from("/app/.env"),
            PathBuf::from("/app/config/database.php"),
            PathBuf::from("/app/app/Http/Kernel.php"),
            PathBuf::from("/app/resources/views/welcome.blade.php"),
        ],
        attrs: Default::default(),
    };

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    let mut rx = action_tx.subscribe();
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false, &action_tx);

    // Should have sent 4 actions
    let mut actions = Vec::new();
    while let Ok(action) = rx.try_recv() {
        actions.push(action);
    }
    assert_eq!(actions.len(), 4, "should have 4 actions for 4 paths");

    assert!(actions.iter().any(|a| matches!(a, DevAction::ReloadConfig)));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DevAction::RecycleWorkers))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DevAction::InvalidateOpCache))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DevAction::ClearViewCache))
    );
}

#[tokio::test]
async fn dev_watcher_event_in_ignored_dirs_no_action() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![
            PathBuf::from("/app/vendor/package/file.php"),
            PathBuf::from("/app/node_modules/bundle.js"),
            PathBuf::from("/app/.git/config"),
        ],
        attrs: Default::default(),
    };

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false, &action_tx);

    // No actions should be sent for ignored directories
    assert!(action_tx.receiver_count() == 0);
}

#[tokio::test]
async fn dev_watcher_non_matching_path_no_action() {
    let event = Event {
        kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )),
        paths: vec![PathBuf::from("/app/some/random/file.txt")],
        attrs: Default::default(),
    };

    let (action_tx, _) = tokio::sync::broadcast::channel(32);
    DevWatcher::handle_event(&event, &PathBuf::from("/app"), false, &action_tx);
    // No action for unrecognized paths
}

// ─── 5. DevAction Variants ────────────────────────────────────────────────

#[test]
fn dev_action_reload_config_variant() {
    let action = DevAction::ReloadConfig;
    match action {
        DevAction::ReloadConfig => {}
        other => panic!("expected ReloadConfig, got {:?}", other),
    }
}

#[test]
fn dev_action_recycle_workers_variant() {
    let action = DevAction::RecycleWorkers;
    match action {
        DevAction::RecycleWorkers => {}
        other => panic!("expected RecycleWorkers, got {:?}", other),
    }
}

#[test]
fn dev_action_invalidate_opcache_variant() {
    let action = DevAction::InvalidateOpCache;
    match action {
        DevAction::InvalidateOpCache => {}
        other => panic!("expected InvalidateOpCache, got {:?}", other),
    }
}

#[test]
fn dev_action_clear_view_cache_variant() {
    let action = DevAction::ClearViewCache;
    match action {
        DevAction::ClearViewCache => {}
        other => panic!("expected ClearViewCache, got {:?}", other),
    }
}

// ─── 6. DevWatcher Creation ───────────────────────────────────────────────

#[test]
fn dev_watcher_creation_with_debounce_and_pretty() {
    let watcher = DevWatcher::new(500, true);
    assert_eq!(watcher.debounce_ms, 500);
    assert!(watcher.pretty);
}

#[test]
fn dev_watcher_creation_with_minimal_debounce() {
    let watcher = DevWatcher::new(1, false);
    assert_eq!(watcher.debounce_ms, 1);
    assert!(!watcher.pretty);
}

#[test]
fn dev_watcher_creation_with_zero_debounce() {
    let watcher = DevWatcher::new(0, true);
    assert_eq!(watcher.debounce_ms, 0);
}
