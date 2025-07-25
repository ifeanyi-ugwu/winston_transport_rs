use crate::{log_query::LogQuery, Transport};
use logform::{Format, LogInfo};
use std::{
    marker::PhantomData,
    sync::{mpsc::Sender, Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Configuration for batch behavior
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of logs to batch before flushing
    pub max_batch_size: usize,
    /// Maximum time to wait before flushing a partial batch
    pub max_batch_time: Duration,
    /// Whether to flush immediately on Drop
    pub flush_on_drop: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_batch_time: Duration::from_millis(500),
            flush_on_drop: true,
        }
    }
}

/// Internal message types for the batch thread
#[derive(Debug)]
enum BatchMessage {
    Log(LogInfo),
    Flush(Sender<Result<(), String>>),
    Query(
        LogQuery,
        std::sync::mpsc::Sender<Result<Vec<LogInfo>, String>>,
    ),
    Shutdown,
}

/// A transport wrapper that batches log messages before sending them to the underlying transport
pub struct BatchedTransport<T: Transport + Send + 'static> {
    sender: std::sync::mpsc::Sender<BatchMessage>,
    thread_handle: Option<JoinHandle<()>>,
    level: Option<String>,
    format: Option<Format>,
    config: BatchConfig,
    _phantom: PhantomData<T>,
}

impl<T: Transport + Send + 'static> BatchedTransport<T> {
    /// Creates a new BatchedTransport with default configuration
    pub fn new(transport: T) -> Self {
        Self::with_config(transport, BatchConfig::default())
    }

    /// Creates a new BatchedTransport with custom configuration
    pub fn with_config(transport: T, config: BatchConfig) -> Self {
        let level = transport.get_level().cloned();
        let format = transport.get_format().cloned();

        let (sender, receiver) = std::sync::mpsc::channel();
        let batch_config = config.clone();

        let thread_handle = thread::spawn(move || {
            Self::run_batch_thread(transport, receiver, batch_config);
        });

        Self {
            sender,
            thread_handle: Some(thread_handle),
            level,
            format,
            config,
            _phantom: PhantomData,
        }
    }

    /// Creates a BatchedTransport with a custom thread name
    pub fn with_thread_name(transport: T, config: BatchConfig, thread_name: String) -> Self {
        let level = transport.get_level().cloned();
        let format = transport.get_format().cloned();

        let (sender, receiver) = std::sync::mpsc::channel();
        let batch_config = config.clone();

        let thread_handle = thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                Self::run_batch_thread(transport, receiver, batch_config);
            })
            .expect("Failed to spawn batch transport thread");

        Self {
            sender,
            thread_handle: Some(thread_handle),
            level,
            format,
            config,
            _phantom: PhantomData,
        }
    }

    /// The main batching loop running on the background thread
    fn run_batch_thread(
        transport: T,
        receiver: std::sync::mpsc::Receiver<BatchMessage>,
        config: BatchConfig,
    ) {
        let mut batch = Vec::new();
        let mut last_flush = Instant::now();

        // Helper function to flush the current batch
        let flush_batch = |batch: &mut Vec<LogInfo>| {
            if !batch.is_empty() {
                // Log each item in the batch
                /*for log_info in batch.drain(..) {
                    transport.log(log_info);
                }*/
                // Drain the batch and pass the collected Vec to log_batch
                transport.log_batch(batch.drain(..).collect());
                // Flush the underlying transport
                let _ = transport.flush();
            }
        };

        loop {
            // Calculate timeout for receiving messages
            let time_since_last_flush = last_flush.elapsed();
            let timeout = if batch.is_empty() {
                // If no logs pending, wait indefinitely
                None
            } else if time_since_last_flush >= config.max_batch_time {
                // Time to flush, no wait
                Some(Duration::from_millis(0))
            } else {
                // Wait for remaining time
                Some(config.max_batch_time - time_since_last_flush)
            };

            // Try to receive a message with timeout
            let message_result = if let Some(timeout) = timeout {
                receiver.recv_timeout(timeout)
            } else {
                receiver
                    .recv()
                    .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
            };

            match message_result {
                Ok(BatchMessage::Log(info)) => {
                    batch.push(info);

                    // Check if we should flush due to batch size
                    if batch.len() >= config.max_batch_size {
                        flush_batch(&mut batch);
                        last_flush = Instant::now();
                    }
                }
                Ok(BatchMessage::Flush(response_sender)) => {
                    flush_batch(&mut batch);
                    last_flush = Instant::now();
                    let _ = response_sender.send(Ok(()));
                }
                Ok(BatchMessage::Query(query, response_sender)) => {
                    // For queries, we need to flush pending logs first
                    flush_batch(&mut batch);
                    last_flush = Instant::now();

                    let result = transport.query(&query);
                    let _ = response_sender.send(result);
                }
                Ok(BatchMessage::Shutdown) => {
                    // Flush any remaining logs before shutting down
                    flush_batch(&mut batch);
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout occurred, flush if we have logs and enough time has passed
                    if !batch.is_empty() && last_flush.elapsed() >= config.max_batch_time {
                        flush_batch(&mut batch);
                        last_flush = Instant::now();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Channel disconnected, flush and exit
                    flush_batch(&mut batch);
                    break;
                }
            }
        }
    }

    /// Gracefully shuts down the batching thread
    pub fn shutdown(mut self) -> Result<(), String> {
        if let Some(handle) = self.thread_handle.take() {
            self.sender
                .send(BatchMessage::Shutdown)
                .map_err(|_| "Failed to send shutdown signal")?;

            handle.join().map_err(|_| "Failed to join batch thread")?;
        }
        Ok(())
    }

    /// Gets the current batch configuration
    pub fn config(&self) -> &BatchConfig {
        &self.config
    }
}

impl<T: Transport + Send + 'static> Transport for BatchedTransport<T> {
    fn log(&self, info: LogInfo) {
        // Non-blocking send - logs are queued for batching
        let _ = self.sender.send(BatchMessage::Log(info));
    }

    fn flush(&self) -> Result<(), String> {
        let (response_sender, response_receiver) = std::sync::mpsc::channel();

        self.sender
            .send(BatchMessage::Flush(response_sender))
            .map_err(|_| "Failed to send flush message to batch thread")?;

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
        let (response_sender, response_receiver) = std::sync::mpsc::channel();

        self.sender
            .send(BatchMessage::Query(options.clone(), response_sender))
            .map_err(|_| "Failed to send query message to batch thread")?;

        response_receiver
            .recv()
            .map_err(|_| "Failed to receive query response from batch thread")?
    }
}

impl<T: Transport + Send + 'static> Drop for BatchedTransport<T> {
    fn drop(&mut self) {
        if self.config.flush_on_drop {
            if let Some(handle) = self.thread_handle.take() {
                let _ = self.sender.send(BatchMessage::Shutdown);
                let _ = handle.join();
            }
        }
    }
}

/// Extension trait for easily wrapping any transport with batching behavior
pub trait IntoBatchedTransport: Transport + Send + Sized + 'static {
    /// Wraps this transport in a BatchedTransport with default configuration
    fn into_batched(self) -> BatchedTransport<Self> {
        BatchedTransport::new(self)
    }

    /// Wraps this transport in a BatchedTransport with custom configuration
    fn into_batched_with_config(self, config: BatchConfig) -> BatchedTransport<Self> {
        BatchedTransport::with_config(self, config)
    }

    /// Wraps this transport in a BatchedTransport with a custom thread name
    fn into_batched_named(
        self,
        config: BatchConfig,
        thread_name: String,
    ) -> BatchedTransport<Self> {
        BatchedTransport::with_thread_name(self, config, thread_name)
    }
}

// Implement for all transports
impl<T: Transport + Send + 'static> IntoBatchedTransport for T {}

/// Builder for creating BatchConfig
pub struct BatchConfigBuilder {
    max_batch_size: usize,
    max_batch_time: Duration,
    flush_on_drop: bool,
}

impl BatchConfigBuilder {
    pub fn new() -> Self {
        let default = BatchConfig::default();
        Self {
            max_batch_size: default.max_batch_size,
            max_batch_time: default.max_batch_time,
            flush_on_drop: default.flush_on_drop,
        }
    }

    pub fn max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    pub fn max_batch_time(mut self, duration: Duration) -> Self {
        self.max_batch_time = duration;
        self
    }

    pub fn flush_on_drop(mut self, flush: bool) -> Self {
        self.flush_on_drop = flush;
        self
    }

    pub fn build(self) -> BatchConfig {
        BatchConfig {
            max_batch_size: self.max_batch_size,
            max_batch_time: self.max_batch_time,
            flush_on_drop: self.flush_on_drop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[derive(Clone)]
    struct MockTransport {
        messages: Arc<Mutex<Vec<String>>>,
        log_calls: Arc<Mutex<Vec<Instant>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
                log_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }

        fn get_log_call_count(&self) -> usize {
            self.log_calls.lock().unwrap().len()
        }
    }

    impl Transport for MockTransport {
        fn log(&self, info: LogInfo) {
            self.messages.lock().unwrap().push(info.message);
            self.log_calls.lock().unwrap().push(Instant::now());
        }

        fn flush(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_batch_size_trigger() {
        let mock = MockTransport::new();
        let mock_clone = mock.clone();

        let config = BatchConfigBuilder::new()
            .max_batch_size(3)
            .max_batch_time(Duration::from_secs(10)) // Long time so size triggers first
            .build();

        let batched = mock.into_batched_with_config(config);

        // Send 3 messages - should trigger a batch flush
        batched.log(LogInfo::new("INFO", "Message 1"));
        batched.log(LogInfo::new("INFO", "Message 2"));
        batched.log(LogInfo::new("INFO", "Message 3"));

        // Give the batch thread time to process
        std::thread::sleep(Duration::from_millis(100));

        let messages = mock_clone.get_messages();
        assert_eq!(messages.len(), 3);

        // All should be logged as a batch (3 separate log calls)
        assert_eq!(mock_clone.get_log_call_count(), 3);
    }

    #[test]
    fn test_time_trigger() {
        let mock = MockTransport::new();
        let mock_clone = mock.clone();

        let config = BatchConfigBuilder::new()
            .max_batch_size(100) // Large size so time triggers first
            .max_batch_time(Duration::from_millis(50))
            .build();

        let batched = mock.into_batched_with_config(config);

        // Send 2 messages
        batched.log(LogInfo::new("INFO", "Message 1"));
        batched.log(LogInfo::new("INFO", "Message 2"));

        // Wait for time trigger
        std::thread::sleep(Duration::from_millis(100));

        let messages = mock_clone.get_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "Message 1");
        assert_eq!(messages[1], "Message 2");
    }

    #[test]
    fn test_manual_flush() {
        let mock = MockTransport::new();
        let mock_clone = mock.clone();

        let config = BatchConfigBuilder::new()
            .max_batch_size(100)
            .max_batch_time(Duration::from_secs(10))
            .build();

        let batched = mock.into_batched_with_config(config);

        batched.log(LogInfo::new("INFO", "Message 1"));
        batched.flush().unwrap();

        // Give time for flush to process
        std::thread::sleep(Duration::from_millis(50));

        let messages = mock_clone.get_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], "Message 1");
    }
}
