use crate::Transport;
use logform::{Format, LogInfo};
use std::{any::Any, io::Write, sync::Mutex};

/// A generic transport for writing logs to any destination implementing the `Write` trait
///
/// Supports logging to files, network sockets, buffers, stdout, and custom writers.
///
/// # Examples
/// ```
/// let file = File::create("app.log")?;
/// let transport = WriterTransport::new(file)
///     .with_level("INFO")
///     .with_format(custom_format);
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

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl<W: Write + Send + Sync> Drop for WriterTransport<W> {
    fn drop(&mut self) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.flush();
        }
    }
}
