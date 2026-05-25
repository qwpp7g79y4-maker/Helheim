use tracing_appender::rolling;
use tracing_subscriber::{fmt, prelude::*, registry::Registry};

pub fn init_telemetry() -> tracing_appender::non_blocking::WorkerGuard {
    // 1. Audit File Logger (JSON)
    let file_appender = rolling::daily("helheim-logs", "audit.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer().json().with_writer(non_blocking);

    // 2. Console Logger (User Friendly)
    // We only log warnings/errors to console to keep the UI clean
    let console_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_filter(tracing_subscriber::filter::LevelFilter::WARN);

    // 3. Registry
    let subscriber = Registry::default().with(file_layer).with(console_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("[FATAL]: Telemetry initialisatie mislukt.");

    tracing::info!(
        system = "helheim-reactor",
        version = "0.2.0",
        message = "Flight Recorder gestart."
    );

    guard
}
