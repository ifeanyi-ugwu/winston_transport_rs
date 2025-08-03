//! This module contains adapter implementations for converting between
//! `Transport` and `Write` traits, both for owned and borrowed scenarios.
//!
//! The adapters allow seamless interoperability:
//! - `TransportWriter` - use any Transport as a Writer
//! - `WriterTransport` - use any Writer as a Transport
//! - Both owned and borrowed variants are provided
//!
//! Extension traits provide convenient `.into_writer()`, `.as_writer()`,
//! `.into_transport()`, and `.as_transport()` methods.

use crate::Transport;
use logform::{Format, LogInfo};
use std::{
    cell::RefCell,
    io::{self, Write},
    sync::{Arc, Mutex},
};

/// owned adapter: takes ownership of a Transport and uses it as a Writer.
/// Here, we assume `Transport<LogInfo>`.
pub struct TransportWriter<T>
where
    T: Transport<LogInfo>,
{
    transport: T,
    buffer: Vec<u8>,
    level: String,
}

impl<T> TransportWriter<T>
where
    T: Transport<LogInfo>,
{
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            buffer: Vec::new(),
            level: "INFO".to_string(),
        }
    }

    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = level.into();
        self
    }
}

impl<T> Write for TransportWriter<T>
where
    T: Transport<LogInfo>,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        // Process full lines
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes = self.buffer.drain(..=pos).collect::<Vec<u8>>();
            let line = String::from_utf8_lossy(&line_bytes)
                .trim_end_matches(&['\r', '\n'][..])
                .to_string();

            let info = LogInfo::new(&self.level, line);
            self.transport.log(info);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Flush any buffered partial line before calling underlying flush
        if !self.buffer.is_empty() {
            let leftover = String::from_utf8_lossy(&self.buffer).to_string();
            self.buffer.clear();
            self.transport.log(LogInfo::new(&self.level, leftover));
        }

        self.transport
            .flush()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

impl<T> Drop for TransportWriter<T>
where
    T: Transport<LogInfo>,
{
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// borrowed adapter: borrows a Transport and uses it as a Writer.
pub struct TransportWriterRef<'a, T>
where
    T: Transport<LogInfo> + ?Sized,
{
    transport: &'a T,
    buffer: RefCell<Vec<u8>>,
    level: String,
}

impl<'a, T> TransportWriterRef<'a, T>
where
    T: Transport<LogInfo> + ?Sized,
{
    pub fn new(transport: &'a T) -> Self {
        Self {
            transport,
            buffer: RefCell::new(Vec::new()),
            level: "INFO".to_string(),
        }
    }

    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = level.into();
        self
    }

    // Helper to flush internal buffer emitting logs for each full line
    fn flush_buffered_lines(&self) {
        let mut buf = self.buffer.borrow_mut();
        let mut start = 0;

        while let Some(pos) = buf[start..].iter().position(|&b| b == b'\n') {
            let end = start + pos;
            // Extract the line + '\n'
            let line_bytes = buf.drain(..=end).collect::<Vec<_>>();
            let line_str = String::from_utf8_lossy(&line_bytes)
                .trim_end_matches(&['\r', '\n'][..])
                .to_string();
            self.transport.log(LogInfo::new(&self.level, line_str));
            start = 0; // dropped above
        }
    }
}

impl<'a, T> Write for TransportWriterRef<'a, T>
where
    T: Transport<LogInfo> + ?Sized,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Append to internal buffer
        {
            let mut internal_buffer = self.buffer.borrow_mut();
            internal_buffer.extend_from_slice(buf);
        }

        // Process any full lines in the buffer
        self.flush_buffered_lines();

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Flush any remaining partial line
        let leftover = self.buffer.borrow_mut().drain(..).collect::<Vec<u8>>();
        if !leftover.is_empty() {
            let leftover_str = String::from_utf8_lossy(&leftover).to_string();
            self.transport.log(LogInfo::new(&self.level, leftover_str));
        }

        self.transport
            .flush()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

impl<'a, T> Drop for TransportWriterRef<'a, T>
where
    T: Transport<LogInfo> + ?Sized,
{
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// owned adapter to use a Writer as a Transport.
pub struct WriterTransport<W: Write + Send + Sync> {
    pub writer: Mutex<W>,
    level: Option<String>,
    format: Option<Arc<dyn Format<Input = LogInfo> + Send + Sync>>,
}

impl<W: Write + Send + Sync> WriterTransport<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
            level: None,
            format: None,
        }
    }

    pub fn with_level(mut self, level: String) -> Self {
        self.level = Some(level);
        self
    }

    pub fn with_format<F>(mut self, format: F) -> Self
    where
        F: Format<Input = LogInfo> + Send + Sync + 'static,
    {
        self.format = Some(Arc::new(format));
        self
    }
}

impl<W: Write + Send + Sync> Transport<LogInfo> for WriterTransport<W> {
    fn log(&self, info: LogInfo) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writeln!(writer, "{}", info.message);
        }
    }

    fn log_batch(&self, infos: Vec<LogInfo>) {
        if infos.is_empty() {
            return;
        }
        if let Ok(mut writer) = self.writer.lock() {
            for info in infos {
                if let Err(e) = writeln!(writer, "{}", info.message) {
                    eprintln!(
                        "Failed to write log entry in batch to WriterTransport: {}",
                        e
                    );
                }
            }
        } else {
            eprintln!("Failed to acquire writer lock for WriterTransport batch logging");
        }
    }

    fn get_level(&self) -> Option<&String> {
        self.level.as_ref()
    }

    fn get_format(&self) -> Option<Arc<dyn Format<Input = LogInfo> + Send + Sync>> {
        self.format.clone()
    }

    fn flush(&self) -> Result<(), String> {
        self.writer
            .lock()
            .map_err(|_| "Failed to lock writer".to_string())
            .and_then(|mut writer| {
                writer
                    .flush()
                    .map_err(|e| format!("Failed to flush: {}", e))
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

/// borrowed adapter for using a Writer as a Transport.
pub struct WriterTransportRef<'a, W: Write + Send + Sync> {
    writer: &'a Mutex<W>,
    level: Option<String>,
    format: Option<Arc<dyn Format<Input = LogInfo> + Send + Sync>>,
}

impl<'a, W: Write + Send + Sync> WriterTransportRef<'a, W> {
    pub fn new(writer: &'a Mutex<W>) -> Self {
        Self {
            writer,
            level: None,
            format: None,
        }
    }

    pub fn with_level(mut self, level: String) -> Self {
        self.level = Some(level);
        self
    }

    pub fn with_format<F>(mut self, format: F) -> Self
    where
        F: Format<Input = LogInfo> + Send + Sync + 'static,
    {
        self.format = Some(Arc::new(format));
        self
    }
}

impl<'a, W: Write + Send + Sync> Transport<LogInfo> for WriterTransportRef<'a, W> {
    fn log(&self, info: LogInfo) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writeln!(writer, "{}", info.message);
        }
    }

    fn log_batch(&self, infos: Vec<LogInfo>) {
        if infos.is_empty() {
            return;
        }

        if let Ok(mut writer) = self.writer.lock() {
            for info in infos {
                if let Err(e) = writeln!(writer, "{}", info.message) {
                    eprintln!(
                        "Failed to write log entry in batch to WriterTransportRef: {}",
                        e
                    );
                }
            }
        } else {
            eprintln!("Failed to acquire writer lock for WriterTransportRef batch logging");
        }
    }

    fn get_level(&self) -> Option<&String> {
        self.level.as_ref()
    }

    fn get_format(&self) -> Option<Arc<dyn Format<Input = LogInfo> + Send + Sync>> {
        self.format.clone()
    }

    fn flush(&self) -> Result<(), String> {
        self.writer
            .lock()
            .map_err(|_| "Failed to lock writer".to_string())
            .and_then(|mut writer| {
                writer
                    .flush()
                    .map_err(|e| format!("Failed to flush: {}", e))
            })
    }
}

impl<'a, W: Write + Send + Sync> Drop for WriterTransportRef<'a, W> {
    fn drop(&mut self) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.flush();
        }
    }
}

/// extension trait for converting an owned transport to a writer.
pub trait IntoTransportWriter {
    fn into_writer(self) -> TransportWriter<Self>
    where
        Self: Transport<LogInfo> + Sized,
    {
        TransportWriter::new(self)
    }
}

/// extension trait for converting a borrowed transport to a writer.
pub trait AsTransportWriter {
    fn as_writer(&self) -> TransportWriterRef<'_, Self>
    where
        Self: Transport<LogInfo> + Sized;
}

impl<T> IntoTransportWriter for T where T: Transport<LogInfo> {}

impl<T> AsTransportWriter for T
where
    T: Transport<LogInfo>,
{
    fn as_writer(&self) -> TransportWriterRef<'_, Self> {
        TransportWriterRef::new(self)
    }
}

/// trait to convert an owned writer into a transport.
pub trait IntoWriterTransport: Write + Send + Sync + Sized {
    fn into_transport(self) -> WriterTransport<Self> {
        WriterTransport::new(self)
    }
}

impl<W: Write + Send + Sync> IntoWriterTransport for W {}

/// extension trait for converting a borrowed writer to a transport.
pub trait AsWriterTransport {
    type Writer: Write + Send + Sync;
    fn as_transport(&self) -> WriterTransportRef<'_, Self::Writer>;
}

impl<W: Write + Send + Sync> AsWriterTransport for Mutex<W> {
    type Writer = W;

    fn as_transport(&self) -> WriterTransportRef<'_, W> {
        WriterTransportRef::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logform::LogInfo;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct MockTransport {
        messages: Arc<Mutex<Vec<String>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }
    }

    impl Transport<LogInfo> for MockTransport {
        fn log(&self, info: LogInfo) {
            self.messages.lock().unwrap().push(info.message);
        }
    }

    struct TestBuffer {
        content: Vec<u8>,
    }

    impl TestBuffer {
        fn new() -> Self {
            Self {
                content: Vec::new(),
            }
        }

        fn contents_as_string(&self) -> String {
            String::from_utf8_lossy(&self.content).to_string()
        }
    }

    impl Write for TestBuffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.content.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_owned_transport_to_writer() {
        let transport = MockTransport::new();
        let transport_clone = transport.clone();
        let mut writer = transport.into_writer();

        writeln!(writer, "Test message 1").unwrap();
        writeln!(writer, "Test message 2").unwrap();

        let messages = transport_clone.get_messages();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("Test message 1"));
        assert!(messages[1].contains("Test message 2"));
    }

    #[test]
    fn test_borrowed_transport_to_writer() {
        let transport = MockTransport::new();
        let mut writer = transport.as_writer();

        writeln!(writer, "Borrowed message 1").unwrap();
        writeln!(writer, "Borrowed message 2").unwrap();

        let messages = transport.get_messages();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("Borrowed message 1"));
        assert!(messages[1].contains("Borrowed message 2"));
    }

    #[test]
    fn test_owned_writer_to_transport() {
        let buffer = TestBuffer::new();
        let transport = buffer.into_transport();

        transport.log(LogInfo::new("INFO", "Test log 1"));
        transport.log(LogInfo::new("INFO", "Test log 2"));

        let writer_guard = transport.writer.lock().unwrap();
        let content = writer_guard.contents_as_string();
        assert!(content.contains("Test log 1"));
        assert!(content.contains("Test log 2"));
    }

    #[test]
    fn test_borrowed_writer_to_transport() {
        let test_buffer = Mutex::new(TestBuffer::new());
        let transport_ref = test_buffer.as_transport();

        transport_ref.log(LogInfo::new("INFO", "Borrowed log 1"));
        transport_ref.log(LogInfo::new("INFO", "Borrowed log 2"));
        transport_ref.flush().unwrap();

        let buffer_guard = test_buffer.lock().unwrap();
        let contents = buffer_guard.contents_as_string();
        assert!(contents.contains("Borrowed log 1"));
        assert!(contents.contains("Borrowed log 2"));
    }
}
