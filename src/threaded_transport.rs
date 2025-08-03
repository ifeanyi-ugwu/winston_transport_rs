use crate::{log_query::LogQuery, Transport};
use logform::Format;
use std::{
    marker::PhantomData,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread::{self, JoinHandle},
};

/// Message types for communicating with the background thread for ThreadedTransport
#[derive(Debug)]
enum TransportMessage<L> {
    Log(L),
    Flush(Sender<Result<(), String>>),
    Query(LogQuery, Sender<Result<Vec<L>, String>>),
    Shutdown,
}

/// A transport wrapper that executes all operations on a separate background thread
/// for non-blocking, asynchronous logging operations.
pub struct ThreadedTransport<T, L>
where
    T: Transport<L> + 'static,
    L: Send + 'static,
{
    sender: Sender<TransportMessage<L>>,
    thread_handle: Option<JoinHandle<()>>,
    level: Option<String>,
    format: Option<Arc<dyn Format<Input = L> + Send + Sync>>,
    _phantom_data: PhantomData<(T, L)>,
}

impl<T, L> ThreadedTransport<T, L>
where
    T: Transport<L> + 'static,
    L: Send + 'static,
{
    /// Creates a new ThreadedTransport that wraps the given transport
    pub fn new(transport: T) -> Self {
        let level = transport.get_level().cloned();
        let format = transport.get_format();

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
        let format = transport.get_format();

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

    fn run_transport_thread(transport: T, receiver: Receiver<TransportMessage<L>>) {
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
                    let _ = transport.flush();
                    break;
                }
            }
        }
    }

    /// Gracefully shuts down the background thread
    pub fn shutdown(mut self) -> Result<(), String> {
        if let Some(handle) = self.thread_handle.take() {
            self.sender
                .send(TransportMessage::Shutdown)
                .map_err(|_| "Failed to send shutdown signal")?;

            handle
                .join()
                .map_err(|_| "Failed to join background thread")?;
        }
        Ok(())
    }
}

impl<T, L> Transport<L> for ThreadedTransport<T, L>
where
    T: Transport<L> + 'static,
    L: Send + Sync + 'static,
{
    fn log(&self, info: L) {
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

    fn get_format(&self) -> Option<Arc<dyn Format<Input = L> + Send + Sync>> {
        self.format.clone()
    }

    fn query(&self, options: &LogQuery) -> Result<Vec<L>, String> {
        let (response_sender, response_receiver) = mpsc::channel();

        self.sender
            .send(TransportMessage::Query(options.clone(), response_sender))
            .map_err(|_| "Failed to send query message to background thread")?;

        response_receiver
            .recv()
            .map_err(|_| "Failed to receive query response from background thread")?
    }
}

impl<T, L> Drop for ThreadedTransport<T, L>
where
    T: Transport<L> + 'static,
    L: Send + 'static,
{
    fn drop(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = self.sender.send(TransportMessage::Shutdown);
            let _ = handle.join();
        }
    }
}

/// Extension trait for easily wrapping any transport with threaded behavior
pub trait IntoThreadedTransport<L>: Transport<L> + Sized + 'static
where
    L: Send + 'static,
{
    /// Wraps this transport in a ThreadedTransport for non-blocking ops
    fn into_threaded(self) -> ThreadedTransport<Self, L> {
        ThreadedTransport::new(self)
    }

    /// Wraps with a custom thread name
    fn into_threaded_named(self, thread_name: String) -> ThreadedTransport<Self, L> {
        ThreadedTransport::with_thread_name(self, thread_name)
    }
}

impl<T, L> IntoThreadedTransport<L> for T
where
    T: Transport<L> + Sized + 'static,
    L: Send + 'static,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use logform::LogInfo;
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

    impl Transport<LogInfo> for MockTransport {
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

        threaded_transport.log(LogInfo::new("INFO", "Message 1"));
        threaded_transport.log(LogInfo::new("INFO", "Message 2"));
        threaded_transport.log(LogInfo::new("INFO", "Message 3"));

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

        threaded_transport.log(LogInfo::new("INFO", "Slow message 1"));
        threaded_transport.log(LogInfo::new("INFO", "Slow message 2"));

        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_millis(50));

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

        threaded_transport.shutdown().unwrap();

        let messages = mock_clone.get_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], "Before shutdown");
    }
}
