// core/src/gpu/logging.rs
use log::{LevelFilter, Record, Metadata, SetLoggerError};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

pub struct SimpleLogger {
    file: Mutex<std::fs::File>,
}

impl SimpleLogger {
    pub fn init(log_file: &str, level: LevelFilter) -> Result<(), SetLoggerError> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(log_file)
            .expect("Failed to open log file");

        let logger = SimpleLogger {
            file: Mutex::new(file),
        };

        log::set_boxed_logger(Box::new(logger))?;
        log::set_max_level(level);
        Ok(())
    }
}

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut file = self.file.lock().unwrap();
            writeln!(
                file,
                "{} - {} - {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            )
            .expect("Failed to write to log file");
        }
    }

    fn flush(&self) {
        let mut file = self.file.lock().unwrap();
        file.flush().expect("Failed to flush log file");
    }
}