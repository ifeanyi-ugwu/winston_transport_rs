use crate::log_query::LogQuery;
use logform::{Format, LogInfo};
use std::any::Any;

pub trait Transport: Any + Send + Sync {
    fn log(&self, info: LogInfo);
    fn get_level(&self) -> Option<&String> {
        None
    }
    fn get_format(&self) -> Option<&Format> {
        None
    }
    fn as_any(&self) -> &dyn Any;
    fn as_queryable(&self) -> Option<&dyn Queryable> {
        None
    }
}

pub trait Queryable: Any + Send + Sync {
    fn query(&self, query: &LogQuery) -> Result<Vec<LogInfo>, String>;
}
