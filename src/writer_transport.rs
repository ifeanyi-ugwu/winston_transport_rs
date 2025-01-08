use crate::Transport;
use logform::{Format, LogInfo};
use std::{io::Write, sync::Mutex};

/// A generic transport for writing logs to any destination implementing the `Write` trait
///
/// Supports logging to files, network sockets, buffers, stdout, and custom writers.
///
/// # Examples
/// ```
/// let stdout = std::io::stdout();
/// let transport = winston_transport::WriterTransport::new(stdout)
///     .with_level("INFO")
///     .with_format(logform::json());
///
#[derive(Debug)]
pub struct WriterTransport<W: Write + Send + Sync> {
    writer: Mutex<W>,
    level: Option<String>,
    format: Option<Format>,
}

impl<W: Write + Send + Sync> WriterTransport<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
            level: None,
            format: None,
        }
    }

    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = Some(level.into());
        self
    }

    pub fn with_format(mut self, format: Format) -> Self {
        self.format = Some(format);
        self
    }
}

impl<W: Write + Send + Sync + 'static> Transport for WriterTransport<W> {
    fn log(&self, info: LogInfo) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writeln!(writer, "{}", info.message);
        }
    }

    fn get_level(&self) -> Option<&String> {
        self.level.as_ref()
    }

    fn get_format(&self) -> Option<&Format> {
        self.format.as_ref()
    }

    fn flush(&self) -> Result<(), String> {
        self.writer
            .lock()
            .map_err(|_| "Failed to lock writer".to_string())
            .and_then(|mut writer| {
                writer
                    .flush()
                    .map_err(|e| format!("Failed to flush file: {}", e))
            })
    }
}

impl<W: Write + Send + Sync> Drop for WriterTransport<W> {
    fn drop(&mut self) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.flush();
        }
    }
}
