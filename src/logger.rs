use log::{Log, LogLevel, LogMetadata, LogRecord};

pub struct Logger<F: Fn(LogLevel, &str) + Send + Sync> {
    callback: F,
}

impl<F: Fn(LogLevel, &str) + Send + Sync> Logger<F> {
    pub fn new(callback: F) -> Logger<F> { Logger { callback: callback } }
}

impl<F: Fn(LogLevel, &str) + Send + Sync> Log for Logger<F> {
    fn enabled(&self, _metadata: &LogMetadata) -> bool { true }

    fn log(&self, record: &LogRecord) {
        if self.enabled(record.metadata()) {
            (self.callback)(record.level(), &format!("{}", record.args()));
        }
    }
}
