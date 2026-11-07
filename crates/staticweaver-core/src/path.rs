// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Template-name containment: the rule that keeps a template, partial
//! or `{{#extends}}` name inside the template root.
//!
//! With the `std` feature the rule is `euxis_commons::path::safe_join`
//! (platform-aware: Windows drive and share prefixes are refused). Without
//! `std` there is no platform path type, so a lexical rule over `/` and
//! `\` separators applies: absolute names, drive prefixes and `..` steps
//! that climb above the root are refused.

use crate::error::EngineError;
use alloc::format;

/// The error every refused template or partial name produces.
fn invalid_name(path: &str) -> EngineError {
    EngineError::InvalidTemplate(format!(
        "invalid template or partial name: {path:?}"
    ))
}

/// Joins the untrusted template name `name` under `root`, refusing a
/// name that is absolute, carries a Windows drive or share prefix, or
/// whose `..` steps climb above `root`. The rule is
/// `euxis_commons::path::safe_join`: lexical, so `.` is dropped, `..`
/// removes the previous component, and names such as `a..b` are
/// ordinary. Symlinks inside `root` are followed as before.
///
/// # Errors
///
/// Returns `EngineError::InvalidTemplate` for a refused name.
///
/// # Examples
///
/// ```
/// use staticweaver_core::path::resolve_template_path;
/// use std::path::{Path, PathBuf};
///
/// let p = resolve_template_path(Path::new("t"), "blog/post.html").unwrap();
/// assert_eq!(p, PathBuf::from("t/blog/post.html"));
/// assert!(resolve_template_path(Path::new("t"), "../secret").is_err());
/// ```
#[cfg(feature = "std")]
pub fn resolve_template_path(
    root: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, EngineError> {
    if name.contains('\0') {
        return Err(invalid_name(name));
    }
    commons::path::safe_join(root, name).map_err(|_| invalid_name(name))
}

/// Validates that a template or partial name is safe and does not
/// attempt to escape the template directory.
///
/// Rejects names that are empty or resolve to the root itself (`.`,
/// `blog/..`), contain a null byte, are absolute or prefixed, or climb
/// above the root.
///
/// # Errors
///
/// Returns `EngineError::InvalidTemplate` for a refused name.
///
/// # Examples
///
/// ```
/// use staticweaver_core::path::validate_name;
///
/// assert!(validate_name("blog/post").is_ok());
/// assert!(validate_name("../etc/passwd").is_err());
/// assert!(validate_name("").is_err());
/// ```
pub fn validate_name(path: &str) -> Result<(), EngineError> {
    if names_root_or_escapes(path) {
        return Err(invalid_name(path));
    }
    Ok(())
}

#[cfg(feature = "std")]
fn names_root_or_escapes(path: &str) -> bool {
    resolve_template_path(std::path::Path::new(""), path)
        .map_or(true, |resolved| resolved.as_os_str().is_empty())
}

#[cfg(not(feature = "std"))]
fn names_root_or_escapes(path: &str) -> bool {
    lexical_refuses(path)
}

/// The rule without a platform path type.
#[cfg(any(test, not(feature = "std")))]
fn lexical_refuses(path: &str) -> bool {
    is_absolute_or_nul(path)
        || kept_components(path).is_none_or(|n| n == 0)
}

/// A leading separator, a drive prefix (`C:`), or a NUL byte.
#[cfg(any(test, not(feature = "std")))]
fn is_absolute_or_nul(path: &str) -> bool {
    let drive = path.as_bytes().get(1) == Some(&b':');
    path.starts_with(['/', '\\']) || drive || path.contains('\0')
}

/// How many components remain after `.` is dropped and each `..`
/// removes the previous one; `None` when a `..` climbs above the root.
#[cfg(any(test, not(feature = "std")))]
fn kept_components(path: &str) -> Option<usize> {
    let mut kept = 0usize;
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => kept = kept.checked_sub(1)?,
            _ => kept += 1,
        }
    }
    Some(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The no_std rule refuses what the std rule refuses, for names
    /// written with `/` (the separator both rules share on Unix).
    #[cfg(unix)]
    #[test]
    fn lexical_rule_agrees_with_safe_join() {
        for name in [
            "page",
            "blog/post",
            "./a",
            "a/../b",
            "a..b",
            "..hidden",
            "",
            ".",
            "..",
            "../x",
            "a/../..",
            "/etc/passwd",
            "blog/..",
            "a/./b/../c",
            "nul\0byte",
        ] {
            assert_eq!(
                lexical_refuses(name),
                names_root_or_escapes(name),
                "{name:?}"
            );
        }
    }

    #[test]
    fn lexical_rule_refuses_drive_and_backslash_prefixes() {
        assert!(lexical_refuses("C:\\x"));
        assert!(lexical_refuses("C:x"));
        assert!(lexical_refuses("\\\\server\\share"));
        assert!(!lexical_refuses("a\\b"));
    }
}
