use crate::log_query::LogQuery;
use logform::{Format, LogInfo};
use std::any::Any;

pub trait Transport: Any + Send + Sync {
    fn log(&self, info: LogInfo);
    fn get_level(&self) -> Option<&String>;
    fn get_format(&self) -> Option<&Format>;
    fn as_any(&self) -> &dyn Any;
    fn as_queryable(&self) -> Option<&dyn Queryable> {
        None
    }
}

pub struct TransportStreamOptions {
    pub level: Option<String>,
    pub format: Option<Format>,
}

pub trait Queryable: Any + Send + Sync {
    fn query(&self, query: &LogQuery) -> Result<Vec<LogInfo>, String>;
}
