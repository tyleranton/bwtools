use std::env;
use std::path::PathBuf;
use std::time::Duration;

pub struct Config {
    pub tick_rate: Duration,
    pub scan_window_secs: i64,
    pub debug_window_secs: i64,
    pub refresh_interval: Duration,
    pub rating_poll_interval: Duration,
    pub cache_dir: PathBuf,
}

impl Config {
    pub fn default() -> Self {
        Self {
            tick_rate: Duration::from_millis(250),
            scan_window_secs: 10,
            debug_window_secs: 10,
            refresh_interval: Duration::from_millis(1000),
            rating_poll_interval: Duration::from_secs(60),
            cache_dir: default_cache_dir(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        let home = env::var("USERPROFILE").unwrap_or_else(|_| String::from("."));
        return PathBuf::from(home)
            .join("AppData")
            .join("Local")
            .join("Temp")
            .join("blizzard_browser_cache");
    }

    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("~"));
    let user = env::var("USER").unwrap_or_else(|_| "default".to_string());
    home.join(".wine-battlenet/drive_c/users")
        .join(user)
        .join("AppData/Local/Temp/blizzard_browser_cache")
}
