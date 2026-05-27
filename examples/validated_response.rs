//! Response validation with garde (requires feature `validate`).
//!
//! Run: `cargo run -p better-fetch --example validated_response --features validate`

use better_fetch::{Client, Result};
use garde::Validate;
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
#[expect(dead_code)]
struct Todo {
    #[garde(range(min = 1))]
    id: u64,
    #[garde(length(min = 1))]
    title: String,
    #[garde(skip)]
    completed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://jsonplaceholder.typicode.com")?;

    let todo: Todo = client
        .get("/todos/:id")
        .param("id", 1)
        .send_json_validated()
        .await?;

    println!("{todo:#?}");
    Ok(())
}
