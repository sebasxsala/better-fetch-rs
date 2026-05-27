use std::time::Duration;

use better_fetch::{Client, Result, RetryPolicy};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
struct Todo {
    id: u64,
    title: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder()
        .base_url("https://jsonplaceholder.typicode.com")?
        .retry(RetryPolicy::linear(3, Duration::from_secs(1)))
        .build()?;

    let todo: Todo = client.get("/todos/:id").param("id", 1).send_json().await?;

    println!("{todo:#?}");
    Ok(())
}
