# winston_transport

A Rust crate providing a winston-transport implementation for Rust applications. This crate offers logging transport capabilities inspired by the popular Node.js winston logging library, enabling flexible and extensible logging solutions.

## Features

- Logging transport implementation compatible with Rust logging ecosystems.
- Includes modules for query building (`query_dsl`) with a type-safe, composable DSL for filtering structured data (e.g., `serde_json::Value`).
- Supports complex query construction inspired by MongoDB query syntax.
- Provides various transport adapters and writer transports for flexible logging output.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
winston_transport = "0.4.0"
```

Then run:

```bash
cargo build
```

## Usage

Basic usage example:

```rust
use winston_transport::{Transport, WriterTransport, LogQuery, Order};
use logform::{Format, LogInfo};

fn main() {
    // Example: create a transport and use it for logging
    let transport = WriterTransport::new();
    // Use transport as needed...

    // Example: create a log query
    let query = LogQuery::new().order(Order::Ascending);
    // Use query to filter logs...
}
```

### Using the `query_dsl` Module

The `query_dsl` module provides a domain-specific language for building type-safe, composable queries against structured data like `serde_json::Value`.

Example:

```rust
use query_dsl::prelude::*;
use serde_json::json;

fn main() {
    let query = and!(
        field_query!("user.age", gt(18)),
        or!(
            field_query!("user.status", eq("active")),
            field_query!("user.role", eq("admin"))
        )
    );

    let data1 = json!({ "user": { "age": 25, "status": "active" } });
    let data2 = json!({ "user": { "age": 30, "role": "admin" } });
    let data3 = json!({ "user": { "age": 15, "status": "inactive" } });

    println!("Data 1 matches: {}", query.evaluate(&data1)); // true
    println!("Data 2 matches: {}", query.evaluate(&data2)); // true
    println!("Data 3 matches: {}", query.evaluate(&data3)); // false
}
```

## Documentation

For detailed documentation, visit [https://docs.rs/winston_transport](https://docs.rs/winston_transport).

## Repository

Source code and issue tracking available at [https://github.com/ifeanyi-ugwu/winston_transport_rs](https://github.com/ifeanyi-ugwu/winston_transport_rs).

## License

This project is licensed under the MIT License.

## Author

ifeanyi ugwu
