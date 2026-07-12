use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug)]
struct ReleaseCache {
    last_check_timestamp: String,
    latest_version: String,
}

fn cache_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("release_cache.json")
}

fn read_cache(db_path: &Path) -> Option<ReleaseCache> {
    let path = cache_path(db_path);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_cache(db_path: &Path, cache: &ReleaseCache) -> anyhow::Result<()> {
    let path = cache_path(db_path);
    let content = serde_json::to_string(cache)?;
    fs::write(path, content)?;
    Ok(())
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch_part = parts.next()?;
    // Strip everything after first non-digit in the patch component (e.g. "0-beta" -> "0")
    let patch_clean: String = patch_part
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let patch = patch_clean.parse().ok()?;
    Some((major, minor, patch))
}

pub struct UpdateChecker {
    thread_handle: Option<std::thread::JoinHandle<Option<String>>>,
    cached_latest: Option<String>,
}

impl UpdateChecker {
    pub fn new(db_path: &Path) -> Self {
        Self::new_with_env(db_path, false)
    }

    fn new_with_env(db_path: &Path, ignore_env: bool) -> Self {
        if !ignore_env
            && (std::env::var("ENGRAMS_NO_UPDATE_CHECK").is_ok()
                || std::env::var("NO_UPDATE_CHECK").is_ok()
                || std::env::var("CI").is_ok())
        {
            return Self {
                thread_handle: None,
                cached_latest: None,
            };
        }

        let mut need_check = true;
        let mut cached_latest = None;

        if let Some(cache) = read_cache(db_path) {
            cached_latest = Some(cache.latest_version.clone());
            if let Ok(last_check) = DateTime::parse_from_rfc3339(&cache.last_check_timestamp) {
                let last_check_utc = last_check.with_timezone(&Utc);
                let age = Utc::now() - last_check_utc;
                if age < chrono::Duration::hours(24) {
                    need_check = false;
                }
            }
        }

        let thread_handle = if need_check {
            let db_path_clone = db_path.to_path_buf();
            Some(std::thread::spawn(move || {
                fetch_latest_version(&db_path_clone)
            }))
        } else {
            None
        };

        Self {
            thread_handle,
            cached_latest,
        }
    }

    pub fn print_notification(self) {
        let latest_version = if let Some(handle) = self.thread_handle {
            match handle.join() {
                Ok(Some(v)) => Some(v),
                _ => self.cached_latest, // fallback to cache if thread failed/panicked
            }
        } else {
            self.cached_latest
        };

        if let Some(latest) = latest_version {
            let current_version_str = env!("CARGO_PKG_VERSION");
            if let (Some(current), Some(lat)) =
                (parse_version(current_version_str), parse_version(&latest))
            {
                if lat > current {
                    use std::io::IsTerminal;
                    let use_color =
                        std::io::stderr().is_terminal() && std::env::var("NO_COLOR").is_err();
                    if use_color {
                        eprintln!(
                            "\n\x1b[1;33mNotification:\x1b[0m A new version of engrams-cli is available: \x1b[32m{}\x1b[0m (current: {})",
                            latest, current_version_str
                        );
                    } else {
                        eprintln!(
                            "\nA new version of engrams-cli is available: {} (current: {})",
                            latest, current_version_str
                        );
                    }
                }
            }
        }
    }
}

fn fetch_latest_version(db_path: &Path) -> Option<String> {
    let response =
        ureq::get("https://api.github.com/repos/stevebrownlee/engrams-cli/releases/latest")
            .set("User-Agent", "engrams-cli")
            .set("Accept", "application/vnd.github.v3+json")
            .timeout(Duration::from_secs(2))
            .call()
            .ok()?;

    #[derive(Deserialize)]
    struct GitHubRelease {
        tag_name: String,
    }

    let release: GitHubRelease = response.into_json().ok()?;
    let fetched_version = release.tag_name;

    // Update cache
    let cache = ReleaseCache {
        last_check_timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        latest_version: fetched_version.clone(),
    };
    let _ = write_cache(db_path, &cache);

    Some(fetched_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("V1.12.34-beta.1"), Some((1, 12, 34)));
        assert_eq!(parse_version("invalid"), None);
    }

    #[test]
    fn test_cache_read_write() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("e.db");

        // Cache shouldn't exist initially
        assert!(read_cache(&db_path).is_none());

        let cache = ReleaseCache {
            last_check_timestamp: "2026-07-11T12:00:00Z".to_string(),
            latest_version: "1.0.0".to_string(),
        };

        write_cache(&db_path, &cache).unwrap();

        let loaded = read_cache(&db_path).unwrap();
        assert_eq!(loaded.last_check_timestamp, "2026-07-11T12:00:00Z");
        assert_eq!(loaded.latest_version, "1.0.0");
    }

    #[test]
    fn test_fresh_cache_skips_fetch() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("e.db");

        // Write cache from 1 hour ago
        let fresh_time = (Utc::now() - chrono::Duration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let cache = ReleaseCache {
            last_check_timestamp: fresh_time,
            latest_version: "1.0.0".to_string(),
        };
        write_cache(&db_path, &cache).unwrap();

        // Even without ENGRAMS_NO_UPDATE_CHECK, a fresh cache should skip fetch
        let checker = UpdateChecker::new_with_env(&db_path, true);
        assert!(checker.thread_handle.is_none());
        assert_eq!(checker.cached_latest, Some("1.0.0".to_string()));
    }
}
