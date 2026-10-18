//! "Check for Updates": asks GitHub for the latest release, only when the
//! user chooses the menu item (never in the background).

use std::time::Duration;

use crate::{REPO_API_LATEST, VERSION};

pub enum Outcome {
    /// A newer release exists; holds its version (without the leading `v`).
    Newer(String),
    UpToDate,
    Failed(String),
}

/// Blocking network call; run it off the main thread.
pub fn check() -> Outcome {
    let url = std::env::var("SAM_CALC_RELEASES_API").unwrap_or_else(|_| REPO_API_LATEST.into());
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("sam-calc/", env!("CARGO_PKG_VERSION")))
        .build();
    let body = match agent.get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(resp) => match resp.into_string() {
            Ok(b) => b,
            Err(e) => return Outcome::Failed(e.to_string()),
        },
        Err(ureq::Error::Status(404, _)) => return Outcome::Failed("No releases found".into()),
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let Some(tag) = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("tag_name")?.as_str().map(str::to_string))
    else {
        return Outcome::Failed("Unexpected response from GitHub".into());
    };
    let latest = tag.trim_start_matches('v').to_string();
    if is_newer(&latest, VERSION) {
        Outcome::Newer(latest)
    } else {
        Outcome::UpToDate
    }
}

fn parse(v: &str) -> Option<Vec<u64>> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    core.split('.').map(|p| p.parse().ok()).collect()
}

/// True if `latest` is a higher version than `current`; unparsable
/// versions are never reported as newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(mut a), Some(mut b)) => {
            let n = a.len().max(b.len());
            a.resize(n, 0);
            b.resize(n, 0);
            a > b
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_comparison() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.1.0", "1.0.9"));
        assert!(is_newer("2.0", "1.9.9"));
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
        assert!(!is_newer("1.0.1-beta", "1.0.1"));
    }
}
