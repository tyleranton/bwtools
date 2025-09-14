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
    pub rating_output_enabled: bool,
    pub rating_output_path: PathBuf,
    pub opponent_history_path: PathBuf,
    pub last_replay_path: PathBuf,
    pub screp_cmd: String,
    pub replay_settle: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tick_rate: Duration::from_millis(250),
            scan_window_secs: 10,
            debug_window_secs: 10,
            refresh_interval: Duration::from_millis(1000),
            rating_poll_interval: Duration::from_secs(60),
            cache_dir: default_cache_dir(),
            rating_output_enabled: true,
            rating_output_path: default_rating_output_path(),
            opponent_history_path: default_history_path(),
            last_replay_path: default_last_replay_path(),
            screp_cmd: default_screp_cmd(),
            replay_settle: Duration::from_millis(500),
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

fn default_rating_output_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        let home = env::var("USERPROFILE").unwrap_or_else(|_| String::from("."));
        PathBuf::from(home)
            .join("bwtools")
            .join("overlay")
            .join("self_rating.txt")
    } else {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join("bwtools")
            .join("overlay")
            .join("self_rating.txt")
    }
}

fn default_history_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        let home = env::var("USERPROFILE").unwrap_or_else(|_| String::from("."));
        PathBuf::from(home)
            .join("bwtools")
            .join("history")
            .join("opponents.json")
    } else {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join("bwtools")
            .join("history")
            .join("opponents.json")
    }
}

fn default_last_replay_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        let home = env::var("USERPROFILE").unwrap_or_else(|_| String::from("."));
        PathBuf::from(home)
            .join("Documents")
            .join("StarCraft")
            .join("Maps")
            .join("Replays")
            .join("LastReplay.rep")
    } else {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let user = env::var("USER").unwrap_or_else(|_| "default".to_string());
        home.join(".wine-battlenet/drive_c/users")
            .join(user)
            .join("Documents/StarCraft/Maps/Replays/LastReplay.rep")
    }
}

fn default_screp_cmd() -> String { "screp".to_string() }
