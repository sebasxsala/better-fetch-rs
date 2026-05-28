//! Validate a JSON request body before send (feature `validate`).
//!
//! ```bash
//! cargo run -p better-fetch --example validated_request --features validate
//! ```

use better_fetch::{ClientBuilder, Result};
use garde::Validate;
use serde::Serialize;

#[derive(Serialize, Validate)]
struct NewTodo {
    #[garde(length(min = 1))]
    title: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new()
        .base_url("https://jsonplaceholder.typicode.com")?
        .build()?;

    let todo = client
        .post("/todos")
        .json_validated(&NewTodo {
            title: "from better-fetch".into(),
        })?
        .send()
        .await?;

    println!("created status = {}", todo.status());
    Ok(())
}
