use tracing::{info, info_span, Span};
use tracing_subscriber::field::MakeExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

use uuid::Uuid;
use yansi::Paint;

use anyhow::Result;
use url::Url;

pub const LOKI_ADDRESS: &str = "http://127.0.0.1:3100";

pub enum LogType {
    Production,
    Formatted,
    Simple,
    Json,
}

impl From<String> for LogType {
    fn from(input: String) -> Self {
        match input.as_str() {
            "prod" | "production" => Self::Production,
            "formatted" => Self::Formatted,
            "simple" => Self::Simple,
            "json" => Self::Json,
            _ => panic!("Unkown log type {}", input),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum LogLevel {
    /// Only shows errors and warnings
    Error,
    /// Shows errors, warnings, and some informational messages that are likely
    /// to be relevant when troubleshooting such as configuration
    Warn,
    /// Shows everything except debug and trace information
    Info,
    /// Shows debug information
    Debug,
    /// Shows everything
    Trace,
    /// Shows nothing
    Off,
}

impl From<&str> for LogLevel {
    fn from(s: &str) -> Self {
        return match &*s.to_ascii_lowercase() {
            "critical" => LogLevel::Error,
            "support" => LogLevel::Warn,
            "normal" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            "off" => LogLevel::Off,
            _ => panic!("unrecognized log level: {}", s),
        };
    }
}

pub fn filter_layer(level: LogLevel) -> EnvFilter {
    let filter_str = match level {
        LogLevel::Error => "warn,hyper=off,rustls=off,tungstenite=off",
        LogLevel::Warn => "warn,rocket::support=info,hyper=off,rustls=off,tungstenite=off",
        LogLevel::Info => "info,hyper=off,rustls=off,tungstenite=off",
        LogLevel::Debug => "debug,hyper=info,rustls=off,sled=info,tungstenite=info",
        LogLevel::Trace => {
            "trace,hyper=info,rustls=off,sled=info,\
            tungstenite=info,message_io=debug,mio=debug,quinn_proto=off,\
            tokio_util=off"
        }
        LogLevel::Off => "off",
    };

    tracing_subscriber::filter::EnvFilter::try_new(filter_str).expect("filter string must parse")
}

#[derive(Clone)]
pub struct TracingSpan<T = Span>(T);

pub struct TracingFairing;

pub fn simple_logging_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber,
    S: for<'span> LookupSpan<'span>,
{
    let field_format = tracing_subscriber::fmt::format::debug_fn(|writer, field, value| {
        // We'll format the field name and value separated with a colon.
        if field.name() == "message" {
            write!(writer, "{:?}", Paint::new(value).bold())
        } else {
            write!(writer, "{}: {:?}", field, Paint::default(value).bold())
        }
    })
    .delimited(", ")
    .display_messages();

    tracing_subscriber::fmt::layer()
        .compact()
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .without_time()
        // .fmt_fields(field_format)
        // Configure the formatter to use `print!` rather than
        // `stdout().write_str(...)`, so that logs are captured by libtest's test
        // capturing.
        .with_test_writer()
}

pub fn default_logging_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber,
    S: for<'span> LookupSpan<'span>,
{
    let field_format = tracing_subscriber::fmt::format::debug_fn(|writer, field, value| {
        // We'll format the field name and value separated with a colon.
        if field.name() == "message" {
            write!(writer, "{:?}", Paint::new(value).bold())
        } else {
            write!(writer, "{}: {:?}", field, Paint::default(value).bold())
        }
    })
    .delimited(", ")
    .display_messages();

    tracing_subscriber::fmt::layer()
        .fmt_fields(field_format)
        // Configure the formatter to use `print!` rather than
        // `stdout().write_str(...)`, so that logs are captured by libtest's test
        // capturing.
        .with_test_writer()
}

pub fn json_logging_layer<
    S: for<'a> tracing_subscriber::registry::LookupSpan<'a> + tracing::Subscriber,
>() -> impl tracing_subscriber::Layer<S> {
    Paint::disable();

    tracing_subscriber::fmt::layer()
        .json()
        // Configure the formatter to use `print!` rather than
        // `stdout().write_str(...)`, so that logs are captured by libtest's test
        // capturing.
        .with_test_writer()
}

pub fn init(hostname: String, log_level: Option<LogLevel>) -> Result<()> {
    use tracing_log::LogTracer;
    use tracing_subscriber::prelude::*;

    LogTracer::init().expect("Unable to setup log tracer!");

    let log_type =
        LogType::from(std::env::var("LOG_TYPE").unwrap_or_else(|_| "formatted".to_string()));
    let log_level = if let Ok(level) = std::env::var("LOG_LEVEL") {
        LogLevel::from(level.as_str())
    } else if let Some(level) = log_level {
        level
    } else {
        LogLevel::from("normal")
    };

    match log_type {
        LogType::Production => {
            // loki layer
            use tracing_loki::url::Url;
            let (loki_layer, task) = tracing_loki::layer(
                Url::parse(LOKI_ADDRESS).unwrap(),
                vec![("host".into(), hostname)].into_iter().collect(),
                vec![].into_iter().collect(),
            )
            .expect("tracing_loki failed making new layer");
            // The background task needs to be spawned so the logs actually get
            // delivered to loki.
            tokio::spawn(task);

            tracing::subscriber::set_global_default(
                tracing_subscriber::registry()
                    // .with(default_logging_layer())
                    .with(loki_layer)
                    .with(json_logging_layer())
                    .with(filter_layer(log_level)),
            )
        }
        LogType::Formatted => tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(default_logging_layer())
                .with(filter_layer(log_level)),
        ),
        LogType::Simple => tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(simple_logging_layer())
                .with(filter_layer(log_level)),
        ),
        LogType::Json => tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(json_logging_layer())
                .with(filter_layer(log_level)),
        ),
    };

    Ok(())
}
