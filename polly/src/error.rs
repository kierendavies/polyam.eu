use std::fmt;
use tracing_error::SpanTrace;

#[derive(thiserror::Error)]
pub struct Error {
    pub source: anyhow::Error,
    pub span_trace: SpanTrace,
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(f)?;
        write!(f, "\n\nSpan trace:\n")?;
        fmt::Display::fmt(&self.span_trace, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(f)
    }
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Error {
            source: error,
            span_trace: SpanTrace::capture(),
        }
    }
}

macro_rules! error_from {
    ($t:ty) => {
        impl From<$t> for Error {
            fn from(error: $t) -> Self {
                Error {
                    source: error.into(),
                    span_trace: SpanTrace::capture(),
                }
            }
        }
    };
}

error_from!(serenity::Error);
error_from!(sqlx::Error);
error_from!(std::io::Error);
error_from!(toml::de::Error);

pub type Result<T> = core::result::Result<T, Error>;

macro_rules! bail {
    ($($args:tt)*) => {
        return Err(anyhow::anyhow!($($args)*).into())
    };
}
pub(crate) use bail;
