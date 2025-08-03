use crate::log_query::LogQuery;
use logform::{Format, LogInfo};
use std::sync::Arc;

pub trait Transport: Send + Sync {
    fn log(&self, info: LogInfo);
    fn log_batch(&self, logs: Vec<LogInfo>) {
        for log_info in logs {
            self.log(log_info);
        }
    }
    fn flush(&self) -> Result<(), String> {
        Ok(())
    }
    fn get_level(&self) -> Option<&String> {
        None
    }
    fn get_format(&self) -> Option<Arc<dyn Format<Input = LogInfo> + Send + Sync>> {
        None
    }
    fn query(&self, _options: &LogQuery) -> Result<Vec<LogInfo>, String> {
        Ok(Vec::new())
    }
}
