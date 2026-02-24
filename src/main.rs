mod api;
mod app;
mod cache;
mod config;
mod detect;
mod error;
mod gateway;
mod history;
mod interaction;
mod overlay;
mod player_list;
mod profile;
mod profile_history;
mod race;
mod replay;
mod replay_download;
mod replay_io;
mod runtime;
mod tui;
mod ui;

use std::sync::OnceLock;

use error::AppError;
use runtime::AppRuntime;

static TRACING_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();
static TRACING_INIT: OnceLock<()> = OnceLock::new();

fn init_logging() {
    TRACING_INIT.get_or_init(|| {
        let log_dir = crate::config::default_log_dir();
        if let Err(err) = std::fs::create_dir_all(&log_dir) {
            eprintln!(
                "failed to create log directory {}: {err}",
                log_dir.display()
            );
            return;
        }

        let file_appender = tracing_appender::rolling::daily(log_dir, "bwtools.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let _ = TRACING_GUARD.set(guard);

        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(non_blocking)
            .with_ansi(false)
            .init();
    });
}

fn main() -> Result<(), AppError> {
    init_logging();
    crate::tui::install_panic_hook();

    let cfg: crate::config::Config = Default::default();
    let mut runtime = AppRuntime::new(cfg)?;
    let result = runtime.run();
    if let Err(err) = runtime.shutdown() {
        tracing::error!(error = %err, "failed to restore terminal");
    }
    result
}
