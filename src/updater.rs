use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use std::time::{Duration, SystemTime};

use crate::theme::{Preferences, save_preferences};
use crate::ui::Toasts;

pub const REPO: &str = "starzonmyarmz/roxel";
pub const RATE_LIMIT_SECS: u64 = 24 * 60 * 60;
const HTTP_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: (u32, u32, u32),
    pub html_url: String,
}

pub fn parse_tag(tag: &str) -> Option<(u32, u32, u32)> {
    let s = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = s.split('.');
    let a: u32 = parts.next()?.parse().ok()?;
    let b: u32 = parts.next()?.parse().ok()?;
    let c: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((a, b, c))
}

pub fn current_version() -> (u32, u32, u32) {
    parse_tag(env!("CARGO_PKG_VERSION")).unwrap_or((0, 0, 0))
}

pub fn is_newer(current: (u32, u32, u32), remote: (u32, u32, u32)) -> bool {
    remote > current
}

pub fn should_check(last: Option<SystemTime>, now: SystemTime, interval: Duration) -> bool {
    match last {
        None => true,
        Some(t) => match now.duration_since(t) {
            Ok(d) => d >= interval,
            Err(_) => true,
        },
    }
}

#[derive(serde::Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
}

pub fn parse_release_json(body: &str) -> Option<Release> {
    let raw: ReleaseJson = serde_json::from_str(body).ok()?;
    let version = parse_tag(&raw.tag_name)?;
    Some(Release {
        tag: raw.tag_name,
        version,
        html_url: raw.html_url,
    })
}

#[derive(Debug, Clone)]
pub enum CheckOutcome {
    Newer(Release),
    UpToDate,
}

fn fetch_latest() -> Result<CheckOutcome, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let ua = format!("roxel/{}", env!("CARGO_PKG_VERSION"));
    let resp = ureq::get(&url)
        .set("User-Agent", &ua)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .call()
        .map_err(|e| format!("{e}"))?;
    let body = resp.into_string().map_err(|e| format!("{e}"))?;
    let rel = parse_release_json(&body).ok_or_else(|| "could not parse release".to_string())?;
    if is_newer(current_version(), rel.version) {
        Ok(CheckOutcome::Newer(rel))
    } else {
        Ok(CheckOutcome::UpToDate)
    }
}

#[derive(Default)]
#[allow(dead_code)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking {
        task: Task<Result<CheckOutcome, String>>,
        manual: bool,
    },
    Available(Release),
    UpToDate,
    Error(String),
}

#[derive(Resource, Default)]
pub struct UpdateCheck(pub UpdateState);

impl UpdateCheck {
    pub fn is_checking(&self) -> bool {
        matches!(self.0, UpdateState::Checking { .. })
    }
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn available(&self) -> Option<&Release> {
        if let UpdateState::Available(r) = &self.0 {
            Some(r)
        } else {
            None
        }
    }
}

pub fn start_check(state: &mut UpdateCheck, manual: bool) {
    if state.is_checking() {
        return;
    }
    let task = AsyncComputeTaskPool::get().spawn(async move { fetch_latest() });
    state.0 = UpdateState::Checking { task, manual };
}

pub fn poll_update_check_system(
    mut state: ResMut<UpdateCheck>,
    mut toasts: ResMut<Toasts>,
    mut prefs: ResMut<Preferences>,
) {
    let (result, manual) = match &mut state.0 {
        UpdateState::Checking { task, manual } => {
            let Some(r) = block_on(future::poll_once(task)) else {
                return;
            };
            (r, *manual)
        }
        _ => return,
    };
    prefs.last_update_check = Some(SystemTime::now());
    save_preferences(&prefs);
    state.0 = match result {
        Ok(CheckOutcome::Newer(rel)) => {
            toasts.info(format!("Roxel {} available", rel.tag));
            UpdateState::Available(rel)
        }
        Ok(CheckOutcome::UpToDate) => {
            if manual {
                toasts.success("You're on the latest version");
            }
            UpdateState::UpToDate
        }
        Err(e) => {
            if manual {
                toasts.error(format!("Update check failed: {e}"));
            }
            UpdateState::Error(e)
        }
    };
}

pub fn startup_check_system(
    mut state: ResMut<UpdateCheck>,
    prefs: Res<Preferences>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    *done = true;
    if prefs.auto_update_check
        && should_check(
            prefs.last_update_check,
            SystemTime::now(),
            Duration::from_secs(RATE_LIMIT_SECS),
        )
    {
        start_check(&mut state, false);
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn open_url(url: &str) {
    let _ = open::that_detached(url);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_strips_v_prefix() {
        assert_eq!(parse_tag("v0.5.1"), Some((0, 5, 1)));
        assert_eq!(parse_tag("0.5.1"), Some((0, 5, 1)));
        assert_eq!(parse_tag("v10.20.30"), Some((10, 20, 30)));
    }

    #[test]
    fn parse_tag_rejects_malformed() {
        assert_eq!(parse_tag("v0.5"), None);
        assert_eq!(parse_tag("v0.5.1.2"), None);
        assert_eq!(parse_tag("garbage"), None);
        assert_eq!(parse_tag(""), None);
        assert_eq!(parse_tag("v0.5.x"), None);
    }

    #[test]
    fn is_newer_compares_lexicographically_per_component() {
        assert!(is_newer((0, 5, 0), (0, 5, 1)));
        assert!(!is_newer((0, 5, 0), (0, 5, 0)));
        assert!(!is_newer((0, 5, 1), (0, 5, 0)));
        assert!(is_newer((0, 5, 9), (0, 6, 0)));
        assert!(is_newer((0, 99, 99), (1, 0, 0)));
        assert!(!is_newer((1, 0, 0), (0, 99, 99)));
    }

    #[test]
    fn parse_release_json_handles_whitespace_and_escapes() {
        let body = r#"{ "tag_name" : "v0.5.1" , "html_url": "a\/b\"c" }"#;
        let r = parse_release_json(body).expect("parse");
        assert_eq!(r.tag, "v0.5.1");
        assert_eq!(r.html_url, "a/b\"c");
    }

    #[test]
    fn parse_release_json_ignores_extra_fields() {
        // GitHub returns dozens of fields; serde must ignore the ones we don't ask for.
        let body = r#"{"tag_name":"v0.5.1","html_url":"u","author":{"login":"x"},"assets":[]}"#;
        let r = parse_release_json(body).expect("parse");
        assert_eq!(r.tag, "v0.5.1");
    }

    #[test]
    fn parse_release_json_extracts_release() {
        let body = r#"{"tag_name":"v0.5.1","name":"Roxel 0.5.1","html_url":"https://github.com/starzonmyarmz/roxel/releases/tag/v0.5.1","body":"notes"}"#;
        let r = parse_release_json(body).expect("parse");
        assert_eq!(r.tag, "v0.5.1");
        assert_eq!(r.version, (0, 5, 1));
        assert_eq!(
            r.html_url,
            "https://github.com/starzonmyarmz/roxel/releases/tag/v0.5.1"
        );
    }

    #[test]
    fn parse_release_json_returns_none_for_garbage() {
        assert!(parse_release_json("not json").is_none());
        assert!(parse_release_json(r#"{"foo":"bar"}"#).is_none());
        assert!(parse_release_json(r#"{"tag_name":"weird","html_url":"u"}"#).is_none());
    }

    #[test]
    fn should_check_returns_true_when_never_checked() {
        assert!(should_check(
            None,
            SystemTime::now(),
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn should_check_respects_rate_limit() {
        let now = SystemTime::now();
        let interval = Duration::from_secs(60 * 60);
        let recent = now - Duration::from_secs(60);
        let stale = now - Duration::from_secs(60 * 60 * 2);
        assert!(!should_check(Some(recent), now, interval));
        assert!(should_check(Some(stale), now, interval));
    }

    #[test]
    fn should_check_returns_true_on_clock_skew() {
        // last_check is in the future (clock moved backward) — treat as stale.
        let now = SystemTime::now();
        let future = now + Duration::from_secs(60);
        assert!(should_check(
            Some(future),
            now,
            Duration::from_secs(60 * 60)
        ));
    }

    #[test]
    fn current_version_parses_cargo_pkg_version() {
        let v = current_version();
        // Sanity: at least one component non-zero (cargo guarantees the env var).
        assert!(v != (0, 0, 0));
    }
}
