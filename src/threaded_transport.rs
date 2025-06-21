use crate::{log_query::LogQuery, Transport};
use logform::{Format, LogInfo};
use std::{
    marker::PhantomData,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

/// Message types for communicating with the background thread
#[derive(Debug)]
enum TransportMessage {
    Log(LogInfo),
    Flush(Sender<Result<(), String>>),
    Query(LogQuery, Sender<Result<Vec<LogInfo>, String>>),
    Shutdown,
}

/// A transport wrapper that executes all operations on a separate background thread
/// for non-blocking, asynchronous logging operations.
pub struct ThreadedTransport<T: Transport + 'static> {
    sender: Sender<TransportMessage>,
    thread_handle: Option<JoinHandle<()>>,
    // Store references to the wrapped transport's level and format for immediate access
    level: Option<String>,
    format: Option<Format>,
    _phantom_data: PhantomData<T>,
}

impl<T: Transport + 'static> ThreadedTransport<T> {
    /// Creates a new ThreadedTransport that wraps the given transport
    pub fn new(transport: T) -> Self {
        // Capture level and format before moving transport to thread
        let level = transport.get_level().cloned();
        let format = transport.get_format().cloned();

        let (sender, receiver) = mpsc::channel();

        let thread_handle = thread::spawn(move || {
            Self::run_transport_thread(transport, receiver);
        });

        Self {
            sender,
            thread_handle: Some(thread_handle),
            level,
            format,
            _phantom_data: PhantomData,
        }
    }

    /// Creates a new ThreadedTransport with a custom thread name
    pub fn with_thread_name(transport: T, thread_name: String) -> Self {
        let level = transport.get_level().cloned();
        let format = transport.get_format().cloned();

        let (sender, receiver) = mpsc::channel();

        let thread_handle = thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                Self::run_transport_thread(transport, receiver);
            })
            .expect("Failed to spawn async transport thread");

        Self {
            sender,
            thread_handle: Some(thread_handle),
            level,
            format,
            _phantom_data: PhantomData,
        }
    }

    /// The main loop running on the background thread
    fn run_transport_thread(transport: T, receiver: Receiver<TransportMessage>) {
        while let Ok(message) = receiver.recv() {
            match message {
                TransportMessage::Log(info) => {
                    transport.log(info);
                }
                TransportMessage::Flush(response_sender) => {
                    let result = transport.flush();
                    let _ = response_sender.send(result);
                }
                TransportMessage::Query(query, response_sender) => {
                    let result = transport.query(&query);
                    let _ = response_sender.send(result);
                }
                TransportMessage::Shutdown => {
                    // Perform final flush before shutting down
                    let _ = transport.flush();
                    break;
                }
            }
        }
    }

    /// Gracefully shuts down the background thread
    pub fn shutdown(mut self) -> Result<(), String> {
        if let Some(handle) = self.thread_handle.take() {
            // Send shutdown signal
            self.sender
                .send(TransportMessage::Shutdown)
                .map_err(|_| "Failed to send shutdown signal")?;

            // Wait for thread to finish
            handle
                .join()
                .map_err(|_| "Failed to join background thread")?;
        }
        Ok(())
    }
}

impl<T: Transport + 'static> Transport for ThreadedTransport<T> {
    fn log(&self, info: LogInfo) {
        // Non-blocking send - if the channel is full or closed, we drop the message
        // We could also use a bounded channel with blocking send if we prefer backpressure
        let _ = self.sender.send(TransportMessage::Log(info));
    }

    fn flush(&self) -> Result<(), String> {
        let (response_sender, response_receiver) = mpsc::channel();

        self.sender
            .send(TransportMessage::Flush(response_sender))
            .map_err(|_| "Failed to send flush message to background thread")?;

        response_receiver
            .recv()
            .map_err(|_| "Failed to receive flush response from background thread")?
    }

    fn get_level(&self) -> Option<&String> {
        self.level.as_ref()
    }

    fn get_format(&self) -> Option<&Format> {
        self.format.as_ref()
    }

    fn query(&self, options: &LogQuery) -> Result<Vec<LogInfo>, String> {
        let (response_sender, response_receiver) = mpsc::channel();

        self.sender
            .send(TransportMessage::Query(options.clone(), response_sender))
            .map_err(|_| "Failed to send query message to background thread")?;

        response_receiver
            .recv()
            .map_err(|_| "Failed to receive query response from background thread")?
    }
}

impl<T: Transport + 'static> Drop for ThreadedTransport<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            // Try to send shutdown signal
            let _ = self.sender.send(TransportMessage::Shutdown);

            // Give the thread a moment to shut down gracefully
            let _ = handle.join();
        }
    }
}

/// Extension trait for easily wrapping any transport with threaded behavior
pub trait IntoThreadedTransport: Transport + Sized + 'static {
    /// Wraps this transport in an ThreadedTransport for non-blocking operations
    fn into_threaded(self) -> ThreadedTransport<Self> {
        ThreadedTransport::new(self)
    }

    /// Wraps this transport in an ThreadedTransport with a custom thread name
    fn into_threaded_named(self, thread_name: String) -> ThreadedTransport<Self> {
        ThreadedTransport::with_thread_name(self, thread_name)
    }
}

// Implement for all transports
impl<T: Transport + 'static> IntoThreadedTransport for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    #[derive(Clone)]
    struct MockTransport {
        messages: Arc<Mutex<Vec<String>>>,
        delay: Duration,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
                delay: Duration::from_millis(0),
            }
        }

        fn with_delay(delay: Duration) -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
                delay,
            }
        }

        fn get_messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }
    }

    impl Transport for MockTransport {
        fn log(&self, info: LogInfo) {
            if self.delay > Duration::from_millis(0) {
                thread::sleep(self.delay);
            }
            self.messages.lock().unwrap().push(info.message);
        }

        fn flush(&self) -> Result<(), String> {
            if self.delay > Duration::from_millis(0) {
                thread::sleep(self.delay);
            }
            Ok(())
        }
    }

    #[test]
    fn test_threaded_transport_basic_logging() {
        let mock = MockTransport::new();
        let mock_clone = mock.clone();
        let threaded_transport = mock.into_threaded();

        // Log some messages
        threaded_transport.log(LogInfo::new("INFO", "Message 1"));
        threaded_transport.log(LogInfo::new("INFO", "Message 2"));
        threaded_transport.log(LogInfo::new("INFO", "Message 3"));

        // Flush to ensure all messages are processed
        threaded_transport.flush().unwrap();

        let messages = mock_clone.get_messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], "Message 1");
        assert_eq!(messages[1], "Message 2");
        assert_eq!(messages[2], "Message 3");
    }

    #[test]
    fn test_threaded_transport_non_blocking() {
        let slow_transport = MockTransport::with_delay(Duration::from_millis(100));
        let slow_transport_clone = slow_transport.clone();
        let threaded_transport = slow_transport.into_threaded();

        let start = std::time::Instant::now();

        // These should return immediately even though the underlying transport is slow
        threaded_transport.log(LogInfo::new("INFO", "Slow message 1"));
        threaded_transport.log(LogInfo::new("INFO", "Slow message 2"));

        let elapsed = start.elapsed();

        // The log calls should complete quickly (much less than 200ms for 2 slow operations)
        assert!(elapsed < Duration::from_millis(50));

        // But we can still flush and verify the messages were processed
        threaded_transport.flush().unwrap();

        let messages = slow_transport_clone.get_messages();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_threaded_transport_graceful_shutdown() {
        let mock = MockTransport::new();
        let mock_clone = mock.clone();
        let threaded_transport = mock.into_threaded_named("test-logger".to_string());

        threaded_transport.log(LogInfo::new("INFO", "Before shutdown"));

        // Graceful shutdown
        threaded_transport.shutdown().unwrap();

        // Verify message was processed
        let messages = mock_clone.get_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], "Before shutdown");
    }
}
