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

impl From<&str> for Order {
    fn from(s: &str) -> Self {
        Order::from_str(s).unwrap_or(Order::Descending)
    }
}

impl From<String> for Order {
    fn from(s: String) -> Self {
        Order::from_str(&s).unwrap_or(Order::Descending)
    }
}

impl From<i8> for Order {
    fn from(n: i8) -> Self {
        if n == 1 {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}
impl From<i16> for Order {
    fn from(n: i16) -> Self {
        if n == 1 {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

impl From<i32> for Order {
    fn from(n: i32) -> Self {
        if n == 1 {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

impl From<i64> for Order {
    fn from(n: i64) -> Self {
        if n == 1 {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

impl From<i128> for Order {
    fn from(n: i128) -> Self {
        if n == 1 {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

impl From<isize> for Order {
    fn from(n: isize) -> Self {
        if n == 1 {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

// Helper trait to allow conversion from various types to Option<DateTime<Utc>>
pub trait IntoDateTimeOption {
    fn into_datetime_option(self) -> Option<DateTime<Utc>>;
}

impl IntoDateTimeOption for DateTime<Utc> {
    fn into_datetime_option(self) -> Option<DateTime<Utc>> {
        Some(self)
    }
}

impl IntoDateTimeOption for &str {
    fn into_datetime_option(self) -> Option<DateTime<Utc>> {
        LogQuery::parse_time(self)
    }
}

impl IntoDateTimeOption for String {
    fn into_datetime_option(self) -> Option<DateTime<Utc>> {
        LogQuery::parse_time(&self)
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

    pub fn from<T: IntoDateTimeOption>(mut self, from: T) -> Self {
        self.from = from.into_datetime_option();
        self
    }

    pub fn until<T: IntoDateTimeOption>(mut self, until: T) -> Self {
        self.until = until.into_datetime_option();
        self
    }

    /*pub fn from_datetime<T: Into<DateTime<Utc>>>(mut self, from_time: T) -> Self {
        self.from = Some(from_time.into());
        self
    }

    pub fn until_datetime<T: Into<DateTime<Utc>>>(mut self, until_time: T) -> Self {
        self.until = Some(until_time.into());
        self
    }*/

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn start(mut self, start: usize) -> Self {
        self.start = Some(start);
        self
    }

    pub fn order<S: Into<Order>>(mut self, order: S) -> Self {
        self.order = order.into();
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
        self.search_term = Some(Regex::new(search_term.as_ref()).unwrap());
        self
    }

    fn extract_timestamp(entry: &LogInfo) -> Option<DateTime<Utc>> {
        entry.meta.get("timestamp").and_then(|value| match value {
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
