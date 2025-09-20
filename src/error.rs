use std::borrow::Cow;
use std::io;

use anyhow::Error as AnyhowError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("terminal setup failed")]
    TerminalSetup(#[source] io::Error),
    #[error("terminal restore failed")]
    TerminalRestore(#[source] io::Error),
    #[error("terminal rendering failed")]
    TerminalRender(#[source] io::Error),
    #[error("runtime error: {context}")]
    Runtime {
        context: Cow<'static, str>,
        #[source]
        source: AnyhowError,
    },
}

impl AppError {
    pub fn runtime<S, E>(context: S, source: E) -> Self
    where
        S: Into<Cow<'static, str>>,
        E: Into<AnyhowError>,
    {
        Self::Runtime {
            context: context.into(),
            source: source.into(),
        }
    }
}

impl From<AnyhowError> for AppError {
    fn from(source: AnyhowError) -> Self {
        AppError::Runtime {
            context: Cow::Borrowed("unexpected runtime error"),
            source,
        }
    }
}
