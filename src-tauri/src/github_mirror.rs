//! Shared gh-proxy mirror policy for `github.com` downloads.
//!
//! Mainland-China clients reach `github.com` release assets unreliably, so every
//! download path that targets GitHub tries public gh-proxy instances *first* and
//! falls back to GitHub itself only once all of them have failed. Each candidate
//! is requested as `<mirror>/<original-url>`, the gh-proxy convention.
//!
//! SECURITY: a proxy sees and can rewrite the bytes it serves. Whether that is
//! acceptable depends entirely on the caller:
//!
//! - Callers that verify a pinned SHA-256 afterwards (the runtime-bootstrap
//!   fallback) are safe — altered bytes fail the digest check and the next source
//!   is tried.
//! - Callers with no checksum (the ACP binary cache, `uv tool install` from a
//!   GitHub archive) are trusting these proxies with code that will be executed.
//!   That is accepted deliberately as a stopgap for download reliability; the fix
//!   is to move those artifacts onto the version center's signed-ticket path,
//!   which verifies size + SHA-256. Until then, deployments that cannot accept
//!   third-party proxies should point [`MIRROR_ENV`] at a self-hosted instance,
//!   or set it to `off` to restore direct-only downloads.

/// Public gh-proxy instances tried, in order, ahead of GitHub itself.
///
/// Checked 2026-08-05 against `opencode-windows-x64.zip`: all three served
/// HTTP 206 for a ranged request; the first two additionally returned a full body
/// byte-identical (SHA-256) to GitHub's. Public instances are volunteer-run and
/// do disappear (`ghp.ci` and `mirror.ghproxy.com` were already dead at that
/// check), which is why the list is ordered and fully exhausted before direct.
pub const DEFAULT_GITHUB_MIRRORS: &[&str] = &[
    "https://gh-proxy.com",
    "https://ghfast.top",
    "https://ghproxy.net",
];

/// Replaces [`DEFAULT_GITHUB_MIRRORS`] with a comma-, semicolon- or
/// whitespace-separated list of proxy base URLs, tried in the order given. Set to
/// `off`, `none`, or `direct` to skip mirrors and download straight from GitHub.
///
/// An explicit value with no usable entry also yields no mirrors: a typo'd
/// override must not silently fan out to the public defaults it was meant to
/// replace.
pub const MIRROR_ENV: &str = "IYW_CLAW_GITHUB_MIRROR";

/// URL prefix this policy applies to. Release assets and source archives live
/// under `github.com`; `raw.githubusercontent.com` is deliberately excluded —
/// its only consumers pipe the response straight into a shell, and those already
/// have a first-party mirror.
const GITHUB_PREFIX: &str = "https://github.com/";

/// True when `url` is a GitHub URL this policy will mirror.
pub fn is_mirrorable(url: &str) -> bool {
    url.starts_with(GITHUB_PREFIX)
}

/// Host of `url`, for telemetry. Proxied candidates embed the whole upstream URL
/// and get long, so logs record which source was used rather than the full URL.
/// Returns `"unknown"` when `url` does not parse.
pub fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Proxied forms of `url`, best first, **without** the direct GitHub fallback.
/// Empty when `url` is not a GitHub URL or mirroring is disabled.
pub fn mirror_urls(url: &str) -> Vec<String> {
    build_mirror_urls(url, &configured_mirrors())
}

/// Every URL to try for `url`, in order: proxies first, direct GitHub last.
/// Always non-empty — a total mirror outage still resolves to the original URL.
pub fn download_candidates(url: &str) -> Vec<String> {
    build_download_candidates(url, &configured_mirrors())
}

fn configured_mirrors() -> String {
    std::env::var(MIRROR_ENV).unwrap_or_default()
}

/// Pure core of [`download_candidates`], split out so mirror-list parsing is
/// testable without mutating process-wide environment state.
fn build_download_candidates(url: &str, configured: &str) -> Vec<String> {
    let mut candidates = build_mirror_urls(url, configured);
    candidates.push(url.to_string());
    candidates
}

/// Pure core of [`mirror_urls`].
fn build_mirror_urls(url: &str, configured: &str) -> Vec<String> {
    if !is_mirrorable(url) {
        return Vec::new();
    }
    let mut mirrors = Vec::new();
    for mirror in parse_mirrors(configured) {
        let candidate = format!("{mirror}/{url}");
        if !mirrors.contains(&candidate) {
            mirrors.push(candidate);
        }
    }
    mirrors
}

fn parse_mirrors(configured: &str) -> Vec<String> {
    let configured = configured.trim();
    if configured.is_empty() {
        return DEFAULT_GITHUB_MIRRORS
            .iter()
            .map(ToString::to_string)
            .collect();
    }
    if matches!(
        configured.to_ascii_lowercase().as_str(),
        "off" | "none" | "direct"
    ) {
        return Vec::new();
    }
    configured
        .split([',', ';', ' ', '\t', '\n', '\r'])
        .map(|entry| entry.trim().trim_end_matches('/'))
        .filter(|entry| entry.starts_with("https://") || entry.starts_with("http://"))
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENCODE_URL: &str =
        "https://github.com/anomalyco/opencode/releases/download/v1.17.13/opencode-windows-x64.zip";

    #[test]
    fn defaults_try_every_mirror_then_github() {
        let candidates = build_download_candidates(OPENCODE_URL, "");
        assert_eq!(candidates.len(), DEFAULT_GITHUB_MIRRORS.len() + 1);
        for (candidate, mirror) in candidates.iter().zip(DEFAULT_GITHUB_MIRRORS) {
            assert_eq!(candidate, &format!("{mirror}/{OPENCODE_URL}"));
        }
        // Direct GitHub stays last: a reachable mirror is always preferred, but a
        // total mirror outage must still resolve.
        assert_eq!(candidates.last().unwrap(), OPENCODE_URL);
    }

    #[test]
    fn explicit_override_replaces_defaults_in_order() {
        let candidates =
            build_download_candidates(OPENCODE_URL, "https://a.example, https://b.example/");
        assert_eq!(
            candidates,
            vec![
                format!("https://a.example/{OPENCODE_URL}"),
                format!("https://b.example/{OPENCODE_URL}"),
                OPENCODE_URL.to_string(),
            ]
        );
    }

    #[test]
    fn sentinels_disable_mirrors() {
        for sentinel in ["off", "none", "direct", "DIRECT"] {
            assert_eq!(
                build_download_candidates(OPENCODE_URL, sentinel),
                vec![OPENCODE_URL.to_string()],
                "{sentinel} should skip mirrors"
            );
            assert!(build_mirror_urls(OPENCODE_URL, sentinel).is_empty());
        }
    }

    /// A typo'd override must not silently resurrect the defaults it replaced —
    /// an operator who pinned a self-hosted proxy should get a hard failure, not
    /// a quiet fan-out to public instances.
    #[test]
    fn unusable_override_yields_direct_only() {
        assert_eq!(
            build_download_candidates(OPENCODE_URL, "gh-proxy.com"),
            vec![OPENCODE_URL.to_string()]
        );
    }

    #[test]
    fn duplicate_mirrors_are_collapsed() {
        let candidates = build_download_candidates(
            OPENCODE_URL,
            "https://a.example https://a.example/ https://b.example",
        );
        assert_eq!(
            candidates,
            vec![
                format!("https://a.example/{OPENCODE_URL}"),
                format!("https://b.example/{OPENCODE_URL}"),
                OPENCODE_URL.to_string(),
            ]
        );
    }

    /// Mirroring is scoped to github.com — every other origin, the version
    /// center's own artifact host included, must be fetched exactly as given.
    #[test]
    fn non_github_urls_are_never_mirrored() {
        for url in [
            "https://vol-ai.iywtu.com/artifact.zip",
            "https://registry.npmmirror.com/-/binary/node/v24.18.1/node-v24.18.1-win-x64.zip",
            "https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/install.sh",
        ] {
            assert!(!is_mirrorable(url), "{url} must not be mirrored");
            assert_eq!(build_download_candidates(url, ""), vec![url.to_string()]);
            assert!(build_mirror_urls(url, "").is_empty());
        }
    }

    #[test]
    fn mirror_urls_excludes_direct_github() {
        let mirrors = build_mirror_urls(OPENCODE_URL, "");
        assert_eq!(mirrors.len(), DEFAULT_GITHUB_MIRRORS.len());
        assert!(!mirrors.contains(&OPENCODE_URL.to_string()));
    }
}
