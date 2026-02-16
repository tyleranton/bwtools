use std::env;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_USER: &str = "default";

#[derive(Clone)]
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
    pub profile_history_path: PathBuf,
    pub last_replay_path: PathBuf,
    pub screp_cmd: String,
    pub replay_settle: Duration,
    pub opponent_output_enabled: bool,
    pub opponent_output_path: PathBuf,
    pub rating_retry_max: u8,
    pub rating_retry_interval: Duration,
    pub replay_library_root: PathBuf,
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
            profile_history_path: default_profile_history_path(),
            last_replay_path: default_last_replay_path(),
            screp_cmd: default_screp_cmd(),
            replay_settle: Duration::from_millis(500),
            opponent_output_enabled: true,
            opponent_output_path: default_opponent_output_path(),
            rating_retry_max: 3,
            rating_retry_interval: Duration::from_millis(500),
            replay_library_root: default_replay_library_root(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        return windows_user_profile_dir()
            .join("AppData")
            .join("Local")
            .join("Temp")
            .join("blizzard_browser_cache");
    }

    wine_user_root().join("AppData/Local/Temp/blizzard_browser_cache")
}

fn default_rating_output_path() -> PathBuf {
    bundle_root().join("overlay").join("self_rating.txt")
}

fn default_history_path() -> PathBuf {
    bundle_root().join("history").join("opponents.json")
}

fn default_profile_history_path() -> PathBuf {
    bundle_root().join("history").join("profile_history.json")
}

fn default_last_replay_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        windows_replay_dir().join("LastReplay.rep")
    } else {
        wine_replay_dir().join("LastReplay.rep")
    }
}

fn default_screp_cmd() -> String {
    "screp".to_string()
}

fn default_opponent_output_path() -> PathBuf {
    bundle_root().join("overlay").join("opponent_info.txt")
}

fn bundle_root() -> PathBuf {
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        return dir.to_path_buf();
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn default_log_dir() -> PathBuf {
    bundle_root().join("logs")
}

fn default_replay_library_root() -> PathBuf {
    if cfg!(target_os = "windows") {
        windows_replay_dir()
    } else {
        wine_replay_dir()
    }
}

fn windows_user_profile_dir() -> PathBuf {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn windows_replay_dir() -> PathBuf {
    windows_user_profile_dir()
        .join("Documents")
        .join("StarCraft")
        .join("Maps")
        .join("Replays")
}

fn unix_home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn unix_user() -> String {
    env::var("USER").unwrap_or_else(|_| DEFAULT_USER.to_string())
}

fn wine_user_root() -> PathBuf {
    wine_user_root_from(unix_home_dir(), &unix_user())
}

fn wine_user_root_from(home_dir: PathBuf, user: &str) -> PathBuf {
    home_dir.join(".wine-battlenet/drive_c/users").join(user)
}

fn wine_replay_dir() -> PathBuf {
    wine_user_root().join("Documents/StarCraft/Maps/Replays")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wine_user_root_uses_expected_layout() {
        let root = wine_user_root_from(PathBuf::from("/home/tester"), "sc_user");
        assert_eq!(
            root,
            PathBuf::from("/home/tester/.wine-battlenet/drive_c/users/sc_user")
        );
    }

    #[test]
    fn wine_replay_dir_appends_replay_segments() {
        let root = wine_user_root_from(PathBuf::from("/tmp/home"), "user");
        assert_eq!(
            root.join("Documents/StarCraft/Maps/Replays"),
            PathBuf::from(
                "/tmp/home/.wine-battlenet/drive_c/users/user/Documents/StarCraft/Maps/Replays"
            )
        );
    }
}
