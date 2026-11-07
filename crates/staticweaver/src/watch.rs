// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Watching loader (issue #52)
//!
//! [`WatchLoader`](crate::watch::WatchLoader) wraps any
//! [`TemplateLoader`](crate::engine::TemplateLoader), caches what it loads,
//! and drops the cache when files under a watched directory change.
//! Change events from the operating system (via `notify`) are
//! debounced: a burst of events (an editor's save writes a temporary
//! file, renames it, touches metadata) becomes one batch once no new
//! event has arrived for the debounce window, 200 ms by default.
//!
//! Each batch also goes to every [`WatchLoader::subscribe`](crate::watch::WatchLoader::subscribe) receiver,
//! so a host can clear its own caches (for example
//! [`Engine::clear_cache`](crate::Engine::clear_cache), whose rendered
//! pages embed partials) and re-render.
//!
//! Gated behind the `watch` feature.

use crate::engine::TemplateLoader;
use crate::EngineError;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

/// The default debounce window: a batch is delivered once no change
/// event has arrived for this long.
///
/// # Examples
///
/// ```
/// use staticweaver::watch::DEFAULT_DEBOUNCE;
/// assert_eq!(DEFAULT_DEBOUNCE.as_millis(), 200);
/// ```
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);

/// State shared with the debounce thread.
#[derive(Default)]
struct Shared {
    cache: Mutex<HashMap<String, String>>,
    subscribers: Mutex<Vec<Sender<Vec<PathBuf>>>>,
    generation: AtomicU64,
}

impl Shared {
    /// Drops every cached body, bumps the generation and tells the
    /// subscribers which paths changed. Receivers that hung up are
    /// forgotten.
    fn flush(&self, paths: Vec<PathBuf>) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let _ = self.generation.fetch_add(1, Ordering::SeqCst);
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|tx| tx.send(paths.clone()).is_ok());
    }
}

/// The paths changed by one debounced burst of events. Access events
/// (a file being opened or read, which loading a template itself
/// causes on Linux) are ignored; a watcher error counts as a change
/// with no path, since something may have been missed.
#[derive(Default)]
struct Batch {
    changed: bool,
    paths: Vec<PathBuf>,
}

impl Batch {
    fn add(&mut self, event: notify::Result<Event>) {
        match event {
            Ok(ev) if matches!(ev.kind, EventKind::Access(_)) => {}
            Ok(ev) => {
                self.changed = true;
                self.paths.extend(ev.paths);
            }
            Err(_) => self.changed = true,
        }
    }

    fn into_paths(mut self) -> Option<Vec<PathBuf>> {
        if !self.changed {
            return None;
        }
        self.paths.sort();
        self.paths.dedup();
        Some(self.paths)
    }
}

/// Collects one burst: the first event, then every event that follows
/// within `debounce` of the previous one. Returns the batch and whether
/// the event source is still connected.
fn collect_burst(
    first: notify::Result<Event>,
    rx: &Receiver<notify::Result<Event>>,
    debounce: Duration,
) -> (Batch, bool) {
    let mut batch = Batch::default();
    batch.add(first);
    loop {
        match rx.recv_timeout(debounce) {
            Ok(event) => batch.add(event),
            Err(RecvTimeoutError::Timeout) => return (batch, true),
            Err(RecvTimeoutError::Disconnected) => {
                return (batch, false)
            }
        }
    }
}

/// The debounce thread: runs until the event source hangs up (the
/// watcher, owned by the `WatchLoader`, is dropped).
fn debounce_loop(
    rx: &Receiver<notify::Result<Event>>,
    shared: &Shared,
    debounce: Duration,
) {
    while let Ok(first) = rx.recv() {
        let (batch, connected) = collect_burst(first, rx, debounce);
        if let Some(paths) = batch.into_paths() {
            shared.flush(paths);
        }
        if !connected {
            return;
        }
    }
}

/// A [`TemplateLoader`] that caches another loader's templates and
/// invalidates the cache when files under a watched directory change.
///
/// # Examples
///
/// ```no_run
/// use staticweaver::engine::FsLoader;
/// use staticweaver::watch::WatchLoader;
/// use staticweaver::{Context, Engine};
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// let loader = Arc::new(WatchLoader::new(FsLoader::new("templates".into()), "templates")?);
/// let changes = loader.subscribe();
/// let engine = Engine::with_loader(loader, Duration::from_secs(60));
/// for changed in changes {
///     engine.clear_cache(); // rendered pages embed partials too
///     println!("{changed:?} changed: {}", engine.render_page(&Context::new(), "index")?);
/// }
/// # Ok::<(), staticweaver::EngineError>(())
/// ```
pub struct WatchLoader<L> {
    inner: L,
    shared: Arc<Shared>,
    // Dropping the watcher ends the debounce thread. `Mutex` keeps the
    // loader `Sync` whatever the platform watcher type is.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl<L> fmt::Debug for WatchLoader<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatchLoader")
            .field(
                "generation",
                &self.shared.generation.load(Ordering::SeqCst),
            )
            .finish_non_exhaustive()
    }
}

impl<L: TemplateLoader> WatchLoader<L> {
    /// Wraps `inner` and watches `root` recursively, with the
    /// [`DEFAULT_DEBOUNCE`] window.
    ///
    /// # Errors
    ///
    /// Returns `EngineError::Io` when the platform watcher cannot be
    /// created or `root` cannot be watched (for example, it does not
    /// exist).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use staticweaver::watch::WatchLoader;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let loader = WatchLoader::new(FsLoader::new(dir.path().into()), dir.path());
    /// assert!(loader.is_ok());
    /// assert!(WatchLoader::new(FsLoader::new("x".into()), "/no/such/dir").is_err());
    /// ```
    pub fn new(
        inner: L,
        root: impl AsRef<Path>,
    ) -> Result<Self, EngineError> {
        Self::with_debounce(inner, root, DEFAULT_DEBOUNCE)
    }

    /// As [`WatchLoader::new`], with a custom debounce window.
    ///
    /// # Errors
    ///
    /// As [`WatchLoader::new`].
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use staticweaver::watch::WatchLoader;
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let loader = WatchLoader::with_debounce(
    ///     FsLoader::new(dir.path().into()),
    ///     dir.path(),
    ///     Duration::from_millis(50),
    /// );
    /// assert!(loader.is_ok());
    /// ```
    pub fn with_debounce(
        inner: L,
        root: impl AsRef<Path>,
        debounce: Duration,
    ) -> Result<Self, EngineError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(tx).map_err(io_error)?;
        watcher
            .watch(root.as_ref(), RecursiveMode::Recursive)
            .map_err(io_error)?;
        let loader = Self::from_events(inner, rx, debounce);
        *loader
            .watcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(watcher);
        Ok(loader)
    }

    /// Builds the loader around an event source and starts the
    /// debounce thread. The OS watcher (if any) is attached after.
    fn from_events(
        inner: L,
        rx: Receiver<notify::Result<Event>>,
        debounce: Duration,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let thread_shared = Arc::clone(&shared);
        let _ = thread::Builder::new()
            .name("staticweaver-watch".into())
            .spawn(move || {
                debounce_loop(&rx, &thread_shared, debounce)
            });
        Self {
            inner,
            shared,
            watcher: Mutex::new(None),
        }
    }

    /// Returns a receiver that gets the changed paths of every
    /// debounced batch (an empty list after a watcher error).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use staticweaver::watch::WatchLoader;
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let loader = WatchLoader::new(FsLoader::new(dir.path().into()), dir.path()).unwrap();
    /// let changes = loader.subscribe();
    /// // Nothing has changed yet.
    /// assert!(changes.recv_timeout(Duration::from_millis(10)).is_err());
    /// ```
    pub fn subscribe(&self) -> Receiver<Vec<PathBuf>> {
        let (tx, rx) = mpsc::channel();
        self.shared
            .subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(tx);
        rx
    }

    /// The number of debounced batches delivered so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use staticweaver::watch::WatchLoader;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let loader = WatchLoader::new(FsLoader::new(dir.path().into()), dir.path()).unwrap();
    /// assert_eq!(loader.generation(), 0);
    /// ```
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::SeqCst)
    }

    /// Drops every cached template now, as if a change had been seen.
    /// Subscribers are told with an empty path list.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use staticweaver::watch::WatchLoader;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let loader = WatchLoader::new(FsLoader::new(dir.path().into()), dir.path()).unwrap();
    /// loader.invalidate();
    /// assert_eq!(loader.generation(), 1);
    /// ```
    pub fn invalidate(&self) {
        self.shared.flush(Vec::new());
    }
}

impl<L: TemplateLoader> TemplateLoader for WatchLoader<L> {
    fn load(&self, name: &str) -> Result<String, EngineError> {
        if let Some(body) = self
            .shared
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
        {
            return Ok(body.clone());
        }
        // Only cache what was read before any invalidation that ran
        // while we were reading: otherwise a stale body could outlive
        // the change that should have evicted it.
        let before = self.generation();
        let body = self.inner.load(name)?;
        let mut cache = self
            .shared
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.generation() == before {
            let _ = cache.insert(name.to_string(), body.clone());
        }
        Ok(body)
    }
}

fn io_error(e: notify::Error) -> EngineError {
    EngineError::Io(io::Error::other(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::MemoryLoader;
    use std::sync::atomic::AtomicUsize;

    /// Counts loads so tests can see whether the cache answered.
    struct Counting {
        inner: MemoryLoader,
        loads: AtomicUsize,
    }

    impl TemplateLoader for Counting {
        fn load(&self, name: &str) -> Result<String, EngineError> {
            let _ = self.loads.fetch_add(1, Ordering::SeqCst);
            self.inner.load(name)
        }
    }

    fn loader(
        debounce: Duration,
    ) -> (WatchLoader<Counting>, Sender<notify::Result<Event>>) {
        let mut inner = MemoryLoader::default();
        let _ = inner.insert("page", "v1");
        let (tx, rx) = mpsc::channel();
        let counting = Counting {
            inner,
            loads: AtomicUsize::new(0),
        };
        (WatchLoader::from_events(counting, rx, debounce), tx)
    }

    fn modify(path: &str) -> notify::Result<Event> {
        Ok(Event::new(EventKind::Modify(
            notify::event::ModifyKind::Any,
        ))
        .add_path(PathBuf::from(path)))
    }

    fn access(path: &str) -> notify::Result<Event> {
        Ok(Event::new(EventKind::Access(
            notify::event::AccessKind::Any,
        ))
        .add_path(PathBuf::from(path)))
    }

    /// Generous: the debounce window here is 20 ms.
    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn caches_until_a_change_arrives() {
        let (l, tx) = loader(Duration::from_millis(20));
        let changes = l.subscribe();
        assert_eq!(l.load("page").unwrap(), "v1");
        assert_eq!(l.load("page").unwrap(), "v1");
        assert_eq!(l.inner.loads.load(Ordering::SeqCst), 1);

        tx.send(modify("t/page.html")).unwrap();
        let batch = changes.recv_timeout(WAIT).unwrap();
        assert_eq!(batch, vec![PathBuf::from("t/page.html")]);
        assert_eq!(l.generation(), 1);

        assert_eq!(l.load("page").unwrap(), "v1");
        assert_eq!(l.inner.loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_burst_becomes_one_batch() {
        let (l, tx) = loader(Duration::from_millis(300));
        let changes = l.subscribe();
        for p in ["t/b.html", "t/a.html", "t/b.html"] {
            tx.send(modify(p)).unwrap();
        }
        let batch = changes.recv_timeout(WAIT).unwrap();
        assert_eq!(
            batch,
            vec![PathBuf::from("t/a.html"), PathBuf::from("t/b.html")]
        );
        assert_eq!(l.generation(), 1);
    }

    #[test]
    fn access_events_do_not_invalidate() {
        let (l, tx) = loader(Duration::from_millis(20));
        let changes = l.subscribe();
        tx.send(access("t/page.html")).unwrap();
        // A real change afterwards is the first batch delivered.
        tx.send(modify("t/x.html")).unwrap();
        let batch = changes.recv_timeout(WAIT).unwrap();
        assert_eq!(batch, vec![PathBuf::from("t/x.html")]);
        assert_eq!(l.generation(), 1);
    }

    #[test]
    fn an_access_only_burst_is_dropped() {
        let (l, tx) = loader(Duration::from_millis(20));
        let changes = l.subscribe();
        tx.send(access("t/page.html")).unwrap();
        drop(tx);
        // The thread exits once the source hangs up, without a batch.
        // (A short wait: a late batch would make this pass wrongly,
        // never fail spuriously.)
        assert!(changes
            .recv_timeout(Duration::from_millis(300))
            .is_err());
        assert_eq!(l.generation(), 0);
    }

    #[test]
    fn a_watcher_error_invalidates_with_no_paths() {
        let (l, tx) = loader(Duration::from_millis(20));
        let changes = l.subscribe();
        tx.send(Err(notify::Error::generic("lost events"))).unwrap();
        assert_eq!(
            changes.recv_timeout(WAIT).unwrap(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn a_pending_burst_is_flushed_when_the_source_hangs_up() {
        let (l, tx) = loader(Duration::from_secs(60));
        let changes = l.subscribe();
        tx.send(modify("t/page.html")).unwrap();
        drop(tx);
        assert_eq!(changes.recv_timeout(WAIT).unwrap().len(), 1);
    }

    #[test]
    fn dropped_subscribers_are_forgotten() {
        let (l, _tx) = loader(Duration::from_millis(20));
        drop(l.subscribe());
        l.invalidate();
        assert!(l.shared.subscribers.lock().unwrap().is_empty());
    }

    #[test]
    fn load_errors_pass_through_uncached() {
        let (l, _tx) = loader(Duration::from_millis(20));
        assert!(l.load("missing").is_err());
        assert!(l.shared.cache.lock().unwrap().is_empty());
        assert!(format!("{l:?}").contains("WatchLoader"));
    }

    #[test]
    fn io_errors_wrap_notify_errors() {
        let e = io_error(notify::Error::generic("boom"));
        assert!(matches!(e, EngineError::Io(_)), "{e:?}");
    }
}
