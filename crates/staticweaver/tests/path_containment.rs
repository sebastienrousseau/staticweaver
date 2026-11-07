// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Template and partial names must resolve inside the template root.
//!
//! The engine checks every layout, `extends` base and partial name before
//! loading it, and the filesystem loaders refuse a name that would leave
//! their root when called directly.

use staticweaver::engine::{FsLoader, TemplateLoader};
use staticweaver::{Context, Engine, EngineError};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

/// A template root `<tmp>/root` holding `ok.html` and `blog/post.html`,
/// with `secret.html` beside it, outside the root.
fn fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(root.join("blog")).unwrap();
    fs::write(root.join("ok.html"), "ok").unwrap();
    fs::write(root.join("blog/post.html"), "post").unwrap();
    fs::write(tmp.path().join("secret.html"), "SECRET").unwrap();
    tmp
}

fn root_of(tmp: &TempDir) -> String {
    tmp.path().join("root").to_string_lossy().into_owned()
}

fn is_refused(result: &Result<String, EngineError>) -> bool {
    matches!(
        result,
        Err(EngineError::InvalidTemplate(msg))
            if msg.contains("invalid template or partial name")
    )
}

fn render_partial(
    root: &str,
    name: &str,
) -> Result<String, EngineError> {
    let engine = Engine::new(root, Duration::from_secs(60));
    engine
        .render_template(&format!("{{{{> {name}}}}}"), &Context::new())
}

#[test]
fn names_that_leave_or_replace_the_root_are_refused() {
    let tmp = fixture();
    let root = root_of(&tmp);
    for name in ["../secret", "blog/../../secret", "..", "/etc/passwd"]
    {
        let partial = render_partial(&root, name);
        assert!(is_refused(&partial), "partial {name:?}: {partial:?}");
        let page = Engine::new(&root, Duration::from_secs(60))
            .render_page(&Context::new(), name);
        assert!(is_refused(&page), "layout {name:?}: {page:?}");
    }
}

#[test]
fn empty_and_nul_names_are_refused() {
    let tmp = fixture();
    let engine = Engine::new(&root_of(&tmp), Duration::from_secs(60));
    for name in ["", ".", "blog/..", "nul\0byte"] {
        let page = engine.render_page(&Context::new(), name);
        assert!(is_refused(&page), "{name:?}: {page:?}");
    }
}

#[test]
fn names_inside_the_root_render() {
    let tmp = fixture();
    let root = root_of(&tmp);
    assert_eq!(render_partial(&root, "ok").unwrap(), "ok");
    assert_eq!(render_partial(&root, "blog/post").unwrap(), "post");
    assert_eq!(render_partial(&root, "./ok").unwrap(), "ok");
}

#[test]
fn names_containing_dots_are_ordinary() {
    let tmp = fixture();
    let root = tmp.path().join("root");
    fs::write(root.join("a..b.html"), "dots").unwrap();
    fs::write(root.join("..hidden.html"), "hidden").unwrap();
    let root = root.to_string_lossy().into_owned();
    assert_eq!(render_partial(&root, "a..b").unwrap(), "dots");
    assert_eq!(render_partial(&root, "..hidden").unwrap(), "hidden");
}

#[test]
fn parent_steps_that_stay_inside_the_root_resolve() {
    let tmp = fixture();
    let root = root_of(&tmp);
    assert_eq!(render_partial(&root, "blog/../ok").unwrap(), "ok");
}

#[test]
fn fs_loader_refuses_names_outside_its_root() {
    let tmp = fixture();
    let loader = FsLoader::new(tmp.path().join("root"));
    assert_eq!(loader.load("blog/post").unwrap(), "post");
    for name in ["../secret", "blog/../../secret"] {
        let loaded = loader.load(name);
        assert!(is_refused(&loaded), "{name:?}: {loaded:?}");
    }
    let absolute = tmp.path().join("secret");
    let loaded = loader.load(&absolute.to_string_lossy());
    assert!(is_refused(&loaded), "{absolute:?}: {loaded:?}");
}

#[cfg(windows)]
#[test]
fn windows_drive_and_share_prefixes_are_refused() {
    let tmp = fixture();
    let root = root_of(&tmp);
    let secret = tmp.path().join("secret");
    let names = [
        secret.to_string_lossy().into_owned(),
        r"C:\Windows\win".to_string(),
        "C:relative".to_string(),
        r"\\server\share\x".to_string(),
    ];
    for name in &names {
        let page = Engine::new(&root, Duration::from_secs(60))
            .render_page(&Context::new(), name);
        assert!(is_refused(&page), "{name:?}: {page:?}");
    }
}

#[cfg(unix)]
#[test]
fn backslashes_are_ordinary_on_unix() {
    let tmp = fixture();
    let root = tmp.path().join("root");
    fs::write(root.join(r"a\..\b.html"), "literal").unwrap();
    let root = root.to_string_lossy().into_owned();
    assert_eq!(render_partial(&root, r"a\..\b").unwrap(), "literal");
    assert!(!std::path::Path::new(&root)
        .join(r"..\secret.html")
        .exists());
}
