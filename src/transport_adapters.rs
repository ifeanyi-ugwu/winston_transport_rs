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
    io::{self, Write},
    sync::{Arc, Mutex},
};

/// owned adapter: takes ownership of a Transport and uses it as a Writer.
pub struct TransportWriter<T: Transport> {
    transport: T,
}

impl<T: Transport> TransportWriter<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<T: Transport> Write for TransportWriter<T> {
    /*fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let info = LogInfo::from_bytes(buf)?;
        self.transport.log(info);
        Ok(buf.len())
    }*/

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let message = String::from_utf8_lossy(buf).to_string();
        self.transport.log(LogInfo::new("INFO", message));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.transport
            .flush()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

impl<T: Transport> Drop for TransportWriter<T> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// borrowed adapter: borrows a Transport and uses it as a Writer.
pub struct TransportWriterRef<'a, T: Transport> {
    transport: &'a T,
}

impl<'a, T: Transport> TransportWriterRef<'a, T> {
    pub fn new(transport: &'a T) -> Self {
        Self { transport }
    }
}

impl<'a, T: Transport> Write for TransportWriterRef<'a, T> {
    /*fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let info = LogInfo::from_bytes(buf)?;
        self.transport.log(info);
        Ok(buf.len())
    }*/
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let message = String::from_utf8_lossy(buf).to_string();
        self.transport.log(LogInfo::new("INFO", message));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.transport
            .flush()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

impl<'a, T: Transport> Drop for TransportWriterRef<'a, T> {
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

impl<W: Write + Send + Sync> Transport for WriterTransport<W> {
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
                    // Continue on error for resilience
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

impl<'a, W: Write + Send + Sync> Transport for WriterTransportRef<'a, W> {
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
                    // Continue on error for resilience
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
        Self: Transport + Sized,
    {
        TransportWriter::new(self)
    }
}

/// extension trait for converting a borrowed transport to a writer.
pub trait AsTransportWriter {
    fn as_writer(&self) -> TransportWriterRef<'_, Self>
    where
        Self: Transport + Sized;
}

impl<T: Transport> IntoTransportWriter for T {}

impl<T: Transport> AsTransportWriter for T {
    fn as_writer(&self) -> TransportWriterRef<'_, Self> {
        TransportWriterRef::new(self)
    }
}

/// trait to convert an owned writer into a transport.
pub trait IntoWriterTransport {
    fn into_transport(self) -> WriterTransport<Self>
    where
        Self: Write + Send + Sync + Sized,
    {
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
    use std::sync::Arc;

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

    impl Transport for MockTransport {
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
