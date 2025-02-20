use chrono::{DateTime, Duration, Utc};
use dateparser::parse;
use logform::LogInfo;
use parse_datetime::parse_datetime;
use regex::Regex;
use serde_json::Value;
use std::str::FromStr;

// todo: the matches, extract_timestamp, and sort methods and functions was
// created as a result of the FileTransport, there is a high chance it wont be used else where
// this is as observed when creating the `MongoDDTransport`
pub struct LogQuery {
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub start: Option<usize>,
    pub order: Order,
    pub levels: Vec<String>,
    pub fields: Vec<String>,
    pub search_term: Option<Regex>,
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
        parse_datetime(time_str)
            .ok()
            .map(|parsed_date| parsed_date.with_timezone(&Utc))
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

    pub fn order<S: AsRef<str>>(mut self, order: S) -> Self {
        self.order = Order::from_str(order.as_ref()).unwrap_or(Order::Descending);
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

    pub fn search_term<S: AsRef<str>>(mut self, search_term: S) -> Self {
        self.search_term = Regex::new(&search_term.as_ref()).ok();
        self
    }

    fn extract_timestamp(entry: &LogInfo) -> Option<DateTime<Utc>> {
        entry.get_meta("timestamp").and_then(|value| match value {
            Value::String(ts_str) => parse(&ts_str).ok().map(|dt| dt.with_timezone(&Utc)),
            _ => None,
        })
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
        if let Some(ref regex) = self.search_term {
            if !regex.is_match(&entry.message) {
                return false;
            }
        }

        // Check fields in meta data
        /*for field in &self.fields {
            // Check if the field exists in either meta or as a top-level attribute
            let field_exists = match field.as_str() {
                "message" => true, // message always exists
                "level" => true,   // level always exists
                _ => entry.meta.contains_key(field),
            };

            if !field_exists {
                //println!("failed at field check");
                return false;
            }
        }*/

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
