use dirs;
use fern;
use log::{Level, LevelFilter};
use std::io;

/// Initialize logging with the fern logger
pub fn log<F: Fn(Level, &str) + Send + Sync + 'static>(callback: F) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        // Include logs for crates that we use.
        .level(LevelFilter::Debug)
        // Exclude logs for crates which we don't care to log.
        .level_for("serde_xml_rs", LevelFilter::Off)
        // This will be used by the front end for display logs in a UI
        .chain(fern::Output::call(move |record| {
            callback(record.level(), &format!("{}", record.args()))
        }))
        // Whereas this will handle displaying the logs to the terminal & a log file
        .chain({
            let mut logger = fern::Dispatch::new()
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "[{} distinst{}] {}",
                        record.level(),
                        match (record.file(), record.line()) {
                            (Some(file), Some(line)) => format!(":{}:{}", file, line),
                            _ => "".into(),
                        },
                        message
                    ))
                })
                .chain(io::stderr());

            match fern::log_file("/tmp/installer.log") {
                Ok(log) => logger = logger.chain(log),
                Err(why) => {
                    eprintln!("failed to create log file at /tmp/installer.log: {}", why);
                }
            };

            // If the home directory exists, add a log there as well.
            // If the Desktop directory exists within the home directory, write the logs there.
            if let Some(home) = dirs::home_dir() {
                let desktop = home.join("Desktop");
                let log = if desktop.is_dir() {
                    fern::log_file(&desktop.join("installer.log"))
                } else {
                    fern::log_file(&home.join("installer.log"))
                };

                match log {
                    Ok(log) => logger = logger.chain(log),
                    Err(why) => {
                        eprintln!("failed to set up logging for the home directory: {}", why);
                    }
                }
            }

            logger
        })
        .apply()?;

    Ok(())
}
