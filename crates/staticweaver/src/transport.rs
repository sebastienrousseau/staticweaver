// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transport policy for the opt-in remote-template fetcher.
//!
//! Remote templates are HTML and JavaScript that end up rendered into a
//! site, so fetching them over cleartext HTTP lets anyone on the path
//! rewrite them. The fetcher therefore accepts `https://` URLs, plus
//! `http://` only for loopback hosts (`localhost`, `127.0.0.0/8`,
//! `::1`), where no network sits between client and server. The same
//! rule is applied to every redirect hop, so an HTTPS URL cannot be
//! downgraded by a `Location: http://...` response.

use crate::error::EngineError;
use reqwest::redirect::{Attempt, Policy};
use reqwest::Url;
use std::net::IpAddr;

/// Redirect hops followed before giving up (reqwest's own default).
const MAX_REDIRECTS: usize = 10;

/// `true` when `host` names the local machine.
fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// `true` when `url` may be fetched: HTTPS anywhere, HTTP on loopback.
fn is_permitted(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => url.host_str().is_some_and(is_loopback_host),
        _ => false,
    }
}

/// Rejects a template URL that the fetcher must not request.
pub(crate) fn require_permitted(url: &str) -> Result<(), EngineError> {
    let parsed = Url::parse(url).map_err(|e| {
        EngineError::InvalidTemplate(format!(
            "invalid template URL {url}: {e}"
        ))
    })?;
    if is_permitted(&parsed) {
        Ok(())
    } else {
        Err(EngineError::InvalidTemplate(format!(
            "remote template URL {url} must use https \
             (plain http is accepted only for loopback hosts)"
        )))
    }
}

/// Redirect policy that applies [`is_permitted`] to every hop.
pub(crate) fn redirect_policy() -> Policy {
    Policy::custom(|attempt: Attempt<'_>| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        if is_permitted(attempt.url()) {
            return attempt.follow();
        }
        let msg = format!(
            "refusing plain-HTTP redirect to {}",
            attempt.url()
        );
        attempt.error(msg)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_is_permitted_anywhere() {
        assert!(require_permitted("https://example.com/t").is_ok());
    }

    #[test]
    fn http_is_permitted_on_loopback_only() {
        for ok in [
            "http://localhost:8080/t",
            "http://LOCALHOST/t",
            "http://127.0.0.1:1234",
            "http://127.8.9.10/t",
            "http://[::1]:9000/t",
        ] {
            assert!(require_permitted(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://example.com/t",
            "http://10.0.0.1/t",
            "http://localhost.example.com/t",
            "http://[::2]/t",
        ] {
            assert!(
                matches!(
                    require_permitted(bad),
                    Err(EngineError::InvalidTemplate(m)) if m.contains("https")
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn other_schemes_and_garbage_are_rejected() {
        assert!(require_permitted("ftp://example.com/t").is_err());
        assert!(require_permitted("http://").is_err());
    }
}
