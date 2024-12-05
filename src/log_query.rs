use chrono::{DateTime, Duration, Utc};
use dateparser::parse;
use logform::LogInfo;
use parse_datetime::parse_datetime;
use serde_json::Value;
use std::str::FromStr;

pub struct LogQuery {
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub start: Option<usize>,
    pub order: Order,
    pub levels: Vec<String>,
    pub fields: Vec<String>,
    pub search_term: Option<String>,
}

pub enum Order {
    Ascending,
    Descending,
}

impl FromStr for Order {
    type Err = String;

    fn from_str(input: &str) -> Result<Order, Self::Err> {
        match input.to_lowercase().as_str() {
            "asc" | "ascending" => Ok(Order::Ascending),
            "desc" | "descending" => Ok(Order::Descending),
            _ => Err(format!("Invalid order: {}", input)),
        }
    }
}

impl LogQuery {
    pub fn new() -> Self {
        LogQuery {
            from: Some(Utc::now() - Duration::days(1)),
            until: Some(Utc::now()),
            limit: Some(50),
            start: Some(0),
            order: Order::Descending,
            fields: Vec::new(),
            levels: Vec::new(),
            search_term: None,
        }
    }

    fn parse_time(time_str: &str) -> Option<DateTime<Utc>> {
        match parse_datetime(time_str) {
            Ok(parsed_date) => Some(parsed_date.with_timezone(&Utc)),
            Err(_) => None,
        }
    }

    pub fn from<S: AsRef<str>>(mut self, from: S) -> Self {
        self.from = LogQuery::parse_time(from.as_ref());
        self
    }

    pub fn until<S: AsRef<str>>(mut self, until: S) -> Self {
        self.until = LogQuery::parse_time(until.as_ref());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn start(mut self, start: usize) -> Self {
        self.start = Some(start);
        self
    }

    pub fn order<S: Into<String>>(mut self, order: S) -> Self {
        let order_str = order.into();
        self.order = Order::from_str(&order_str).unwrap_or(Order::Descending);
        self
    }

    pub fn levels<S: Into<String>>(mut self, levels: Vec<S>) -> Self {
        self.levels = levels.into_iter().map(Into::into).collect();
        self
    }

    pub fn fields<S: Into<String>>(mut self, fields: Vec<S>) -> Self {
        self.fields = fields.into_iter().map(Into::into).collect();
        self
    }

    pub fn search_term<S: Into<String>>(mut self, search_term: S) -> Self {
        self.search_term = Some(search_term.into());
        self
    }

    fn extract_timestamp(entry: &LogInfo) -> Option<DateTime<Utc>> {
        if let Some(Value::String(ts_str)) = entry.get_meta("timestamp") {
            //println!("Found timestamp string: {}", ts_str);

            match parse(ts_str) {
                Ok(parsed_date) => {
                    //println!("Parsed date: {}", parsed_date);
                    return Some(parsed_date.with_timezone(&Utc));
                }
                Err(_e) => {
                    //eprintln!("Failed to parse timestamp '{}': {}", ts_str, e);
                    return None;
                }
            }
        }
        None
    }

    pub fn matches(&self, entry: &LogInfo) -> bool {
        //println!("checking entry: {:?}", entry);
        // Check level
        if !self.levels.is_empty() && !self.levels.contains(&entry.level) {
            //println!("failed at levels check");
            return false;
        }

        // Check timestamp
        if let Some(from) = self.from {
            if let Some(timestamp) = Self::extract_timestamp(entry) {
                if timestamp < from {
                    //println!("failed at from check");
                    return false;
                }
            } else {
                //println!("failed at from check");
                return false;
            }
        }

        if let Some(until) = self.until {
            if let Some(timestamp) = Self::extract_timestamp(entry) {
                if timestamp > until {
                    //println!("failed at until check");
                    return false;
                }
            } else {
                //println!("failed at until check");
                return false;
            }
        }

        // Check search term in message
        if let Some(ref search_term) = self.search_term {
            if !entry.message.contains(search_term) {
                //println!("failed at search term check");
                return false;
            }
        }

        // Check fields in meta data
        for field in &self.fields {
            if !entry.meta.contains_key(field) {
                //println!("failed at field check");
                return false;
            }
        }

        true
    }

    pub fn sort(&self, entries: &mut Vec<LogInfo>) {
        match self.order {
            Order::Ascending => {
                entries.sort_by(|a, b| Self::extract_timestamp(a).cmp(&Self::extract_timestamp(b)))
            }
            Order::Descending => {
                entries.sort_by(|a, b| Self::extract_timestamp(b).cmp(&Self::extract_timestamp(a)))
            }
        }
    }
}
