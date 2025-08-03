use crate::log_query::LogQuery;
use logform::Format;
use std::sync::Arc;

pub trait Transport<L>: Send + Sync {
    fn log(&self, info: L);

    fn log_batch(&self, logs: Vec<L>) {
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

    fn get_format(&self) -> Option<Arc<dyn Format<Input = L> + Send + Sync>> {
        None
    }

    fn query(&self, _options: &LogQuery) -> Result<Vec<L>, String> {
        Ok(Vec::new())
    }
}
