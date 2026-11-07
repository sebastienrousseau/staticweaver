// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! OS-DEPENDENT: these tests use the platform's real file watcher
//! (inotify, FSEvents, ReadDirectoryChangesW) through `notify`. The
//! debounce and invalidation logic is covered deterministically, with
//! synthetic events, by the unit tests in `src/watch.rs`; these only
//! check that real edits reach it. Timeouts are generous (20 s) so a
//! slow CI runner does not turn latency into a failure.

#![cfg(feature = "watch")]

use staticweaver::engine::{FsLoader, TemplateLoader};
use staticweaver::watch::WatchLoader;
use staticweaver::{Context, Engine};
use std::sync::Arc;
use std::time::Duration;

const GENEROUS: Duration = Duration::from_secs(20);

#[test]
fn os_watcher_reports_an_edit_and_the_reload_sees_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("page.html"), "v1 {{x}}").unwrap();
    let loader = Arc::new(
        WatchLoader::new(FsLoader::new(dir.path().into()), dir.path())
            .unwrap(),
    );
    let changes = loader.subscribe();
    let engine =
        Engine::with_loader(loader.clone(), Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "!".to_string());
    assert_eq!(engine.render_page(&ctx, "page").unwrap(), "v1 !");

    std::fs::write(dir.path().join("page.html"), "v2 {{x}}").unwrap();
    let batch = changes.recv_timeout(GENEROUS).expect("a change batch");
    assert!(loader.generation() >= 1, "batch {batch:?}");

    engine.clear_cache();
    assert_eq!(engine.render_page(&ctx, "page").unwrap(), "v2 !");
}

#[test]
fn os_watcher_sees_new_partials_in_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("partials")).unwrap();
    let loader =
        WatchLoader::new(FsLoader::new(dir.path().into()), dir.path())
            .unwrap();
    let changes = loader.subscribe();
    assert!(loader.load("partials/nav").is_err());

    std::fs::write(dir.path().join("partials/nav.html"), "nav")
        .unwrap();
    let _ = changes.recv_timeout(GENEROUS).expect("a change batch");
    assert_eq!(loader.load("partials/nav").unwrap(), "nav");
}
