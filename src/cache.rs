use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use chrome_cache_parser::ChromeCache;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use url::Url;
use percent_encoding::percent_decode_str;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn extract_port(url: &str) -> Option<u16> {
    Url::parse(url).ok().and_then(|parsed| parsed.port())
}

fn decode_segment(seg: &str) -> String {
    percent_decode_str(seg).decode_utf8_lossy().into_owned()
}

fn parse_profile_from_url_mmgameloading(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    let has_flag = parsed
        .query_pairs()
        .any(|(k, v)| k == "request_flags" && v.as_ref().contains("scr_mmgameloading"));
    if !has_flag {
        return None;
    }

    let mut segments = parsed.path_segments()?;
    let s1 = segments.next()?;
    let s2 = segments.next()?;
    let s3 = segments.next()?;
    if s1 != "web-api" || s2 != "v2" || s3 != "aurora-profile-by-toon" {
        return None;
    }
    let profile = decode_segment(segments.next()?);
    let gateway_str = segments.next()?;
    let gateway: u16 = gateway_str.parse().ok()?;
    Some((profile, gateway))
}

fn parse_profile_from_url_path(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    let mut segments = parsed.path_segments()?;
    let s1 = segments.next()?;
    let s2 = segments.next()?;
    let s3 = segments.next()?;
    if s1 != "web-api" || s2 != "v2" || s3 != "aurora-profile-by-toon" {
        return None;
    }
    let profile = decode_segment(segments.next()?);
    let gw: u16 = segments.next()?.parse().ok()?;
    Some((profile, gw))
}

pub struct CacheReader {
    cache_dir: PathBuf,
    cache: ChromeCache,
}

impl CacheReader {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        let cache = ChromeCache::from_path(cache_dir.clone()).context("Failed to open Chrome cache")?;
        Ok(Self { cache_dir, cache })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.cache = ChromeCache::from_path(self.cache_dir.clone()).context("Failed to refresh Chrome cache")?;
        Ok(())
    }

    pub fn parse_for_port(&self, window_secs: i64) -> Result<Option<u16>> {
        let now = Utc::now();
        let work = || -> Result<Option<u16>> {
            let entries = self.cache.entries().context("Failed to read cache entries")?;
            let latest = entries
                .filter_map(|e| {
                    let entry = e.get().ok()?;
                    let key = entry.key.to_string();
                    if !key.contains("/web-api/") { return None; }
                    let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                    if (now - creation_time) >= ChronoDuration::seconds(window_secs) { return None; }
                    let port = extract_port(&key)?;
                    Some((port, creation_time))
                })
                .max_by_key(|(_, ct)| *ct)
                .map(|(p, _)| p);
            Ok(latest)
        };
        match catch_unwind(AssertUnwindSafe(work)) {
            Ok(res) => res,
            Err(_) => Err(anyhow!("Cache scan panicked (transient cache rotation)")),
        }
    }

    pub fn latest_opponent_profile(&self, exclude_name: Option<&str>, window_secs: i64) -> Result<Option<(String, u16)>> {
        let now = Utc::now();
        let work = || -> Result<Option<(String, u16)>> {
            let entries = self.cache.entries().context("Failed to read cache entries")?;
            let latest = entries
                .filter_map(|e| {
                    let entry = e.get().ok()?;
                    let key = entry.key.to_string();
                    if !(key.contains("/web-api/v2/aurora-profile-by-toon/") && key.contains("scr_mmgameloading")) { return None; }
                    let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                    if (now - creation_time) >= ChronoDuration::seconds(window_secs) { return None; }
                    let (profile, gateway) = parse_profile_from_url_mmgameloading(&key)?;
                    if let Some(ex) = exclude_name { if profile.eq_ignore_ascii_case(ex) { return None; } }
                    Some(((profile, gateway), creation_time))
                })
                .max_by_key(|(_, ct)| *ct)
                .map(|(data, _)| data);
            Ok(latest)
        };
        match catch_unwind(AssertUnwindSafe(work)) { Ok(res) => res, Err(_) => Err(anyhow!("Cache scan panicked (transient cache rotation)")) }
    }

    pub fn latest_mmgameloading_profile(&self, window_secs: i64) -> Result<Option<(String, u16)>> {
        let now = Utc::now();
        let work = || -> Result<Option<(String, u16)>> {
            let entries = self.cache.entries().context("Failed to read cache entries")?;
            let latest = entries
                .filter_map(|e| {
                    let entry = e.get().ok()?;
                    let key = entry.key.to_string();
                    if !(key.contains("/web-api/v2/aurora-profile-by-toon/") && key.contains("scr_mmgameloading")) { return None; }
                    let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                    if (now - creation_time) >= ChronoDuration::seconds(window_secs) { return None; }
                    let (profile, gateway) = parse_profile_from_url_mmgameloading(&key)?;
                    Some(((profile, gateway), creation_time))
                })
                .max_by_key(|(_, ct)| *ct)
                .map(|(data, _)| data);
            Ok(latest)
        };
        match catch_unwind(AssertUnwindSafe(work)) { Ok(res) => res, Err(_) => Err(anyhow!("Cache scan panicked (transient cache rotation)")) }
    }

    pub fn latest_self_profile(&self, window_secs: i64) -> Result<Option<(String, u16)>> {
        let now = Utc::now();
        let work = || -> Result<Option<(String, u16)>> {
            let entries = self.cache.entries().context("Failed to read cache entries")?;
            let latest = entries
                .filter_map(|e| {
                    let entry = e.get().ok()?;
                    let key = entry.key.to_string();
                    if !(key.contains("/web-api/v2/aurora-profile-by-toon/") && key.contains("scr_tooninfo")) { return None; }
                    let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                    if (now - creation_time) >= ChronoDuration::seconds(window_secs) { return None; }
                    let (profile, gw) = parse_profile_from_url_path(&key)?;
                    Some(((profile, gw), creation_time))
                })
                .max_by_key(|(_, creation_time)| *creation_time)
                .map(|(data, _)| data);
            Ok(latest)
        };
        match catch_unwind(AssertUnwindSafe(work)) { Ok(res) => res, Err(_) => Err(anyhow!("Cache scan panicked (transient cache rotation)")) }
    }

    pub fn recent_keys(&self, window_secs: i64, max: usize) -> Result<Vec<(String, i64)>> {
        let now = Utc::now();
        let work = || -> Result<Vec<(String, i64)>> {
            let entries = self.cache.entries().context("Failed to read cache entries")?;
            let mut items: Vec<(String, i64, DateTime<Utc>)> = entries
                .filter_map(|e| {
                    let entry = e.get().ok()?;
                    let key = entry.key.to_string();
                    if !key.contains("/web-api/") { return None; }
                    let creation_time: DateTime<Utc> = entry.creation_time.into_datetime_utc().ok()?;
                    let age = (now - creation_time).num_seconds();
                    if age <= window_secs { Some((key, age, creation_time)) } else { None }
                })
                .collect();
            items.sort_by_key(|(_, _, ct)| *ct);
            items.reverse();
            Ok(items.into_iter().take(max).map(|(k, age, _)| (k, age)).collect())
        };
        match catch_unwind(AssertUnwindSafe(work)) { Ok(res) => res, Err(_) => Err(anyhow!("Cache scan panicked (transient cache rotation)")) }
    }
}
